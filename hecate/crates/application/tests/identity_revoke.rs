//! T087: revocations, disables and the lists of the admin (FR-012, FR-018 to FR-020, FR-022,
//! FR-024, FR-026, SC-005, SC-007, SC-015): a passive revocation only removes the renewal, an
//! urgent one also puts the certificate in the CRL and removes its key; Hécate's certificate is
//! replaced at once.

mod common;

use common::*;
use hecate_application::identity::service::{CallRefusal, Credentials, Disabled, Revoked};
use hecate_application::ports::{Clock, EventStore};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::events::{CertificateEvent, RevocationReason, RevokedBy};
use hecate_domain::identity::certificate::{Certificate, CertificateState};
use hecate_domain::identity::identity::IdentityStatus;
use hecate_domain::identity::names::{IdentityName, Serial};
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::testing::read_crl;
use time::Duration;

fn rows(rig: &CaRig, operation: Operation) -> Vec<AccessLogEntry> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .collect()
}

fn certificate(rig: &CaRig, identity: &str, serial: &Serial) -> Certificate {
    let events: Vec<CertificateEvent> = rig
        .store
        .load(&format!("certificate/{identity}/{serial}"))
        .unwrap()
        .iter()
        .map(|record| CertificateEvent::decode(record).unwrap())
        .collect();
    Certificate::from_events(&events).unwrap()
}

fn stream_len(rig: &CaRig, stream: &str) -> usize {
    rig.store.load(stream).unwrap().len()
}

/// The serials of every CRL that a component receives now.
fn crl_serials(rig: &CaRig, caller: &Component) -> Vec<Serial> {
    let mut serials: Vec<Serial> = rig
        .identities
        .revocations(&caller.credentials(None))
        .unwrap()
        .crls
        .iter()
        .flat_map(|der| read_crl(der).unwrap().revoked)
        .collect();
    serials.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    serials
}

/// The identities whose keys sign new calls now.
fn with_active_keys(rig: &CaRig, caller: &Component) -> Vec<String> {
    let mut identities: Vec<String> = rig
        .identities
        .keys(&caller.credentials(None))
        .unwrap()
        .keys
        .iter()
        .map(|key| key.identity.identity().to_string())
        .collect();
    identities.sort();
    identities
}

fn own_serial(rig: &CaRig) -> Serial {
    rig.service
        .status()
        .unwrap()
        .unwrap()
        .hecate_certificate
        .unwrap()
        .serial
}

fn sorted(mut serials: Vec<Serial>) -> Vec<Serial> {
    serials.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    serials
}

fn hermes_name() -> IdentityName {
    IdentityName::parse("hermes").unwrap()
}

// --- Revocations ---------------------------------------------------------------------------------

#[test]
fn a_passive_revocation_removes_the_renewal_and_keeps_the_certificate_and_its_key() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let serial = hermes.serial();
    let revoked = rig
        .identities
        .certificates_revoke(serial.as_str(), false)
        .unwrap();
    assert_eq!(
        revoked,
        Revoked {
            serial: serial.clone(),
            reissued: None,
        }
    );
    assert!(is_auth(&hermes.renew(&rig).err().unwrap()));
    assert!(crl_serials(&rig, &caller).is_empty());
    assert_eq!(with_active_keys(&rig, &caller), vec!["caller", "hermes"]);
    assert!(rig.identities.keys(&hermes.credentials(None)).is_ok());
    let kept = certificate(&rig, "hermes", &serial);
    assert_eq!(kept.state(rig.clock.now()), CertificateState::Valid);
    let revocation = kept.revocation.unwrap();
    assert_eq!(
        (revocation.urgent, revocation.by, revocation.reason),
        (false, RevokedBy::Admin, RevocationReason::Admin)
    );
    rig.clock.advance(Duration::hours(48));
    caller.renewed(&rig);
    rig.clock.advance(Duration::hours(24));
    assert_eq!(with_active_keys(&rig, &caller), vec!["caller"]);
}

