//! T051: enrollment tokens and enrollments (FR-007 to FR-011, FR-013, FR-015, FR-034, SC-007).

mod common;

use std::sync::Arc;

use common::*;
use hecate_application::identity::service::CallRefusal;
use hecate_application::identity::settings::IdentitySettings;
use hecate_application::ports::{CertificateProfile, Clock, EnrollAttemptsRepository, EventStore};
use hecate_application::testing::{CaRig, FakeClock, GOOD_PASSPHRASE, initialize};
use hecate_domain::events::{CertificateEvent, CertificateOrigin, IdentityEvent};
use hecate_domain::identity::certificate::Certificate;
use hecate_domain::identity::identity::Identity;
use hecate_domain::identity::names::{IdentityName, RootFingerprint, TokenId};
use hecate_domain::identity::token::EnrollmentToken;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_domain::sensitive::SecretKey;
use hecate_infrastructure::testing::{SqliteStack, short_dir};
use serde_json::json;
use time::Duration;

fn rows(rig: &CaRig, operation: Operation) -> Vec<AccessLogEntry> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .collect()
}

fn is_auth(error: &hecate_application::errors::ServiceError) -> bool {
    matches!(
        error,
        hecate_application::errors::ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    )
}

fn identity_of(rig: &CaRig, name: &str) -> Identity {
    let records = rig.store.load(&format!("identity/{name}")).unwrap();
    let events: Vec<IdentityEvent> = records
        .iter()
        .map(|record| IdentityEvent::decode(record).unwrap())
        .collect();
    Identity::from_events(IdentityName::parse(name).unwrap(), &events).unwrap()
}

fn certificate_of(rig: &CaRig, name: &str, serial: &str) -> Certificate {
    let records = rig
        .store
        .load(&format!("certificate/{name}/{serial}"))
        .unwrap();
    let events: Vec<CertificateEvent> = records
        .iter()
        .map(|record| CertificateEvent::decode(record).unwrap())
        .collect();
    Certificate::from_events(&events).unwrap()
}

fn pending_ids(rig: &CaRig) -> Vec<String> {
    rig.identities
        .tokens_list()
        .unwrap()
        .iter()
        .map(|token| token.token_id.to_string())
        .collect()
}

fn token_id_of(token: &str) -> String {
    EnrollmentToken::parse(token).unwrap().id().to_string()
}

// --- Tokens --------------------------------------------------------------------------------------

#[test]
fn tokens_need_a_ca_and_hecate_unsealed() {
    let without_ca = ca_rig();
    let error = without_ca.identities.tokens_create("hermes").err().unwrap();
    assert!(is_conflict(&error), "{error:?}");
    let (rig, _) = created_ca();
    rig.sealing.seal().unwrap();
    let sealed = rig.identities.tokens_create("hermes").err().unwrap();
    assert_eq!(conflict_state(&sealed), Some(SealState::Sealed));
}

#[test]
fn reserved_or_malformed_names_are_validation_and_disabled_or_an_expired_intermediate_conflict() {
    let (rig, _) = created_ca();
    for name in ["hecate", "admin", "Bad Name", ""] {
        let error = rig.identities.tokens_create(name).err().unwrap();
        assert!(is_invalid(&error), "{name}: {error:?}");
    }
    enroll_new(&rig, "hermes", "k1");
    let now = rig.clock.now();
    append_identity_events(
        &rig,
        "hermes",
        &[IdentityEvent::IdentityDisabled {
            at: now,
            urgent: false,
        }],
    );
    let disabled = rig.identities.tokens_create("hermes").err().unwrap();
    assert!(is_conflict(&disabled), "{disabled:?}");
    rig.clock.advance(Duration::days(366));
    let expired = rig.identities.tokens_create("hades").err().unwrap();
    assert!(is_conflict(&expired), "{expired:?}");
}

