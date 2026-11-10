//! T071: the renewal of a certificate (FR-014 to FR-016, FR-021, FR-025, SC-006, SC-007): once,
//! with the current certificate and a short call token; a second attempt revokes urgently every
//! valid certificate of the identity.

mod common;

use std::sync::Arc;

use common::*;
use hecate_application::identity::service::Credentials;
use hecate_application::ports::{Clock, EventStore, IdentityStore};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::events::{
    CertificateEvent, CertificateOrigin, IdentityEvent, RevocationReason, RevokedBy,
};
use hecate_domain::identity::certificate::{Certificate, CertificateState};
use hecate_domain::identity::names::Serial;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::testing::read_crl;
use serde_json::json;
use time::Duration;

fn is_auth(error: &hecate_application::errors::ServiceError) -> bool {
    matches!(
        error,
        hecate_application::errors::ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    )
}

fn rows(rig: &CaRig, operation: Operation) -> Vec<AccessLogEntry> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .collect()
}

fn certificate(rig: &CaRig, identity: &str, serial: &Serial) -> Certificate {
    let records = rig
        .store
        .load(&format!("certificate/{identity}/{serial}"))
        .unwrap();
    let events: Vec<CertificateEvent> = records
        .iter()
        .map(|record| CertificateEvent::decode(record).unwrap())
        .collect();
    Certificate::from_events(&events).unwrap()
}

fn certificate_events(rig: &CaRig, identity: &str, serial: &Serial) -> usize {
    rig.store
        .load(&format!("certificate/{identity}/{serial}"))
        .unwrap()
        .len()
}

fn append_certificate_event(rig: &CaRig, identity: &str, serial: &Serial, event: CertificateEvent) {
    let stream = format!("certificate/{identity}/{serial}");
    rig.store
        .transaction(&mut |writes| {
            let version = writes.load(&stream)?.len() as u64;
            writes.append(&stream, &[event.encode()], version)?;
            Ok(())
        })
        .unwrap();
}

/// A renewal whose body and token are given, from the current certificate.
fn renew_with(
    rig: &CaRig,
    component: &Component,
    bearer: Option<String>,
    body: &serde_json::Value,
) -> Result<hecate_application::identity::service::Enrolled, hecate_application::errors::ServiceError>
{
    let credentials = Credentials {
        certificates: component.inner.certificate_chain().unwrap(),
        bearer,
    };
    rig.identities.renew(&credentials, Some("r1"), body)
}

// --- Renewal --------------------------------------------------------------------------------------

#[test]
fn a_valid_renewal_issues_a_3_day_certificate_with_the_same_identity_and_new_keys() {
    let (rig, signed) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let old = hermes.serial();
    rig.clock.advance(Duration::hours(48));
    let now = rig.clock.now();
    let (body, _) = hermes.inner.renewal().unwrap();
    let new_call_key = rig
        .pki
        .read_call_key(body["call_key"].as_str().unwrap())
        .unwrap();
    let renewed = hermes.renewed(&rig);
    assert_eq!(renewed.identity, spiffe("hermes"));
    assert_ne!(renewed.serial, old);
    assert_eq!(renewed.not_after, now + Duration::hours(72));
    assert_eq!(renewed.chain[1], signed.root_der);
    assert_eq!(renewed.call_key_id, new_call_key.kid());

    let previous = certificate(&rig, "hermes", &old);
    assert_eq!(previous.renewed_to.as_ref(), Some(&renewed.serial));
    let current = certificate(&rig, "hermes", &renewed.serial);
    assert_eq!(current.issued.origin, CertificateOrigin::Renewal);
    assert_eq!(current.issued.renewed_from.as_ref(), Some(&old));
    assert_eq!(current.issued.epoch, previous.issued.epoch);
    assert_eq!(current.issued.call_key.as_ref(), Some(&new_call_key));
    let old_key = rig
        .pki
        .read_certificate(&previous.issued.certificate)
        .unwrap()
        .public_key;
    let new_key = rig
        .pki
        .read_certificate(&renewed.certificate)
        .unwrap()
        .public_key;
    assert_ne!(old_key, new_key);
    assert!(!previous.renewable(now, 1, current.issued.epoch, true));
    assert!(current.renewable(now, 1, current.issued.epoch, true));

    let row = rows(&rig, Operation::CertificateRenew).pop().unwrap();
    assert_eq!(row.method, Method::Identity);
    assert_eq!(row.result, AccessResult::Success);
    assert_eq!(row.subject.as_deref(), Some("spiffe://cerbero.test/hermes"));
    assert_eq!(row.identity.as_deref(), Some("hermes"));
    assert_eq!(row.certificate.as_deref(), Some(old.as_str()));
    assert_eq!(row.detail, format!("issued {}", renewed.serial));
}

