//! T097: the renewal of the intermediate (FR-005, FR-025, SC-009, SC-018, research R9): a
//! ceremony with the root CA key in the CLI; the new version issues from then on, the previous one
//! only signs the CRL of its certificates and its key is destroyed when the last one expires.

mod common;

use common::*;
use hecate_application::ca::service::RenewedIntermediate;
use hecate_application::ports::{Clock, EventStore};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::ca::authority::IntermediateState;
use hecate_domain::events::{CaEvent, RetireReason};
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::pki::RootCaKey;
use time::Duration;

fn rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaIntermediateRenew)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn state_of(rig: &CaRig, version: u64) -> IntermediateState {
    rig.service
        .authority()
        .unwrap()
        .unwrap()
        .intermediate(1, version)
        .unwrap()
        .state
}

/// What the CLI signs between `begin` and `confirm`, with the root CA key of the ceremony.
fn signed_intermediate(rig: &CaRig, root_key: &RootCaKey, root_der: &[u8], csr: &str) -> String {
    let der = root_key
        .sign_intermediate(root_der, csr, &serial(0x9d), rig.clock.now())
        .unwrap();
    pem::encode("CERTIFICATE", &der)
}

/// The whole renewal; returns what `confirm` answers and the DER of the new intermediate.
fn renew(rig: &CaRig, signed: &SignedCa) -> (RenewedIntermediate, Vec<u8>) {
    let begun = rig.service.intermediate_begin().unwrap();
    let certificate = signed_intermediate(
        rig,
        &signed.root_key,
        &signed.root_der,
        &begun.intermediate_csr,
    );
    let renewed = rig
        .service
        .intermediate_confirm(&begun.ceremony_id, &certificate)
        .unwrap();
    (renewed, pem::single(&certificate, "CERTIFICATE").unwrap())
}

// --- begin ---------------------------------------------------------------------------------------

#[test]
fn begin_returns_the_request_and_the_root_ca_and_allows_one_ceremony() {
    let (rig, signed) = created_ca();
    let begun = rig.service.intermediate_begin().unwrap();
    assert!(
        begun
            .intermediate_csr
            .starts_with("-----BEGIN CERTIFICATE REQUEST-----\n")
    );
    assert_eq!(
        begun.root_certificate,
        pem::encode("CERTIFICATE", &signed.root_der)
    );
    assert_eq!(begun.expires_at, rig.clock.now() + Duration::minutes(15));
    assert!(is_conflict(
        &rig.service.intermediate_begin().err().unwrap()
    ));
    assert_eq!(
        rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::RejectedConflict, "begin".to_string()),
        ]
    );
}

#[test]
fn begin_needs_unsealed_hecate_and_a_ca() {
    let rig = ca_rig();
    assert!(is_conflict(
        &rig.service.intermediate_begin().err().unwrap()
    ));
    let (rig, _) = created_ca();
    rig.sealing.seal().unwrap();
    let error = rig.service.intermediate_begin().err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

#[test]
fn with_the_intermediate_expired_the_renewal_is_the_way_out() {
    let (rig, signed) = created_ca();
    rig.clock.advance(Duration::days(366));
    assert!(is_conflict(
        &rig.identities.tokens_create("hermes").err().unwrap()
    ));
    let (renewed, _) = renew(&rig, &signed);
    assert_eq!(renewed.intermediate_version, 2);
    let hermes = Component::enrolled(&rig, "hermes");
    assert_eq!(hermes.inner.spiffe_id_of(None).unwrap(), spiffe("hermes"));
}

// --- confirm -------------------------------------------------------------------------------------

#[test]
fn confirm_activates_the_new_version_and_leaves_the_previous_one_verify_only() {
    let (rig, signed) = created_ca();
    rig.clock.advance(Duration::hours(1));
    let (renewed, _) = renew(&rig, &signed);
    assert_eq!(
        renewed,
        RenewedIntermediate {
            intermediate_version: 2,
            intermediate_not_after: rig.clock.now() + Duration::days(365),
        }
    );
    assert_eq!(state_of(&rig, 1), IntermediateState::VerifyOnly);
    assert_eq!(state_of(&rig, 2), IntermediateState::Active);
    assert_eq!(rig.store.ca_material_count(), 2);
    let records = rig.store.load("ca").unwrap();
    assert_eq!(
        CaEvent::decode(records.last().unwrap()).unwrap(),
        CaEvent::IntermediateRenewed {
            at: rig.clock.now(),
            generation: 1,
            intermediate_version: 2,
            intermediate_not_after: renewed.intermediate_not_after,
        }
    );
    assert_eq!(
        rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "confirm".to_string()),
        ]
    );
}