#[test]
fn the_token_is_shown_once_and_only_the_hash_of_its_secret_is_kept() {
    let (rig, _) = created_ca();
    let now = rig.clock.now();
    let created = rig.identities.tokens_create("hermes").unwrap();
    let token = EnrollmentToken::parse(&created.token).unwrap();
    assert_eq!(token.identity().as_str(), "hermes");
    assert_eq!(token.id(), &created.token_id);
    assert_eq!(created.expires_at, now + Duration::minutes(15));
    let summary = rig.service.status().unwrap().unwrap();
    assert_eq!(token.fingerprint(), &summary.root_fingerprint);
    let records = rig.store.load("identity/hermes").unwrap();
    assert_eq!(
        IdentityEvent::decode(&records[0]).unwrap(),
        IdentityEvent::EnrollmentTokenCreated {
            at: now,
            token_id: created.token_id.clone(),
            secret_sha256: token.secret_sha256(),
            generation: 1,
            expires_at: now + Duration::minutes(15),
        }
    );
    let secret = created.token.split('.').nth(3).unwrap().to_string();
    let contents = rig.store.contents();
    assert!(!contains(&contents, created.token.as_bytes()));
    assert!(!contains(&contents, secret.as_bytes()));
    for row in rig.access_log.entries() {
        assert!(!format!("{row:?}").contains(&secret));
    }
    let row = rows(&rig, Operation::TokenCreate).pop().unwrap();
    assert_eq!(
        (row.result, row.subject.as_deref(), row.identity.as_deref()),
        (AccessResult::Success, Some("admin"), Some("hermes"))
    );
}

#[test]
fn the_lifetime_of_tokens_comes_from_the_settings() {
    let rig = ca_rig_with(IdentitySettings::new(30));
    create_ca_on(&rig);
    let now = rig.clock.now();
    let created = rig.identities.tokens_create("hermes").unwrap();
    assert_eq!(created.expires_at, now + Duration::minutes(30));
}

#[test]
fn the_list_shows_the_pending_tokens_without_their_value_and_annulling_removes_them() {
    let (rig, _) = created_ca();
    let hermes = new_token(&rig, "hermes");
    let hades = new_token(&rig, "hades");
    let listed = rig.identities.tokens_list().unwrap();
    assert_eq!(listed.len(), 2);
    for token in &listed {
        assert!(!format!("{token:?}").contains(hermes.split('.').nth(3).unwrap()));
    }
    let annulled = rig.identities.tokens_annul(&token_id_of(&hermes)).unwrap();
    assert_eq!(annulled.to_string(), token_id_of(&hermes));
    assert_eq!(pending_ids(&rig), vec![token_id_of(&hades)]);
    // Annulling it again changes nothing and is accepted (FR-026).
    let events_before = rig.store.load("identity/hermes").unwrap().len();
    rig.identities.tokens_annul(&token_id_of(&hermes)).unwrap();
    assert_eq!(
        rig.store.load("identity/hermes").unwrap().len(),
        events_before
    );
    let unknown = rig
        .identities
        .tokens_annul(&TokenId::from_bytes([9; 8]).to_string())
        .err()
        .unwrap();
    assert!(is_not_found(&unknown), "{unknown:?}");
    let malformed = rig.identities.tokens_annul("nope").err().unwrap();
    assert!(is_invalid(&malformed), "{malformed:?}");
    let annul_rows = rows(&rig, Operation::TokenAnnul);
    assert_eq!(annul_rows[0].identity.as_deref(), Some("hermes"));
    assert_eq!(rows(&rig, Operation::TokenList).len(), 2);
}

// --- Enrollment ----------------------------------------------------------------------------------