#[test]
fn the_repetition_with_a_new_token_returns_the_same_serial_without_effects() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let old = hermes.serial();
    let first = hermes.renew(&rig).unwrap();
    let events = certificate_events(&rig, "hermes", &old);
    let again = hermes.renew(&rig).unwrap();
    assert_eq!(again.serial, first.serial);
    assert_eq!(again.certificate, first.certificate);
    assert_eq!(certificate_events(&rig, "hermes", &old), events);
    assert_eq!(rig.store.streams("certificate/hermes/").unwrap().len(), 2);
    let repeat = rows(&rig, Operation::CertificateRenew).pop().unwrap();
    assert_eq!(repeat.result, AccessResult::Success);
    assert_eq!(repeat.detail, format!("repeat {}", first.serial));
    let state = certificate(&rig, "hermes", &first.serial).state(rig.clock.now());
    assert_eq!(state, CertificateState::Valid);
}

#[test]
fn a_certificate_that_is_not_valid_is_auth_and_leaves_a_row() {
    let (rig, _) = created_ca();
    let now = rig.clock.now();
    let expired = Component::enrolled(&rig, "expired");
    let revoked = Component::enrolled(&rig, "revoked");
    append_certificate_event(
        &rig,
        "revoked",
        &revoked.serial(),
        CertificateEvent::CertificateRevoked {
            at: now,
            urgent: true,
            by: RevokedBy::Admin,
            reason: RevocationReason::Admin,
        },
    );
    let passive = Component::enrolled(&rig, "passive");
    append_certificate_event(
        &rig,
        "passive",
        &passive.serial(),
        CertificateEvent::CertificateRevoked {
            at: now,
            urgent: false,
            by: RevokedBy::Admin,
            reason: RevocationReason::Admin,
        },
    );
    let disabled = Component::enrolled(&rig, "disabled");
    append_identity_events(
        &rig,
        "disabled",
        &[IdentityEvent::IdentityDisabled {
            at: now,
            urgent: false,
        }],
    );
    // A certificate of another CA with a name of this trust domain.
    let (other_rig, _) = created_ca();
    let foreign = Component::enrolled(&other_rig, "hermes");
    for component in [&revoked, &passive, &disabled, &foreign] {
        let error = component.renew(&rig).err().unwrap();
        assert!(is_auth(&error), "{error:?}");
    }
    let no_certificate = Credentials {
        certificates: Vec::new(),
        bearer: Some(revoked.inner.call_token(None).unwrap()),
    };
    let (body, key) = revoked.inner.throwaway_renewal(None).unwrap();
    let error = rig
        .identities
        .renew(&no_certificate, Some(&key), &body)
        .err()
        .unwrap();
    assert!(is_auth(&error), "{error:?}");
    rig.clock.advance(Duration::hours(72));
    let error = expired.renew(&rig).err().unwrap();
    assert!(is_auth(&error), "{error:?}");
    let refused = rows(&rig, Operation::CertificateRenew);
    assert_eq!(refused.len(), 6);
    assert!(
        refused
            .iter()
            .all(|row| row.result == AccessResult::Failure)
    );
    assert_eq!(
        refused[3].subject, None,
        "a certificate of another CA is not recognized"
    );
    for (identity, component) in [
        ("expired", &expired),
        ("revoked", &revoked),
        ("passive", &passive),
        ("disabled", &disabled),
    ] {
        assert!(
            certificate(&rig, identity, &component.serial())
                .renewed_to
                .is_none()
        );
    }
}

