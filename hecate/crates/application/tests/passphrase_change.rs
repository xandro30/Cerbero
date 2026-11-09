mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::json;

const NEW: &str = "a brand new passphrase";

// py: unit/application/test_passphrase_change.py::test_change_with_the_current_passphrase
#[test]
fn change_with_the_current_passphrase() {
    let (rig, _) = unsealed();
    let before = rig.repo.config().unwrap();
    assert_eq!(
        rig.service
            .change_passphrase(&json!({"passphrase": GOOD_PASSPHRASE}), NEW)
            .unwrap(),
        2
    );
    let after = rig.repo.config().unwrap();
    assert!(after.config_version == 2 && after.updated_at >= before.updated_at);
    assert_eq!(after.recovery, before.recovery);
    assert_ne!(after.passphrase.salt, before.passphrase.salt);
    assert_eq!(after.initialized_at, before.initialized_at);
    let restarted = rig.restart();
    assert!(is_auth(
        &restarted
            .unseal_with_passphrase(GOOD_PASSPHRASE)
            .unwrap_err()
    ));
    restarted.unseal_with_passphrase(NEW).unwrap();
    let last = entries_of(&rig, Operation::PassphraseChange).pop().unwrap();
    assert_eq!(
        (last.result, last.method),
        (AccessResult::Success, Method::Passphrase)
    );
}

// py: unit/application/test_passphrase_change.py::test_shares_still_unseal_after_a_passphrase_change
#[test]
fn shares_still_unseal_after_a_passphrase_change() {
    let (rig, shares) = unsealed();
    rig.service
        .change_passphrase(&json!({"passphrase": GOOD_PASSPHRASE}), NEW)
        .unwrap();
    assert_eq!(
        rig.restart()
            .unseal_with_shares(&shares[1..])
            .unwrap()
            .state,
        SealState::Unsealed
    );
}

// py: unit/application/test_passphrase_change.py::test_change_proving_with_two_shares_clears_the_recommendation
#[test]
fn change_proving_with_two_shares_clears_the_recommendation() {
    let (rig, shares) = initialized();
    rig.service.unseal_with_shares(&shares[..2]).unwrap();
    assert!(rig.service.status().unwrap().recommend_passphrase_change);
    rig.service
        .change_passphrase(&json!({"shares": [shares[0], shares[2]]}), NEW)
        .unwrap();
    assert!(!rig.service.status().unwrap().recommend_passphrase_change);
    assert_eq!(
        entries_of(&rig, Operation::PassphraseChange)
            .pop()
            .unwrap()
            .method,
        Method::Recovery
    );
    rig.restart().unseal_with_passphrase(NEW).unwrap();
}

// py: unit/application/test_passphrase_change.py::test_without_a_valid_proof_nothing_changes_and_a_failure_is_counted
#[test]
fn without_a_valid_proof_nothing_changes_and_a_failure_is_counted() {
    let (rig, _) = unsealed();
    let before = rig.repo.config();
    assert!(is_auth(
        &rig.service
            .change_passphrase(&json!({"passphrase": "not the current one"}), NEW)
            .unwrap_err()
    ));
    let foreign = foreign_shares();
    assert!(is_auth(
        &rig.service
            .change_passphrase(&json!({"shares": foreign[..2]}), NEW)
            .unwrap_err()
    ));
    assert_eq!(rig.repo.config(), before);
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 2);
    assert!(!format!("{:?}", rig.log.entries()).contains(NEW));
}

// py: unit/application/test_passphrase_change.py::test_invalid_requests_are_validation_errors_and_not_failures
#[test]
fn invalid_requests_are_validation_errors_and_not_failures() {
    let (rig, shares) = unsealed();
    let before = rig.repo.config();
    let attempts = [
        (json!({"passphrase": GOOD_PASSPHRASE}), "short"),
        (json!({}), NEW),
        (
            json!({"passphrase": GOOD_PASSPHRASE, "shares": shares[..2]}),
            NEW,
        ),
        (json!({"shares": shares[..1]}), NEW),
    ];
    for (proof, new) in attempts {
        assert!(is_invalid(
            &rig.service.change_passphrase(&proof, new).unwrap_err()
        ));
    }
    assert_eq!(rig.repo.config(), before);
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_passphrase_change.py::test_change_needs_to_be_unsealed
#[test]
fn change_needs_to_be_unsealed() {
    let (rig, _) = unsealed();
    rig.service.seal().unwrap();
    let error = rig
        .service
        .change_passphrase(&json!({"passphrase": GOOD_PASSPHRASE}), NEW)
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
}