#[test]
fn a_valid_enrollment_issues_a_3_day_certificate_with_only_the_identity() {
    let (rig, signed) = created_ca();
    let now = rig.clock.now();
    let token = new_token(&rig, "hermes");
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    let enrolled = rig.identities.enroll(Some("k1"), &body).unwrap();
    assert_eq!(enrolled.identity, spiffe("hermes"));
    assert_eq!(enrolled.not_after, now + Duration::hours(72));
    assert_eq!(
        enrolled.chain,
        vec![signed.intermediate_der.clone(), signed.root_der.clone()]
    );
    let profile = CertificateProfile::Leaf {
        spiffe_id: spiffe("hermes"),
    };
    assert!(
        rig.pki
            .follows_profile(&enrolled.certificate, &profile)
            .unwrap()
    );
    assert!(
        rig.pki
            .is_signed_by(&enrolled.certificate, &signed.intermediate_der)
            .unwrap()
    );
    let call_key = rig.pki.read_call_key(&call_key_pem(1)).unwrap();
    assert_eq!(enrolled.call_key_id, call_key.kid());

    let identity = identity_of(&rig, "hermes");
    assert_eq!(identity.epoch, 1);
    assert!(pending_ids(&rig).is_empty());
    let certificate = certificate_of(&rig, "hermes", enrolled.serial.as_str());
    let issued = &certificate.issued;
    assert_eq!(issued.origin, CertificateOrigin::Enrollment);
    assert_eq!(
        (issued.generation, issued.intermediate_version, issued.epoch),
        (1, 1, 1)
    );
    assert_eq!(issued.call_key.as_ref(), Some(&call_key));
    assert_eq!(issued.idempotency_key.as_deref(), Some("k1"));
    assert!(issued.request_sha256.is_some());
    assert_eq!(issued.certificate, enrolled.certificate);

    let row = rows(&rig, Operation::IdentityEnroll).pop().unwrap();
    assert_eq!(row.method, Method::EnrollmentToken);
    assert_eq!(row.result, AccessResult::Success);
    assert_eq!(row.subject.as_deref(), Some("spiffe://cerbero.test/hermes"));
    assert_eq!(row.identity.as_deref(), Some("hermes"));
    assert_eq!(row.certificate.as_deref(), Some(enrolled.serial.as_str()));
}

#[test]
fn the_exact_repetition_returns_the_same_certificate_and_another_request_is_auth() {
    let (rig, _) = created_ca();
    let token = new_token(&rig, "hermes");
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    let first = rig.identities.enroll(Some("k1"), &body).unwrap();
    let events = rig.store.load("identity/hermes").unwrap().len();
    let again = rig.identities.enroll(Some("k1"), &body).unwrap();
    assert_eq!(again.serial, first.serial);
    assert_eq!(again.certificate, first.certificate);
    assert_eq!(rig.store.load("identity/hermes").unwrap().len(), events);
    assert_eq!(rig.store.streams("certificate/hermes/").unwrap().len(), 1);
    let repeat = rows(&rig, Operation::IdentityEnroll).pop().unwrap();
    assert_eq!(
        (repeat.result, repeat.detail.as_str()),
        (AccessResult::Success, "repeat")
    );
    let other = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    let refused = rig.identities.enroll(Some("k1"), &other).err().unwrap();
    assert!(is_auth(&refused), "{refused:?}");
}

#[test]
fn tokens_that_are_not_valid_are_auth_and_leave_a_row() {
    let (rig, _) = created_ca();
    let csr = request_for(&rig, Some("hermes"));
    let expired = new_token(&rig, "hermes");
    let annulled = new_token(&rig, "hermes");
    rig.identities
        .tokens_annul(&token_id_of(&annulled))
        .unwrap();
    // A token of another generation of the CA, as one left from before a replacement.
    let fingerprint = rig.service.status().unwrap().unwrap().root_fingerprint;
    let old = EnrollmentToken::new(
        IdentityName::parse("hermes").unwrap(),
        TokenId::from_bytes([7; 8]),
        SecretKey::new([7; 32]),
        fingerprint,
    );
    append_identity_events(
        &rig,
        "hermes",
        &[IdentityEvent::EnrollmentTokenCreated {
            at: rig.clock.now(),
            token_id: old.id().clone(),
            secret_sha256: old.secret_sha256(),
            generation: 2,
            expires_at: rig.clock.now() + Duration::minutes(15),
        }],
    );
    // A well-formed token whose secret Hécate never issued.
    let forged = EnrollmentToken::new(
        IdentityName::parse("hermes").unwrap(),
        EnrollmentToken::parse(&expired).unwrap().id().clone(),
        SecretKey::new([8; 32]),
        fingerprint,
    );
    let forged_text = forged.text().to_string();
    let old_text = old.text().to_string();
    for token in [annulled.as_str(), old_text.as_str(), forged_text.as_str()] {
        let error = rig
            .identities
            .enroll(Some("k1"), &enroll_body(token, &csr, &call_key_pem(1)))
            .err()
            .unwrap();
        assert!(is_auth(&error), "{error:?}");
    }
    rig.clock.advance(Duration::minutes(15));
    let late = rig
        .identities
        .enroll(Some("k1"), &enroll_body(&expired, &csr, &call_key_pem(1)))
        .err()
        .unwrap();
    assert!(is_auth(&late), "{late:?}");
    let refused = rows(&rig, Operation::IdentityEnroll);
    assert_eq!(refused.len(), 4);
    assert!(
        refused
            .iter()
            .all(|row| row.result == AccessResult::Failure)
    );
    assert!(
        refused
            .iter()
            .all(|row| row.identity.as_deref() == Some("hermes"))
    );
    assert_eq!(refused[2].subject, None, "a forged token is not recognized");
    assert!(rig.store.streams("certificate/hermes/").unwrap().is_empty());
}