#[test]
fn a_call_token_that_is_not_valid_is_auth_and_leaves_a_row() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let hades = Component::enrolled(&rig, "hades");
    let (body, _) = hermes.inner.renewal().unwrap();
    let token = |claims: &serde_json::Value| hermes.inner.call_token_with(None, claims).unwrap();
    let mut other_issuer = hermes.inner.call_claims(None).unwrap();
    other_issuer["iss"] = json!("spiffe://cerbero.test/hades");
    let other_key = hades
        .inner
        .call_token_with(None, &hermes.inner.call_claims(None).unwrap())
        .unwrap();
    let stale = token(&hermes.inner.call_claims(None).unwrap());
    for bearer in [None, Some(token(&other_issuer)), Some(other_key)] {
        let error = renew_with(&rig, &hermes, bearer, &body).err().unwrap();
        assert!(is_auth(&error), "{error:?}");
    }
    // A token used by a request that failed is used all the same.
    let used = token(&hermes.inner.call_claims(None).unwrap());
    let bad_body = json!({"csr": "not pem", "call_key": "not pem"});
    assert!(is_invalid(
        &renew_with(&rig, &hermes, Some(used.clone()), &bad_body)
            .err()
            .unwrap()
    ));
    let error = renew_with(&rig, &hermes, Some(used), &body).err().unwrap();
    assert!(is_auth(&error), "{error:?}");
    rig.clock.advance(Duration::seconds(331));
    let error = renew_with(&rig, &hermes, Some(stale), &body).err().unwrap();
    assert!(is_auth(&error), "{error:?}");
    let refused = rows(&rig, Operation::CertificateRenew);
    assert_eq!(refused.len(), 6);
    assert!(
        certificate(&rig, "hermes", &hermes.serial())
            .renewed_to
            .is_none()
    );
}

#[test]
fn a_request_that_repeats_a_current_key_or_asks_for_another_identity_is_validation() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let (fresh, _) = hermes.inner.throwaway_renewal(None).unwrap();
    let same_tls_key = json!({
        "csr": hermes.inner.request_for_current_key().unwrap(),
        "call_key": fresh["call_key"],
    });
    let same_call_key = json!({
        "csr": fresh["csr"],
        "call_key": hermes.inner.call_key().unwrap(),
    });
    let (other_identity, _) = hermes
        .inner
        .throwaway_renewal(Some(&spiffe("hades")))
        .unwrap();
    for body in [
        same_tls_key,
        same_call_key,
        other_identity,
        json!({"csr": fresh["csr"]}),
    ] {
        let bearer = Some(hermes.inner.call_token(None).unwrap());
        let error = renew_with(&rig, &hermes, bearer, &body).err().unwrap();
        assert!(is_invalid(&error), "{body}: {error:?}");
    }
    let credentials = hermes.credentials(None);
    let missing_key = rig
        .identities
        .renew(&credentials, None, &fresh)
        .err()
        .unwrap();
    assert!(is_invalid(&missing_key), "{missing_key:?}");
    assert!(
        certificate(&rig, "hermes", &hermes.serial())
            .renewed_to
            .is_none()
    );
}

// --- Second attempt -------------------------------------------------------------------------------

#[test]
fn a_second_attempt_revokes_every_valid_certificate_of_the_identity() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let first = hermes.serial();
    let renewed = hermes.renewed(&rig);
    let (body, key) = hermes.inner.throwaway_renewal(None).unwrap();
    let error = rig
        .identities
        .renew(&hermes.credentials(Some(&first)), Some(&key), &body)
        .err()
        .unwrap();
    assert!(is_auth(&error), "{error:?}");
    let now = rig.clock.now();
    for serial in [&first, &renewed.serial] {
        let revoked = certificate(&rig, "hermes", serial);
        assert_eq!(revoked.state(now), CertificateState::Revoked, "{serial}");
        let revocation = revoked.revocation.unwrap();
        assert_eq!(
            (revocation.urgent, revocation.by, revocation.reason),
            (true, RevokedBy::Hecate, RevocationReason::SuspiciousRenewal)
        );
    }
    let records = rig.store.load("identity/hermes").unwrap();
    let last = IdentityEvent::decode(records.last().unwrap()).unwrap();
    assert_eq!(
        last,
        IdentityEvent::SuspiciousRenewalDetected {
            at: now,
            serial: first.clone(),
        }
    );
    let revocations = rows(&rig, Operation::CertificateRevoke);
    assert_eq!(revocations.len(), 2);
    for row in &revocations {
        assert_eq!(row.method, Method::Internal);
        assert_eq!(row.subject.as_deref(), Some("hecate"));
        assert_eq!(row.identity.as_deref(), Some("hermes"));
    }
    let attempt = rows(&rig, Operation::CertificateRenew).pop().unwrap();
    assert_eq!(
        (attempt.result, attempt.detail.as_str()),
        (AccessResult::Failure, "second attempt")
    );
    assert_eq!(rig.identities.counts().unwrap().suspicious_renewals_24h, 1);
    rig.sealing.seal().unwrap();
    assert_eq!(rig.identities.counts().unwrap().suspicious_renewals_24h, 1);
    rig.clock.advance(Duration::hours(24));
    assert_eq!(rig.identities.counts().unwrap().suspicious_renewals_24h, 0);
}