#[test]
fn an_urgent_revocation_ends_the_certificate_and_its_key_and_lists_it_until_it_expires() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let serial = hermes.serial();
    rig.identities
        .certificates_revoke(serial.as_str(), true)
        .unwrap();
    assert!(is_auth(
        &rig.identities
            .keys(&hermes.credentials(None))
            .err()
            .unwrap()
    ));
    assert!(is_auth(&hermes.renew(&rig).err().unwrap()));
    assert_eq!(crl_serials(&rig, &caller), vec![serial.clone()]);
    assert_eq!(with_active_keys(&rig, &caller), vec!["caller"]);
    let revoked = certificate(&rig, "hermes", &serial);
    assert_eq!(revoked.state(rig.clock.now()), CertificateState::Revoked);
    rig.clock.advance(Duration::hours(48));
    caller.renewed(&rig);
    rig.clock
        .advance(Duration::hours(24) - Duration::seconds(1));
    assert_eq!(crl_serials(&rig, &caller), vec![serial]);
    rig.clock.advance(Duration::seconds(1));
    assert!(crl_serials(&rig, &caller).is_empty());
}

#[test]
fn revoking_a_certificate_just_renewed_revokes_only_that_one() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let old = hermes.serial();
    let renewed = hermes.renewed(&rig);
    rig.identities
        .certificates_revoke(old.as_str(), true)
        .unwrap();
    let now = rig.clock.now();
    assert_eq!(
        certificate(&rig, "hermes", &old).state(now),
        CertificateState::Revoked
    );
    let current = certificate(&rig, "hermes", &renewed.serial);
    assert_eq!(current.state(now), CertificateState::Valid);
    assert!(current.revocation.is_none());
    assert_eq!(crl_serials(&rig, &caller), vec![old]);
    assert!(rig.identities.keys(&hermes.credentials(None)).is_ok());
}

#[test]
fn the_revocation_leaves_a_row_with_the_identity_and_the_serial() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    rig.identities
        .certificates_revoke(hermes.serial().as_str(), true)
        .unwrap();
    let row = rows(&rig, Operation::CertificateRevoke).pop().unwrap();
    assert_eq!(row.method, Method::Local);
    assert_eq!(row.subject.as_deref(), Some("admin"));
    assert_eq!(row.result, AccessResult::Success);
    assert_eq!(row.identity.as_deref(), Some("hermes"));
    assert_eq!(row.certificate.as_deref(), Some(hermes.serial().as_str()));
    assert_eq!(row.detail, "revoke urgent");
}

// --- Disable and enable --------------------------------------------------------------------------

#[test]
fn a_disable_stops_renewals_keys_and_pending_tokens_and_no_token_is_created() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let pending = new_token(&rig, "hermes");
    let disabled = rig.identities.identities_disable("hermes", false).unwrap();
    assert_eq!(
        disabled,
        Disabled {
            identity: hermes_name(),
            revoked: Vec::new(),
        }
    );
    assert!(is_auth(&hermes.renew(&rig).err().unwrap()));
    assert_eq!(with_active_keys(&rig, &caller), vec!["caller"]);
    assert!(rig.identities.tokens_list().unwrap().is_empty());
    let body = enroll_body(
        &pending,
        &request_for(&rig, Some("hermes")),
        &call_key_pem(7),
    );
    assert!(is_auth(
        &rig.identities.enroll(Some("again"), &body).err().unwrap()
    ));
    assert!(is_conflict(
        &rig.identities.tokens_create("hermes").err().unwrap()
    ));
    assert!(crl_serials(&rig, &caller).is_empty());
    let row = rows(&rig, Operation::IdentityDisable).pop().unwrap();
    assert_eq!(
        (
            row.subject.as_deref(),
            row.identity.as_deref(),
            row.detail.as_str()
        ),
        (Some("admin"), Some("hermes"), "disable")
    );
}