#[test]
fn another_identity_or_a_malformed_pem_is_validation_and_leaves_the_token_unused() {
    let (rig, _) = created_ca();
    let token = new_token(&rig, "hermes");
    let csr = request_for(&rig, Some("hermes"));
    let good_key = call_key_pem(1);
    for body in [
        enroll_body(&token, &request_for(&rig, Some("hades")), &good_key),
        enroll_body(&token, &format!("{csr}{csr}"), &good_key),
        enroll_body(&token, &csr, &csr),
        enroll_body("HET1.hermes", &csr, &good_key),
    ] {
        let error = rig.identities.enroll(Some("k1"), &body).err().unwrap();
        assert!(is_invalid(&error), "{body}: {error:?}");
    }
    assert_eq!(pending_ids(&rig), vec![token_id_of(&token)]);
    let no_name = enroll_body(&token, &request_for(&rig, None), &good_key);
    let enrolled = rig.identities.enroll(Some("k1"), &no_name).unwrap();
    assert_eq!(enrolled.identity, spiffe("hermes"));
}

#[test]
fn a_body_with_other_fields_or_a_bad_idempotency_key_is_validation() {
    let (rig, _) = created_ca();
    let token = new_token(&rig, "hermes");
    let csr = request_for(&rig, Some("hermes"));
    let good_key = call_key_pem(1);
    for (key, body) in [
        (Some("k1"), json!({"token": token, "csr": csr})),
        (
            Some("k1"),
            json!({"token": token, "csr": csr, "call_key": good_key, "more": 1}),
        ),
        (
            Some("k1"),
            json!({"token": token, "csr": 7, "call_key": good_key}),
        ),
        (Some("k1"), json!(["not", "an", "object"])),
        (None, enroll_body(&token, &csr, &good_key)),
        (Some(""), enroll_body(&token, &csr, &good_key)),
        (Some("\u{e9}t\u{e9}"), enroll_body(&token, &csr, &good_key)),
    ] {
        let error = rig.identities.enroll(key, &body).err().unwrap();
        assert!(is_invalid(&error), "{key:?} {body}: {error:?}");
    }
    assert_eq!(pending_ids(&rig), vec![token_id_of(&token)]);
    let refused = rows(&rig, Operation::IdentityEnroll);
    assert!(
        refused
            .iter()
            .all(|row| row.result == AccessResult::InvalidInput)
    );
}

#[test]
fn a_new_enrollment_leaves_the_previous_certificates_without_renewal() {
    let (rig, _) = created_ca();
    let first = enroll_new(&rig, "hermes", "k1");
    let second = enroll_new(&rig, "hermes", "k2");
    let identity = identity_of(&rig, "hermes");
    assert_eq!(identity.epoch, 2);
    let now = rig.clock.now();
    let enabled = true;
    assert!(
        !certificate_of(&rig, "hermes", first.serial.as_str()).renewable(
            now,
            1,
            identity.epoch,
            enabled
        )
    );
    assert!(
        certificate_of(&rig, "hermes", second.serial.as_str()).renewable(
            now,
            1,
            identity.epoch,
            enabled
        )
    );
}