/// T123 (SC-011): what Hécate publishes no longer accepts any certificate of the identity issued
/// before the second attempt.
#[test]
fn after_a_second_attempt_the_crl_lists_every_earlier_certificate_and_their_keys_are_gone() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let first = hermes.serial();
    let renewed = hermes.renewed(&rig);
    let (body, key) = hermes.inner.throwaway_renewal(None).unwrap();
    let error = rig
        .identities
        .renew(&hermes.credentials(Some(&first)), Some(&key), &body)
        .err()
        .unwrap();
    assert!(is_auth(&error), "{error:?}");
    let sorted = |mut serials: Vec<Serial>| {
        serials.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        serials
    };
    let listed = sorted(
        rig.identities
            .revocations(&caller.credentials(None))
            .unwrap()
            .crls
            .iter()
            .flat_map(|der| read_crl(der).unwrap().revoked)
            .collect(),
    );
    assert_eq!(listed, sorted(vec![first, renewed.serial]));
    let identities: Vec<String> = rig
        .identities
        .keys(&caller.credentials(None))
        .unwrap()
        .keys
        .iter()
        .map(|key| key.identity.identity().to_string())
        .collect();
    assert_eq!(identities, vec!["caller"]);
}

#[test]
fn of_two_renewals_of_one_certificate_at_once_one_issues_and_the_other_is_a_second_attempt() {
    let (rig, _) = created_ca();
    let rig = Arc::new(rig);
    let hermes = Arc::new(Component::enrolled(&rig, "hermes"));
    let requests: Vec<_> = (0..2)
        .map(|_| {
            let (body, key) = hermes.inner.throwaway_renewal(None).unwrap();
            (hermes.credentials(None), body, key)
        })
        .collect();
    let handles: Vec<_> = requests
        .into_iter()
        .map(|(credentials, body, key)| {
            let rig = rig.clone();
            std::thread::spawn(move || rig.identities.renew(&credentials, Some(&key), &body))
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
    let now = rig.clock.now();
    for stream in rig.store.streams("certificate/hermes/").unwrap() {
        let serial = Serial::parse(stream.rsplit('/').next().unwrap()).unwrap();
        assert_eq!(
            certificate(&rig, "hermes", &serial).state(now),
            CertificateState::Revoked
        );
    }
}

// --- Failures ------------------------------------------------------------------------------------

#[test]
fn a_power_cut_during_the_renewal_leaves_nothing_and_the_repetition_issues() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    rig.store.fail_commits(true);
    assert!(hermes.renew(&rig).is_err());
    assert_eq!(rig.store.streams("certificate/hermes/").unwrap().len(), 1);
    assert!(
        certificate(&rig, "hermes", &hermes.serial())
            .renewed_to
            .is_none()
    );
    rig.store.fail_commits(false);
    let renewed = hermes.renew(&rig).unwrap();
    assert_eq!(renewed.identity, spiffe("hermes"));
}

#[test]
fn sealed_hecate_answers_that_it_is_sealed() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let before = rows(&rig, Operation::CertificateRenew).len();
    rig.sealing.seal().unwrap();
    let error = hermes.renew(&rig).err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    // T127 (FR-024): the refusal leaves its row, without a subject: nothing was checked.
    let renewals = rows(&rig, Operation::CertificateRenew);
    assert_eq!(renewals.len(), before + 1, "{renewals:?}");
    let refused = renewals.last().unwrap();
    assert_eq!(
        (
            refused.method,
            refused.result,
            refused.detail.as_str(),
            refused.subject.as_deref(),
            refused.certificate.as_deref(),
        ),
        (
            Method::Identity,
            AccessResult::RejectedConflict,
            "sealed",
            None,
            None
        )
    );
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

#[test]
fn status_counts_the_identities_whose_last_certificate_expires_within_24_hours() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let _hades = Component::enrolled(&rig, "hades");
    let disabled = Component::enrolled(&rig, "disabled");
    assert_eq!(rig.identities.counts().unwrap().identities_expiring_24h, 0);
    append_identity_events(
        &rig,
        "disabled",
        &[IdentityEvent::IdentityDisabled {
            at: rig.clock.now(),
            urgent: false,
        }],
    );
    drop(disabled);
    rig.clock.advance(Duration::hours(49));
    assert_eq!(rig.identities.counts().unwrap().identities_expiring_24h, 2);
    hermes.renewed(&rig);
    assert_eq!(rig.identities.counts().unwrap().identities_expiring_24h, 1);
    rig.sealing.seal().unwrap();
    assert_eq!(rig.identities.counts().unwrap().identities_expiring_24h, 1);
}