#[test]
fn confirm_refuses_an_intermediate_that_is_not_the_one_of_the_ceremony() {
    let (rig, signed) = created_ca();
    let begun = rig.service.intermediate_begin().unwrap();
    let foreign = RootCaKey::generate().unwrap();
    let foreign_root = foreign
        .root_certificate(&trust_domain(), &serial(0x9c), rig.clock.now())
        .unwrap();
    let by_another_root =
        signed_intermediate(&rig, &foreign, &foreign_root, &begun.intermediate_csr);
    let error = rig
        .service
        .intermediate_confirm(&begun.ceremony_id, &by_another_root)
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
    let other = rig.pki.generate_key().unwrap();
    let other_request = rig
        .pki
        .create_request(
            &other,
            &hecate_application::ports::CertificateProfile::Intermediate {
                trust_domain: trust_domain(),
                generation: 1,
                version: 2,
            },
        )
        .unwrap();
    let other_key = signed_intermediate(&rig, &signed.root_key, &signed.root_der, &other_request);
    let error = rig
        .service
        .intermediate_confirm(&begun.ceremony_id, &other_key)
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
    assert_eq!(state_of(&rig, 1), IntermediateState::Active);
    assert_eq!(rig.store.ca_material_count(), 1);
    let unknown = rig
        .service
        .intermediate_confirm("unknown", &other_key)
        .err()
        .unwrap();
    assert!(is_conflict(&unknown), "{unknown:?}");
}

// --- After renewing ------------------------------------------------------------------------------

#[test]
fn after_renewing_issuance_uses_the_new_one_and_older_certificates_renew_with_it() {
    let (rig, signed) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let hades = Component::enrolled(&rig, "hades");
    let first = hermes.serial();
    let (_, new_intermediate) = renew(&rig, &signed);
    let caller = Component::enrolled(&rig, "caller");
    assert_eq!(
        caller.inner.certificate_chain().unwrap()[1],
        new_intermediate
    );
    let renewed = hermes.renewed(&rig);
    assert_eq!(renewed.chain[0], new_intermediate);
    let previous: Vec<Vec<u8>> = hermes
        .inner
        .certificate_chain_of(Some(&first))
        .unwrap()
        .into_iter()
        .take(2)
        .collect();
    assert_ne!(previous[1], new_intermediate);
    let revoked = hades.inner.certificate_chain().unwrap();
    rig.identities
        .certificates_revoke(hades.serial().as_str(), true)
        .unwrap();
    let crls = rig
        .identities
        .revocations(&caller.credentials(None))
        .unwrap()
        .crls;
    assert_eq!(crls.len(), 2, "the CRLs of both versions");
    caller.inner.keep_crls(&crls).unwrap();
    // Each certificate is checked against the CRL of its intermediate, which its key signed.
    assert!(caller.inner.accepts(&previous).unwrap());
    assert!(!caller.inner.accepts(&revoked).unwrap());
    assert!(
        caller
            .inner
            .accepts(&hermes.inner.certificate_chain().unwrap())
            .unwrap()
    );
}

// --- abort ---------------------------------------------------------------------------------------

#[test]
fn an_abort_with_its_reason_changes_nothing_and_does_not_count_for_the_common_counter() {
    let (rig, _) = created_ca();
    for reason in ["wrong_passphrase", "wrong_root_key", "cancelled"] {
        let begun = rig.service.intermediate_begin().unwrap();
        rig.service
            .intermediate_abort(&begun.ceremony_id, reason)
            .unwrap();
        assert!(is_conflict(
            &rig.service
                .intermediate_confirm(&begun.ceremony_id, "")
                .err()
                .unwrap()
        ));
    }
    assert_eq!(state_of(&rig, 1), IntermediateState::Active);
    assert_eq!(rig.store.ca_material_count(), 1);
    assert_eq!(rig.sealing.status().unwrap().consecutive_failures, 0);
    let begun = rig.service.intermediate_begin().unwrap();
    assert!(is_invalid(
        &rig.service
            .intermediate_abort(&begun.ceremony_id, "backup_check_failed")
            .err()
            .unwrap()
    ));
    assert!(is_conflict(
        &rig.service
            .intermediate_abort("unknown", "cancelled")
            .err()
            .unwrap()
    ));
    let details: Vec<String> = rows(&rig)
        .into_iter()
        .filter(|(_, detail)| detail.starts_with("abort"))
        .map(|(_, detail)| detail)
        .collect();
    assert_eq!(
        details,
        [
            "abort: wrong_passphrase",
            "abort: wrong_root_key",
            "abort: cancelled",
            "abort",
            "abort: cancelled",
        ]
    );
}

#[test]
fn the_ceremonies_of_creation_and_renewal_do_not_take_each_others_ids() {
    let (rig, _) = created_ca();
    let begun = rig.service.intermediate_begin().unwrap();
    assert!(is_conflict(
        &rig.service
            .create_abort(&begun.ceremony_id, "cancelled")
            .err()
            .unwrap()
    ));
    rig.service
        .intermediate_abort(&begun.ceremony_id, "cancelled")
        .unwrap();
}

// --- status --------------------------------------------------------------------------------------