#[test]
fn two_enrollments_at_once_with_the_same_token_give_one_certificate() {
    let (rig, _) = created_ca();
    let rig = Arc::new(rig);
    let token = new_token(&rig, "hermes");
    let bodies: Vec<_> = (0..2)
        .map(|byte| {
            enroll_body(
                &token,
                &request_for(&rig, Some("hermes")),
                &call_key_pem(byte),
            )
        })
        .collect();
    let handles: Vec<_> = bodies
        .into_iter()
        .map(|body| {
            let rig = rig.clone();
            std::thread::spawn(move || rig.identities.enroll(Some("k1"), &body))
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .any(|result| result.as_ref().is_err_and(is_auth))
    );
    assert_eq!(rig.store.streams("certificate/hermes/").unwrap().len(), 1);
}

#[test]
fn an_idempotency_key_reused_with_other_content_is_a_conflict_within_its_identity() {
    let (rig, _) = created_ca();
    enroll_new(&rig, "hermes", "k1");
    let token = new_token(&rig, "hermes");
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(2));
    let reused = rig.identities.enroll(Some("k1"), &body).err().unwrap();
    assert!(is_conflict(&reused), "{reused:?}");
    assert_eq!(pending_ids(&rig), vec![token_id_of(&token)]);
    rig.identities.enroll(Some("k2"), &body).unwrap();
    enroll_new(&rig, "hades", "k1");
}

// --- Throttle ------------------------------------------------------------------------------------

fn fail(rig: &CaRig) {
    let error = rig
        .identities
        .enroll(Some("k1"), &enroll_body("HET1.bad", "x", "y"))
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
}

#[test]
fn the_throttle_refuses_after_ten_failures_and_leaves_one_row_per_batch() {
    let (rig, _) = created_ca();
    let token = new_token(&rig, "hermes");
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    for _ in 0..10 {
        fail(&rig);
    }
    assert!(rig.identities.enrollment_throttled().unwrap());
    for _ in 0..2 {
        let throttled = rig.identities.enroll(Some("k1"), &body).err().unwrap();
        assert_eq!(retry_after(&throttled), Some(60), "{throttled:?}");
    }
    assert_eq!(rows(&rig, Operation::IdentityEnroll).len(), 10);
    assert!(rows(&rig, Operation::IdentityEnrollThrottled).is_empty());
    let status = rig.sealing.status().unwrap();
    assert_eq!((status.consecutive_failures, status.retry_after_s), (0, 0));

    rig.clock.advance(Duration::seconds(60));
    assert!(!rig.identities.enrollment_throttled().unwrap());
    rig.identities.enroll(Some("k1"), &body).unwrap();
    let batches = rows(&rig, Operation::IdentityEnrollThrottled);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].method, Method::Internal);
    assert_eq!(batches[0].detail, "failures 10, rejected 2");
    assert_eq!(rig.attempts.load().unwrap().consecutive_failures, 0);
}

#[test]
fn sealed_or_throttled_refusals_are_no_failures_and_the_task_closes_batches() {
    let (rig, _) = created_ca();
    rig.sealing.seal().unwrap();
    let sealed = rig
        .identities
        .enroll(Some("k1"), &enroll_body("HET1.bad", "x", "y"))
        .err()
        .unwrap();
    assert!(is_conflict(&sealed), "{sealed:?}");
    assert_eq!(rig.attempts.load().unwrap().consecutive_failures, 0);
    // T127 (FR-024): the refusal leaves its row, without a subject: nothing was checked.
    let refused = rows(&rig, Operation::IdentityEnroll);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(
        (
            refused[0].method,
            refused[0].result,
            refused[0].detail.as_str(),
            refused[0].subject.as_deref(),
            refused[0].identity.as_deref(),
        ),
        (
            Method::EnrollmentToken,
            AccessResult::RejectedConflict,
            "sealed",
            None,
            None
        )
    );
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    for _ in 0..10 {
        fail(&rig);
    }
    let throttled = rig
        .identities
        .enroll(Some("k1"), &enroll_body("HET1.bad", "x", "y"))
        .err()
        .unwrap();
    assert_eq!(retry_after(&throttled), Some(60));
    assert_eq!(rig.attempts.load().unwrap().consecutive_failures, 10);
    rig.identities.close_throttle_batch().unwrap();
    assert!(rows(&rig, Operation::IdentityEnrollThrottled).is_empty());
    rig.clock.advance(Duration::seconds(61));
    rig.identities.close_throttle_batch().unwrap();
    rig.identities.close_throttle_batch().unwrap();
    let batches = rows(&rig, Operation::IdentityEnrollThrottled);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].detail, "failures 10, rejected 1");
}

