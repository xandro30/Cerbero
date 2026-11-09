mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::share::RecoveryShare;
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

fn proof() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

fn texts(shares: &[zeroize::Zeroizing<String>]) -> Vec<String> {
    shares.iter().map(|share| share.to_string()).collect()
}

// py: unit/application/test_shares_rekey.py::test_rekey_replaces_the_shares_and_leaves_everything_else
#[test]
fn rekey_replaces_the_shares_and_leaves_everything_else() {
    let (rig, old) = unsealed();
    let before = rig.repo.config().unwrap();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    let new = texts(&begun.shares);
    assert!(new.len() == 3 && new.iter().all(|share| !old.contains(share)));
    assert_eq!(rig.repo.config().unwrap(), before);
    assert_eq!(
        rig.service
            .rekey_confirm(&begun.rekey_id, &new[..2])
            .unwrap(),
        2
    );
    let after = rig.repo.config().unwrap();
    assert_eq!(after.passphrase, before.passphrase);
    assert_ne!(after.recovery.share_set_id, before.recovery.share_set_id);
    assert_eq!(after.initialized_at, before.initialized_at);
    let restarted = rig.restart();
    assert!(is_auth(
        &restarted.unseal_with_shares(&old[..2]).unwrap_err()
    ));
    assert_eq!(
        restarted
            .unseal_with_shares(&[new[2].clone(), new[0].clone()])
            .unwrap()
            .state,
        SealState::Unsealed
    );
    restarted.seal().unwrap();
    assert_eq!(
        restarted
            .unseal_with_passphrase(GOOD_PASSPHRASE)
            .unwrap()
            .state,
        SealState::Unsealed
    );
}

// py: unit/application/test_shares_rekey.py::test_new_shares_still_work_after_a_later_passphrase_change
#[test]
fn new_shares_still_work_after_a_later_passphrase_change() {
    let (rig, _) = unsealed();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    rig.service
        .rekey_confirm(&begun.rekey_id, &begun.shares[..2])
        .unwrap();
    rig.service
        .change_passphrase(&proof(), "a brand new passphrase")
        .unwrap();
    assert_eq!(
        rig.restart()
            .unseal_with_shares(&begun.shares[1..])
            .unwrap()
            .state,
        SealState::Unsealed
    );
}

// py: unit/application/test_shares_rekey.py::test_the_proof_can_be_two_current_shares
#[test]
fn the_proof_can_be_two_current_shares() {
    let (rig, old) = unsealed();
    let begun = rig
        .service
        .rekey_begin(&json!({"shares": old[..2]}))
        .unwrap();
    assert!(!begun.rekey_id.is_empty());
    let last = entries_of(&rig, Operation::SharesRekey).pop().unwrap();
    assert_eq!(
        (last.method, last.result),
        (Method::Recovery, AccessResult::Success)
    );
}

// py: unit/application/test_shares_rekey.py::test_invalid_proof_nothing_pending_and_a_failure_is_counted
#[test]
fn invalid_proof_nothing_pending_and_a_failure_is_counted() {
    let (rig, _) = unsealed();
    let before = rig.repo.config();
    assert!(is_auth(
        &rig.service
            .rekey_begin(&json!({"passphrase": "not the current one"}))
            .err()
            .unwrap()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 1);
    assert!(is_invalid(
        &rig.service
            .rekey_confirm("anything", &["x", "y"])
            .unwrap_err()
    ));
    assert_eq!(rig.repo.config(), before);
}

// py: unit/application/test_shares_rekey.py::test_malformed_proofs_are_validation_errors_and_not_counted
#[test]
fn malformed_proofs_are_validation_errors_and_not_counted() {
    let (rig, old) = unsealed();
    let malformed = [
        json!({}),
        json!("x"),
        json!({"passphrase": 5}),
        json!({"shares": old[..1]}),
        json!({"passphrase": GOOD_PASSPHRASE, "shares": old[..2]}),
    ];
    for bad in malformed {
        assert!(
            is_invalid(&rig.service.rekey_begin(&bad).err().unwrap()),
            "{bad}"
        );
    }
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_shares_rekey.py::test_confirming_with_the_old_or_wrong_shares_changes_nothing
#[test]
fn confirming_with_the_old_or_wrong_shares_changes_nothing() {
    let (rig, old) = unsealed();
    let before = rig.repo.config();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    assert!(is_invalid(
        &rig.service
            .rekey_confirm(&begun.rekey_id, &old[..2])
            .unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service
            .rekey_confirm(&begun.rekey_id, &begun.shares[..1])
            .unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service
            .rekey_confirm("another id", &begun.shares[..2])
            .unwrap_err()
    ));
    assert_eq!(rig.repo.config(), before);
}

// py: unit/application/test_shares_rekey.py::test_abort_and_expiry_discard_the_pending_set
#[test]
fn abort_and_expiry_discard_the_pending_set() {
    let (rig, _) = unsealed();
    let before = rig.repo.config();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    rig.service.rekey_abort(&begun.rekey_id).unwrap();
    assert!(is_invalid(
        &rig.service
            .rekey_confirm(&begun.rekey_id, &begun.shares[..2])
            .unwrap_err()
    ));
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    rig.clock.advance(Duration::minutes(16));
    assert!(is_invalid(
        &rig.service
            .rekey_confirm(&begun.rekey_id, &begun.shares[..2])
            .unwrap_err()
    ));
    assert_eq!(rig.repo.config(), before);
}

// py: unit/application/test_shares_rekey.py::test_rekey_needs_to_be_unsealed_and_sealing_drops_the_pending_set
#[test]
fn rekey_needs_to_be_unsealed_and_sealing_drops_the_pending_set() {
    let (rig, _) = unsealed();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    rig.service.seal().unwrap();
    assert_eq!(
        conflict_state(&rig.service.rekey_begin(&proof()).err().unwrap()),
        Some(SealState::Sealed)
    );
    assert!(
        conflict_state(
            &rig.service
                .rekey_confirm(&begun.rekey_id, &begun.shares[..2])
                .unwrap_err()
        )
        .is_some()
    );
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert!(is_invalid(
        &rig.service
            .rekey_confirm(&begun.rekey_id, &begun.shares[..2])
            .unwrap_err()
    ));
    assert_eq!(rig.repo.config().unwrap().config_version, 1);
}

// py: unit/application/test_shares_rekey.py::test_new_shares_are_valid_shares_of_a_new_set
#[test]
fn new_shares_are_valid_shares_of_a_new_set() {
    let (rig, old) = unsealed();
    let begun = rig.service.rekey_begin(&proof()).unwrap();
    let new: Vec<RecoveryShare> = begun
        .shares
        .iter()
        .map(|text| RecoveryShare::parse(text).unwrap())
        .collect();
    let first = RecoveryShare::parse(&old[0]).unwrap();
    assert!(
        new.iter()
            .all(|share| share.share_set_id() != first.share_set_id())
    );
    let mut indices: Vec<u8> = new.iter().map(RecoveryShare::index).collect();
    indices.sort_unstable();
    assert_eq!(indices, [1, 2, 3]);
}