#[test]
fn an_urgent_disable_puts_the_valid_certificates_in_the_crl() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let caller = Component::enrolled(&rig, "caller");
    let old = hermes.serial();
    let renewed = hermes.renewed(&rig);
    let disabled = rig.identities.identities_disable("hermes", true).unwrap();
    let both = sorted(vec![old.clone(), renewed.serial.clone()]);
    assert_eq!(sorted(disabled.revoked), both);
    assert_eq!(crl_serials(&rig, &caller), both);
    for serial in &both {
        let revocation = certificate(&rig, "hermes", serial).revocation.unwrap();
        assert_eq!(
            (revocation.urgent, revocation.by, revocation.reason),
            (true, RevokedBy::Admin, RevocationReason::Admin)
        );
    }
    let revocations = rows(&rig, Operation::CertificateRevoke);
    assert_eq!(revocations.len(), 2);
    for row in &revocations {
        assert_eq!(row.method, Method::Local);
        assert_eq!(row.subject.as_deref(), Some("admin"));
        assert_eq!(row.identity.as_deref(), Some("hermes"));
        assert_eq!(row.detail, "disable urgent");
    }
    let row = rows(&rig, Operation::IdentityDisable).pop().unwrap();
    assert_eq!(row.detail, "disable urgent");
}

#[test]
fn an_enabled_identity_needs_a_new_token_and_its_previous_certificates_do_not_renew() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let before = new_token(&rig, "hermes");
    rig.identities.identities_disable("hermes", false).unwrap();
    assert_eq!(
        rig.identities.identities_enable("hermes").unwrap(),
        hermes_name()
    );
    assert!(is_auth(&hermes.renew(&rig).err().unwrap()));
    let body = enroll_body(
        &before,
        &request_for(&rig, Some("hermes")),
        &call_key_pem(7),
    );
    assert!(is_auth(
        &rig.identities.enroll(Some("before"), &body).err().unwrap()
    ));
    let again = Component::enrolled(&rig, "hermes");
    assert!(again.renew(&rig).is_ok());
    let detail = rig.identities.identities_get("hermes").unwrap();
    assert_eq!(detail.status, IdentityStatus::Enrolled);
    let row = rows(&rig, Operation::IdentityEnable).pop().unwrap();
    assert_eq!(
        (
            row.subject.as_deref(),
            row.identity.as_deref(),
            row.detail.as_str()
        ),
        (Some("admin"), Some("hermes"), "enable")
    );
}

// --- Hécate's certificate ------------------------------------------------------------------------

#[test]
fn the_urgent_revocation_of_hecates_certificate_replaces_it_at_once_with_another_key() {
    let (rig, _) = created_ca();
    let caller = Component::enrolled(&rig, "caller");
    let own = own_serial(&rig);
    let before = rig.service.tls_material().unwrap().unwrap();
    let old_key = before.key.expose().to_vec();
    let revoked = rig
        .identities
        .certificates_revoke(own.as_str(), true)
        .unwrap();
    assert_eq!(revoked.serial, own);
    let new = revoked.reissued.unwrap();
    assert_ne!(new, own);
    assert_eq!(own_serial(&rig), new);
    let after = rig.service.tls_material().unwrap().unwrap();
    assert_ne!(after.key.expose(), old_key.as_slice());
    assert_eq!(
        rig.pki.read_certificate(&after.chain[0]).unwrap().serial,
        new
    );
    assert_eq!(crl_serials(&rig, &caller), vec![own.clone()]);
    let row = rows(&rig, Operation::CertificateRevoke).pop().unwrap();
    assert_eq!(
        (
            row.subject.as_deref(),
            row.identity.as_deref(),
            row.certificate.as_deref()
        ),
        (Some("admin"), Some("hecate"), Some(own.as_str()))
    );
    let reissue = rows(&rig, Operation::CertificateRenew).pop().unwrap();
    assert_eq!(reissue.method, Method::Internal);
    assert_eq!(reissue.subject.as_deref(), Some("hecate"));
    assert_eq!(reissue.identity.as_deref(), Some("hecate"));
    assert_eq!(reissue.certificate.as_deref(), Some(new.as_str()));
    assert_eq!(reissue.detail, "reissue");
}