#[test]
fn status_shows_the_days_left_under_30_and_0_once_expired_also_sealed() {
    let (rig, _) = created_ca();
    let days_left = |rig: &CaRig| {
        rig.service
            .status()
            .unwrap()
            .unwrap()
            .intermediate_days_left
    };
    assert_eq!(days_left(&rig), None);
    rig.clock.advance(Duration::days(335));
    assert_eq!(days_left(&rig), None);
    rig.clock.advance(Duration::days(1));
    assert_eq!(days_left(&rig), Some(29));
    rig.clock.advance(Duration::hours(12));
    assert_eq!(days_left(&rig), Some(29), "a part of a day counts as a day");
    rig.sealing.seal().unwrap();
    assert_eq!(days_left(&rig), Some(29));
    rig.clock.advance(Duration::days(29));
    assert_eq!(days_left(&rig), Some(0));
}

// --- Retirement ----------------------------------------------------------------------------------

#[test]
fn the_previous_key_is_destroyed_when_its_last_certificate_expires() {
    let (rig, signed) = created_ca();
    let _hermes = Component::enrolled(&rig, "hermes");
    rig.clock.advance(Duration::hours(1));
    renew(&rig, &signed);
    assert!(rig.service.retire_intermediates().unwrap().is_empty());
    rig.clock.advance(Duration::hours(47));
    assert!(
        rig.service
            .renew_own_certificate_if_due()
            .unwrap()
            .is_some()
    );
    rig.clock
        .advance(Duration::hours(24) - Duration::seconds(1));
    assert!(rig.service.retire_intermediates().unwrap().is_empty());
    assert_eq!(rig.store.ca_material_count(), 2);
    rig.clock.advance(Duration::seconds(1));
    rig.sealing.seal().unwrap();
    assert_eq!(rig.service.retire_intermediates().unwrap(), vec![(1, 1)]);
    assert_eq!(state_of(&rig, 1), IntermediateState::Retired);
    assert_eq!(state_of(&rig, 2), IntermediateState::Active);
    assert_eq!(rig.store.ca_material_count(), 1);
    let records = rig.store.load("ca").unwrap();
    assert_eq!(
        CaEvent::decode(records.last().unwrap()).unwrap(),
        CaEvent::IntermediateRetired {
            at: rig.clock.now(),
            generation: 1,
            intermediate_version: 1,
            reason: RetireReason::ExpiredCertificates,
        }
    );
    assert!(rig.service.retire_intermediates().unwrap().is_empty());
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

// --- Power cuts (T122) ---------------------------------------------------------------------------

#[test]
fn a_power_cut_in_the_confirmation_keeps_the_previous_intermediate_and_the_repetition_renews() {
    let (rig, signed) = created_ca();
    let begun = rig.service.intermediate_begin().unwrap();
    let certificate = signed_intermediate(
        &rig,
        &signed.root_key,
        &signed.root_der,
        &begun.intermediate_csr,
    );
    rig.store.fail_commits(true);
    assert!(
        rig.service
            .intermediate_confirm(&begun.ceremony_id, &certificate)
            .is_err()
    );
    rig.store.fail_commits(false);
    let ca = rig.service.authority().unwrap().unwrap();
    assert_eq!(ca.active().unwrap().version, 1);
    assert!(ca.intermediate(1, 2).is_none());
    assert_eq!(rig.store.ca_material_count(), 1);
    let renewed = rig
        .service
        .intermediate_confirm(&begun.ceremony_id, &certificate)
        .unwrap();
    assert_eq!(renewed.intermediate_version, 2);
}

#[test]
fn a_power_cut_in_the_retirement_keeps_the_key_and_the_next_run_destroys_it() {
    let (rig, signed) = created_ca();
    renew(&rig, &signed);
    rig.clock.advance(Duration::hours(73));
    rig.store.fail_commits(true);
    assert!(rig.service.retire_intermediates().is_err());
    rig.store.fail_commits(false);
    assert_eq!(state_of(&rig, 1), IntermediateState::VerifyOnly);
    assert_eq!(rig.store.ca_material_count(), 2);
    assert_eq!(rig.service.retire_intermediates().unwrap(), vec![(1, 1)]);
    assert_eq!(rig.store.ca_material_count(), 1);
}

// --- The instant of the retirement (T124) --------------------------------------------------------

#[test]
fn the_previous_key_goes_when_the_last_certificate_it_signed_expires() {
    let (rig, signed) = created_ca();
    assert_eq!(rig.service.next_retirement().unwrap(), None);
    // Hécate's certificate and the one of hermes, the last of version 1, end 72 hours after now.
    let _hermes = Component::enrolled(&rig, "hermes");
    rig.clock.advance(Duration::hours(1));
    renew(&rig, &signed);
    assert_eq!(
        rig.service.next_retirement().unwrap(),
        Some(Duration::hours(71))
    );
    rig.clock
        .advance(Duration::hours(71) - Duration::seconds(30));
    assert_eq!(
        rig.service.next_retirement().unwrap(),
        Some(Duration::seconds(30))
    );
    assert!(rig.service.retire_intermediates().unwrap().is_empty());
    rig.clock.advance(Duration::seconds(30));
    assert_eq!(rig.service.next_retirement().unwrap(), Some(Duration::ZERO));
    assert_eq!(rig.service.retire_intermediates().unwrap(), vec![(1, 1)]);
    assert_eq!(rig.service.next_retirement().unwrap(), None);
}