/// T129 (FR-034): an enrollment refused for its request id goes through the throttle, like one
/// with a malformed Idempotency-Key. Sealed, it only leaves its row; other operations never count.
#[test]
fn an_enrollment_refused_for_its_request_id_goes_through_the_throttle() {
    let (rig, _) = created_ca();
    let refuse = |operation: Operation, method: Method| {
        rig.identities
            .record_refused_call(operation, method, CallRefusal::RequestId)
    };
    refuse(Operation::KeyList, Method::Identity).unwrap();
    assert_eq!(rig.attempts.load().unwrap().consecutive_failures, 0);
    for _ in 0..10 {
        refuse(Operation::IdentityEnroll, Method::EnrollmentToken).unwrap();
    }
    assert_eq!(rig.attempts.load().unwrap().consecutive_failures, 10);
    assert_eq!(rows(&rig, Operation::IdentityEnroll).len(), 10);
    let throttled = refuse(Operation::IdentityEnroll, Method::EnrollmentToken)
        .err()
        .unwrap();
    assert_eq!(retry_after(&throttled), Some(60));
    assert_eq!(
        rows(&rig, Operation::IdentityEnroll).len(),
        10,
        "counted in the batch, without a row of its own"
    );
    rig.clock.advance(Duration::seconds(61));
    rig.identities.close_throttle_batch().unwrap();
    let batches = rows(&rig, Operation::IdentityEnrollThrottled);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].detail, "failures 10, rejected 1");
    let failures = rig.attempts.load().unwrap().consecutive_failures;
    rig.sealing.seal().unwrap();
    refuse(Operation::IdentityEnroll, Method::EnrollmentToken).unwrap();
    assert_eq!(
        rig.attempts.load().unwrap().consecutive_failures,
        failures,
        "sealed, it does not count"
    );
    assert_eq!(rows(&rig, Operation::IdentityEnroll).len(), 11);
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

#[test]
fn a_power_cut_during_the_enrollment_leaves_nothing_and_the_retry_issues() {
    let (rig, _) = created_ca();
    let token = new_token(&rig, "hermes");
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    rig.store.fail_commits(true);
    assert!(rig.identities.enroll(Some("k1"), &body).is_err());
    assert!(rig.store.streams("certificate/hermes/").unwrap().is_empty());
    assert_eq!(rig.store.load("identity/hermes").unwrap().len(), 1);
    assert_eq!(pending_ids(&rig), vec![token_id_of(&token)]);
    rig.store.fail_commits(false);
    let enrolled = rig.identities.enroll(Some("k1"), &body).unwrap();
    assert_eq!(enrolled.identity, spiffe("hermes"));
}

#[test]
fn a_token_fingerprint_names_the_current_root_ca() {
    let (rig, signed) = created_ca();
    let token = new_token(&rig, "hermes");
    assert_eq!(
        EnrollmentToken::parse(&token).unwrap().fingerprint(),
        &RootFingerprint::of_der(&signed.root_der)
    );
}

#[test]
fn the_throttle_survives_reopening_the_database_during_a_wait() {
    let dir = short_dir();
    let path = dir.path().join("hecate.db");
    let clock = Arc::new(FakeClock::new());
    let stack = SqliteStack::open(&path, clock.clone());
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    for _ in 0..10 {
        let error = stack
            .identities
            .enroll(Some("k1"), &enroll_body("HET1.bad", "x", "y"))
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{error:?}");
    }
    stack.close();
    let reopened = SqliteStack::open(&path, clock);
    reopened
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    assert!(reopened.identities.enrollment_throttled().unwrap());
    let throttled = reopened
        .identities
        .enroll(Some("k1"), &enroll_body("HET1.bad", "x", "y"))
        .err()
        .unwrap();
    assert_eq!(retry_after(&throttled), Some(60));
    reopened.close();
}