#[test]
fn a_passive_revocation_of_hecates_certificate_and_disabling_hecate_are_validation() {
    let (rig, _) = created_ca();
    let own = own_serial(&rig);
    let error = rig
        .identities
        .certificates_revoke(own.as_str(), false)
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
    for urgent in [false, true] {
        let error = rig
            .identities
            .identities_disable("hecate", urgent)
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{error:?}");
    }
    assert_eq!(stream_len(&rig, &format!("certificate/hecate/{own}")), 1);
    assert_eq!(own_serial(&rig), own);
    let refused = rows(&rig, Operation::CertificateRevoke).pop().unwrap();
    assert_eq!(refused.result, AccessResult::InvalidInput);
    assert_eq!(refused.identity.as_deref(), Some("hecate"));
}

// --- Lists ---------------------------------------------------------------------------------------

#[test]
fn the_lists_show_identities_and_certificates_without_keys() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let first = hermes.serial();
    rig.clock.advance(Duration::hours(48));
    let renewed = hermes.renewed(&rig);
    let _hades = Component::enrolled(&rig, "hades");
    rig.identities.identities_disable("hades", false).unwrap();
    new_token(&rig, "pending");
    let listed: Vec<(String, IdentityStatus, time::OffsetDateTime)> = rig
        .identities
        .identities_list()
        .unwrap()
        .into_iter()
        .map(|summary| {
            (
                summary.identity.to_string(),
                summary.status,
                summary.certificate_not_after,
            )
        })
        .collect();
    let now = rig.clock.now();
    assert_eq!(
        listed,
        vec![
            (
                "hades".to_string(),
                IdentityStatus::Disabled,
                now + Duration::hours(72)
            ),
            (
                "hermes".to_string(),
                IdentityStatus::Enrolled,
                renewed.not_after
            ),
        ]
    );
    let detail = rig.identities.identities_get("hermes").unwrap();
    assert_eq!(detail.identity, hermes_name());
    assert_eq!(detail.status, IdentityStatus::Enrolled);
    let certificates: Vec<(Serial, CertificateState, bool)> = detail
        .certificates
        .iter()
        .map(|certificate| {
            (
                certificate.serial.clone(),
                certificate.state,
                certificate.renewable,
            )
        })
        .collect();
    assert_eq!(
        certificates,
        vec![
            (first, CertificateState::Valid, false),
            (renewed.serial.clone(), CertificateState::Valid, true),
        ]
    );
    let current = &detail.certificates[1];
    assert_eq!(current.not_after, renewed.not_after);
    assert_eq!(current.not_before, now - Duration::minutes(1));
    let list_row = rows(&rig, Operation::IdentityList).pop().unwrap();
    assert_eq!(
        (
            list_row.method,
            list_row.subject.as_deref(),
            list_row.identity.as_deref()
        ),
        (Method::Local, Some("admin"), None)
    );
    let get_row = rows(&rig, Operation::IdentityGet).pop().unwrap();
    assert_eq!(get_row.identity.as_deref(), Some("hermes"));
    assert_eq!(get_row.result, AccessResult::Success);
}

#[test]
fn an_identity_with_only_tokens_or_unknown_is_not_found_and_a_bad_name_is_validation() {
    let (rig, _) = created_ca();
    new_token(&rig, "pending");
    for name in ["pending", "nobody"] {
        assert!(is_not_found(
            &rig.identities.identities_get(name).err().unwrap()
        ));
        assert!(is_not_found(
            &rig.identities.identities_disable(name, true).err().unwrap()
        ));
        assert!(is_not_found(
            &rig.identities.identities_enable(name).err().unwrap()
        ));
    }
    assert!(is_invalid(
        &rig.identities.identities_get("Not A Name").err().unwrap()
    ));
    let unknown = serial(0x42);
    assert!(is_not_found(
        &rig.identities
            .certificates_revoke(unknown.as_str(), true)
            .err()
            .unwrap()
    ));
    assert!(is_invalid(
        &rig.identities
            .certificates_revoke("not a serial", true)
            .err()
            .unwrap()
    ));
    let refused = rows(&rig, Operation::IdentityGet);
    assert_eq!(refused[0].result, AccessResult::NotFound);
    assert_eq!(refused[0].identity.as_deref(), Some("pending"));
}

// --- Repeated orders -----------------------------------------------------------------------------

