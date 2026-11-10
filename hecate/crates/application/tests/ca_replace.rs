//! T105: the replacement of the CA (FR-006, FR-002, SC-007, SC-010, SC-018, research R9): a proof
//! that counts for the common counter and a new CA passphrase; the confirmation changes the
//! generation in one transaction, and everything of the previous CA stops being valid.

mod common;

use std::time::{Duration as StdDuration, Instant};

use common::*;
use hecate_application::ca::service::{CeremonyBegun, ReplacedCa};
use hecate_application::ports::{Clock, EventStore};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::ca::authority::IntermediateState;
use hecate_domain::events::{CaEvent, RetireReason};
use hecate_domain::identity::token::EnrollmentToken;
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

const NEW_CA_PASSPHRASE: &str = "harbor violet engine 77";

fn proof() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

fn rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaReplace)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn begin(rig: &CaRig) -> CeremonyBegun {
    rig.service
        .replace_begin(&proof(), NEW_CA_PASSPHRASE)
        .unwrap()
}

/// The whole replacement with a new root CA that the CLI generates.
fn replace(rig: &CaRig) -> (ReplacedCa, SignedCa) {
    let begun = begin(rig);
    let signed = sign(rig, &begun);
    let replaced = rig
        .service
        .replace_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    (replaced, signed)
}

fn renew_intermediate(rig: &CaRig, signed: &SignedCa) {
    let begun = rig.service.intermediate_begin().unwrap();
    let der = signed
        .root_key
        .sign_intermediate(
            &signed.root_der,
            &begun.intermediate_csr,
            &serial(0x9d),
            rig.clock.now(),
        )
        .unwrap();
    rig.service
        .intermediate_confirm(&begun.ceremony_id, &pem::encode("CERTIFICATE", &der))
        .unwrap();
}

fn generation(rig: &CaRig) -> u64 {
    rig.service.authority().unwrap().unwrap().generation
}

fn own_serial(rig: &CaRig) -> hecate_domain::identity::names::Serial {
    rig.service
        .status()
        .unwrap()
        .unwrap()
        .hecate_certificate
        .unwrap()
        .serial
}

fn failures(rig: &CaRig) -> u32 {
    rig.sealing.status().unwrap().consecutive_failures
}

// --- begin ---------------------------------------------------------------------------------------

#[test]
fn the_proof_is_the_passphrase_or_two_shares_and_a_wrong_one_counts() {
    let (rig, _) = created_ca();
    let wrong = rig
        .service
        .replace_begin(
            &json!({"passphrase": "not the passphrase"}),
            NEW_CA_PASSPHRASE,
        )
        .err()
        .unwrap();
    assert!(is_auth(&wrong), "{wrong:?}");
    assert_eq!(failures(&rig), 1);
    let begun = begin(&rig);
    assert_eq!(failures(&rig), 0, "a valid proof resets the counter");
    rig.service
        .replace_abort(&begun.ceremony_id, "cancelled")
        .unwrap();
    let shares = json!({"shares": [rig.shares[0], rig.shares[2]]});
    assert!(
        rig.service
            .replace_begin(&shares, NEW_CA_PASSPHRASE)
            .is_ok()
    );
}

#[test]
fn during_the_wait_of_the_common_counter_it_is_rate_limited() {
    let (rig, _) = created_ca();
    let wrong = json!({"passphrase": "not the passphrase"});
    for _ in 0..3 {
        let error = rig
            .service
            .replace_begin(&wrong, NEW_CA_PASSPHRASE)
            .err()
            .unwrap();
        assert!(is_auth(&error), "{error:?}");
    }
    let error = rig
        .service
        .replace_begin(&proof(), NEW_CA_PASSPHRASE)
        .err()
        .unwrap();
    assert_eq!(retry_after(&error), Some(60));
}

#[test]
fn the_new_ca_passphrase_follows_the_rules_of_fr_002() {
    let (rig, _) = created_ca();
    let short = rig
        .service
        .replace_begin(&proof(), "too short")
        .err()
        .unwrap();
    assert!(is_invalid(&short), "{short:?}");
    assert_eq!(failures(&rig), 0, "refused before the proof is checked");
    let unseal = rig
        .service
        .replace_begin(&proof(), GOOD_PASSPHRASE)
        .err()
        .unwrap();
    assert!(is_invalid(&unseal), "{unseal:?}");
    let shares = json!({"shares": [rig.shares[0], rig.shares[1]]});
    let with_shares = rig
        .service
        .replace_begin(&shares, GOOD_PASSPHRASE)
        .err()
        .unwrap();
    assert!(is_invalid(&with_shares), "{with_shares:?}");
}