#[test]
fn a_repeated_order_writes_no_events_and_urgency_is_a_change() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let hades = Component::enrolled(&rig, "hades");
    let identity = "identity/hermes";
    rig.identities.identities_disable("hermes", false).unwrap();
    let after_disable = stream_len(&rig, identity);
    let again = rig.identities.identities_disable("hermes", false).unwrap();
    assert!(again.revoked.is_empty());
    assert_eq!(stream_len(&rig, identity), after_disable);
    let urgent = rig.identities.identities_disable("hermes", true).unwrap();
    assert_eq!(urgent.revoked, vec![hermes.serial()]);
    assert_eq!(stream_len(&rig, identity), after_disable + 1);
    let urgent_again = rig.identities.identities_disable("hermes", true).unwrap();
    assert!(urgent_again.revoked.is_empty());
    assert_eq!(stream_len(&rig, identity), after_disable + 1);
    rig.identities.identities_enable("hermes").unwrap();
    let enabled = stream_len(&rig, identity);
    rig.identities.identities_enable("hermes").unwrap();
    assert_eq!(stream_len(&rig, identity), enabled);

    let serial = hades.serial();
    let stream = format!("certificate/hades/{serial}");
    rig.identities
        .certificates_revoke(serial.as_str(), false)
        .unwrap();
    assert_eq!(stream_len(&rig, &stream), 2);
    rig.identities
        .certificates_revoke(serial.as_str(), false)
        .unwrap();
    assert_eq!(stream_len(&rig, &stream), 2);
    let made_urgent = rig
        .identities
        .certificates_revoke(serial.as_str(), true)
        .unwrap();
    assert_eq!(made_urgent.serial, serial);
    assert_eq!(stream_len(&rig, &stream), 3);
    assert!(
        certificate(&rig, "hades", &serial)
            .revocation
            .unwrap()
            .urgent
    );
    for urgent in [true, false] {
        rig.identities
            .certificates_revoke(serial.as_str(), urgent)
            .unwrap();
        assert_eq!(stream_len(&rig, &stream), 3);
    }
    let rows = rows(&rig, Operation::CertificateRevoke);
    assert!(rows.iter().all(|row| row.result == AccessResult::Success));
}

#[test]
fn revoking_an_expired_certificate_changes_nothing() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let serial = hermes.serial();
    rig.clock.advance(Duration::hours(73));
    let revoked = rig
        .identities
        .certificates_revoke(serial.as_str(), true)
        .unwrap();
    assert_eq!(revoked.reissued, None);
    assert_eq!(stream_len(&rig, &format!("certificate/hermes/{serial}")), 1);
}

// --- Failures ------------------------------------------------------------------------------------

#[test]
fn a_power_cut_during_a_revocation_or_a_disable_leaves_nothing() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let serial = hermes.serial();
    let own = own_serial(&rig);
    rig.store.fail_commits(true);
    assert!(
        rig.identities
            .certificates_revoke(serial.as_str(), true)
            .is_err()
    );
    assert!(rig.identities.identities_disable("hermes", true).is_err());
    assert!(
        rig.identities
            .certificates_revoke(own.as_str(), true)
            .is_err()
    );
    rig.store.fail_commits(false);
    assert!(certificate(&rig, "hermes", &serial).revocation.is_none());
    let detail = rig.identities.identities_get("hermes").unwrap();
    assert_eq!(detail.status, IdentityStatus::Enrolled);
    assert_eq!(own_serial(&rig), own);
    assert!(certificate(&rig, "hecate", &own).revocation.is_none());
    assert_eq!(rig.store.streams("certificate/hecate/").unwrap().len(), 1);
    assert!(rows(&rig, Operation::CertificateRenew).is_empty());
    rig.identities.identities_disable("hermes", true).unwrap();
    assert!(
        certificate(&rig, "hermes", &serial)
            .revocation
            .unwrap()
            .urgent
    );
}

#[test]
fn sealed_hecate_refuses_the_orders_and_the_crl() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let credentials = hermes.credentials(None);
    rig.sealing.seal().unwrap();
    let errors = [
        rig.identities.revocations(&credentials).err().unwrap(),
        rig.identities
            .certificates_revoke(hermes.serial().as_str(), true)
            .err()
            .unwrap(),
        rig.identities
            .identities_disable("hermes", false)
            .err()
            .unwrap(),
        rig.identities.identities_enable("hermes").err().unwrap(),
        rig.identities.identities_list().err().unwrap(),
        rig.identities.identities_get("hermes").err().unwrap(),
    ];
    for error in &errors {
        assert_eq!(conflict_state(error), Some(SealState::Sealed), "{error:?}");
    }
    // T127 (FR-024): the refused query of the component leaves its row, without a subject.
    let refused = rows(&rig, Operation::RevocationList);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(
        (
            refused[0].method,
            refused[0].result,
            refused[0].detail.as_str(),
            refused[0].subject.as_deref(),
        ),
        (
            Method::Identity,
            AccessResult::RejectedConflict,
            "sealed",
            None
        )
    );
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

/// T127 (FR-024): a call that the API refuses before its use case runs leaves the row of its
/// operation, without a subject, an identity or a certificate: nobody is authenticated yet.
#[test]
fn a_call_refused_before_its_use_case_leaves_its_row() {
    let (rig, _) = created_ca();
    rig.sealing.seal().unwrap();
    rig.identities
        .record_refused_call(Operation::KeyList, Method::Identity, CallRefusal::Sealed)
        .unwrap();
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    rig.identities
        .record_refused_call(
            Operation::IdentityEnroll,
            Method::EnrollmentToken,
            CallRefusal::RequestId,
        )
        .unwrap();
    let rows: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .map(|entry| {
            (
                entry.operation,
                entry.method,
                entry.result,
                entry.detail,
                entry.subject,
                entry.identity,
                entry.certificate,
            )
        })
        .filter(|(operation, ..)| {
            [Operation::KeyList, Operation::IdentityEnroll].contains(operation)
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                Operation::KeyList,
                Method::Identity,
                AccessResult::RejectedConflict,
                "sealed".to_string(),
                None,
                None,
                None
            ),
            (
                Operation::IdentityEnroll,
                Method::EnrollmentToken,
                AccessResult::InvalidInput,
                "request_id".to_string(),
                None,
                None,
                None
            ),
        ]
    );
}

// --- The CRL -------------------------------------------------------------------------------------

#[test]
fn each_crl_has_a_growing_number_in_milliseconds_and_is_valid_for_300_seconds() {
    let (rig, _) = created_ca();
    let caller = Component::enrolled(&rig, "caller");
    let now = rig.clock.now();
    let first = rig
        .identities
        .revocations(&caller.credentials(None))
        .unwrap();
    let second = rig
        .identities
        .revocations(&caller.credentials(None))
        .unwrap();
    assert_eq!(first.crls.len(), 1);
    let whole = now.replace_nanosecond(0).unwrap();
    assert_eq!(first.expires_at, whole + Duration::seconds(300));
    let (a, b) = (
        read_crl(&first.crls[0]).unwrap(),
        read_crl(&second.crls[0]).unwrap(),
    );
    assert_eq!(a.number, (now.unix_timestamp_nanos() / 1_000_000) as u64);
    assert_eq!(b.number, a.number + 1);
    assert_eq!(a.this_update, whole);
    assert_eq!(a.next_update, Some(first.expires_at));
}

#[test]
fn a_served_crl_leaves_no_row_and_a_refused_one_does() {
    let (rig, _) = created_ca();
    let caller = Component::enrolled(&rig, "caller");
    rig.identities
        .revocations(&caller.credentials(None))
        .unwrap();
    assert!(rows(&rig, Operation::RevocationList).is_empty());
    let no_token = Credentials {
        certificates: caller.inner.certificate_chain().unwrap(),
        bearer: None,
    };
    assert!(is_auth(
        &rig.identities.revocations(&no_token).err().unwrap()
    ));
    let refused = rows(&rig, Operation::RevocationList);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].method, Method::Identity);
    assert_eq!(refused[0].result, AccessResult::Failure);
    assert_eq!(refused[0].identity.as_deref(), Some("caller"));
}