#[test]
fn begin_conflicts_sealed_without_a_ca_or_with_another_ceremony() {
    let rig = ca_rig();
    assert!(is_conflict(
        &rig.service
            .replace_begin(&proof(), NEW_CA_PASSPHRASE)
            .err()
            .unwrap()
    ));
    let (rig, _) = created_ca();
    begin(&rig);
    assert!(is_conflict(
        &rig.service
            .replace_begin(&proof(), NEW_CA_PASSPHRASE)
            .err()
            .unwrap()
    ));
    rig.sealing.seal().unwrap();
    let error = rig
        .service
        .replace_begin(&proof(), NEW_CA_PASSPHRASE)
        .err()
        .unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}

// --- confirm -------------------------------------------------------------------------------------

#[test]
fn the_confirmation_starts_a_new_generation_and_ends_the_previous_ca() {
    let (rig, signed) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let pending = new_token(&rig, "hades");
    renew_intermediate(&rig, &signed);
    let old_own = own_serial(&rig);
    let (replaced, new_ca) = replace(&rig);
    let fingerprint = rig.service.status().unwrap().unwrap().root_fingerprint;
    assert_eq!(
        replaced,
        ReplacedCa {
            trust_domain: trust_domain(),
            root_fingerprint: fingerprint,
            generation: 2,
        }
    );
    assert_ne!(fingerprint, signed_fingerprint(&signed));
    let ca = rig.service.authority().unwrap().unwrap();
    assert_eq!(ca.generation, 2);
    assert_eq!(ca.active().unwrap().generation, 2);
    for version in [1, 2] {
        assert_eq!(
            ca.intermediate(1, version).unwrap().state,
            IntermediateState::Retired
        );
    }
    let events: Vec<CaEvent> = rig
        .store
        .load("ca")
        .unwrap()
        .iter()
        .map(|record| CaEvent::decode(record).unwrap())
        .collect();
    let retired: Vec<(u64, RetireReason)> = events
        .iter()
        .filter_map(|event| match event {
            CaEvent::IntermediateRetired {
                intermediate_version,
                reason,
                ..
            } => Some((*intermediate_version, *reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        retired,
        vec![(1, RetireReason::CaReplaced), (2, RetireReason::CaReplaced)]
    );
    assert!(matches!(
        events.last().unwrap(),
        CaEvent::CaReplaced { generation: 2, .. }
    ));
    assert_eq!(
        rig.store.ca_material_count(),
        1,
        "the previous keys are gone"
    );
    let new_own = own_serial(&rig);
    assert_ne!(new_own, old_own);
    let material = rig.service.tls_material().unwrap().unwrap();
    assert_eq!(material.chain[2], new_ca.root_der);
    assert_eq!(material.root, new_ca.root_der);
    assert!(is_auth(
        &rig.identities
            .keys(&hermes.credentials(None))
            .err()
            .unwrap()
    ));
    assert!(is_auth(&hermes.renew(&rig).err().unwrap()));
    let body = enroll_body(
        &pending,
        &request_for(&rig, Some("hades")),
        &call_key_pem(5),
    );
    assert!(is_auth(
        &rig.identities.enroll(Some("old"), &body).err().unwrap()
    ));
    assert!(rig.identities.tokens_list().unwrap().is_empty());
    let caller = Component::enrolled(&rig, "caller");
    let active: Vec<String> = rig
        .identities
        .keys(&caller.credentials(None))
        .unwrap()
        .keys
        .iter()
        .map(|key| key.identity.identity().to_string())
        .collect();
    assert_eq!(
        active,
        vec!["caller"],
        "the keys of the previous CA are gone"
    );
    assert_eq!(
        rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "confirm".to_string()),
        ]
    );
}

fn signed_fingerprint(signed: &SignedCa) -> hecate_domain::identity::names::RootFingerprint {
    hecate_domain::identity::names::RootFingerprint::of_der(&signed.root_der)
}

#[test]
fn a_new_token_carries_the_new_fingerprint_and_enrolls_within_a_second() {
    let (rig, _) = created_ca();
    let (replaced, new_ca) = replace(&rig);
    let token = new_token(&rig, "hermes");
    assert_eq!(
        *EnrollmentToken::parse(&token).unwrap().fingerprint(),
        replaced.root_fingerprint
    );
    let body = enroll_body(&token, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    let started = Instant::now();
    let enrolled = rig.identities.enroll(Some("k1"), &body).unwrap();
    assert!(started.elapsed() < StdDuration::from_secs(1));
    assert_eq!(enrolled.chain[1], new_ca.root_der);
}

#[test]
fn confirm_refuses_certificates_that_do_not_follow_the_profile() {
    let (rig, signed) = created_ca();
    let begun = begin(&rig);
    // The intermediate signed again by the current root CA: the root must be another one.
    let same_root = signed
        .root_key
        .sign_intermediate(
            &signed.root_der,
            &begun.intermediate_csr,
            &serial(0x9b),
            rig.clock.now(),
        )
        .unwrap();
    let error = rig
        .service
        .replace_confirm(
            &begun.ceremony_id,
            &signed.root,
            &pem::encode("CERTIFICATE", &same_root),
        )
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
    let other = sign(&rig, &begun);
    let swapped = rig
        .service
        .replace_confirm(&begun.ceremony_id, &other.intermediate, &other.root)
        .err()
        .unwrap();
    assert!(is_invalid(&swapped), "{swapped:?}");
    assert_eq!(generation(&rig), 1);
    assert!(
        rig.service
            .replace_confirm(&begun.ceremony_id, &other.root, &other.intermediate)
            .is_ok()
    );
}

// --- Interrupted ---------------------------------------------------------------------------------

#[test]
fn an_interrupted_replacement_leaves_nothing() {
    let (rig, _) = created_ca();
    let own = own_serial(&rig);
    let begun = begin(&rig);
    rig.service
        .replace_abort(&begun.ceremony_id, "backup_check_failed")
        .unwrap();
    let expired = begin(&rig);
    let signed = sign(&rig, &expired);
    rig.clock.advance(Duration::minutes(16));
    assert!(is_conflict(
        &rig.service
            .replace_confirm(&expired.ceremony_id, &signed.root, &signed.intermediate)
            .err()
            .unwrap()
    ));
    let lost = begin(&rig);
    let signed = sign(&rig, &lost);
    rig.sealing.seal().unwrap();
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert!(is_conflict(
        &rig.service
            .replace_confirm(&lost.ceremony_id, &signed.root, &signed.intermediate)
            .err()
            .unwrap()
    ));
    assert_eq!(generation(&rig), 1);
    assert_eq!(rig.store.ca_material_count(), 1);
    assert_eq!(own_serial(&rig), own);
    let fresh = new_token(&rig, "hermes");
    let body = enroll_body(&fresh, &request_for(&rig, Some("hermes")), &call_key_pem(1));
    assert!(rig.identities.enroll(Some("k1"), &body).is_ok());
    let details: Vec<String> = rows(&rig).into_iter().map(|(_, detail)| detail).collect();
    assert!(details.contains(&"abort: backup_check_failed".to_string()));
}

#[test]
fn a_power_cut_in_the_confirmation_changes_nothing() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let own = own_serial(&rig);
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    rig.store.fail_commits(true);
    assert!(
        rig.service
            .replace_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
            .is_err()
    );
    rig.store.fail_commits(false);
    assert_eq!(generation(&rig), 1);
    assert_eq!(rig.store.ca_material_count(), 1);
    assert_eq!(own_serial(&rig), own);
    assert!(rig.identities.keys(&hermes.credentials(None)).is_ok());
    assert!(
        rig.service
            .replace_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
            .is_ok(),
        "the ceremony is still pending"
    );
    assert_eq!(generation(&rig), 2);
}

#[test]
fn after_the_replacement_status_does_not_count_certificates_of_the_previous_ca() {
    let (rig, _) = created_ca();
    let _hermes = Component::enrolled(&rig, "hermes");
    replace(&rig);
    rig.clock.advance(Duration::hours(49));
    assert_eq!(rig.identities.counts().unwrap().identities_expiring_24h, 0);
}
