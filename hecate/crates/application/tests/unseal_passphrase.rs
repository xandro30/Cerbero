mod common;

use common::*;
use hecate_application::sealing::service::Unsealed;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::{SealState, UnsealMethod};
use time::Duration;

// py: unit/application/test_unseal_passphrase.py::test_correct_passphrase_unseals
#[test]
fn correct_passphrase_unseals() {
    let (rig, _) = initialized();
    let unsealed = rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert_eq!(
        unsealed,
        Unsealed {
            state: SealState::Unsealed,
            unsealed_via: UnsealMethod::Passphrase,
            recommend_passphrase_change: false
        }
    );
    let status = rig.service.status().unwrap();
    assert_eq!(
        (status.state, status.unsealed_via),
        (SealState::Unsealed, Some(UnsealMethod::Passphrase))
    );
    let entries = entries_of(&rig, Operation::Unseal);
    let success = entries
        .iter()
        .find(|entry| entry.result == AccessResult::Success)
        .unwrap();
    assert_eq!(success.method, Method::Passphrase);
}

// py: unit/application/test_unseal_passphrase.py::test_wrong_passphrase_stays_sealed_and_counts_a_failure
#[test]
fn wrong_passphrase_stays_sealed_and_counts_a_failure() {
    let (rig, _) = initialized();
    assert!(is_auth(
        &rig.service
            .unseal_with_passphrase("not the passphrase")
            .unwrap_err()
    ));
    let status = rig.service.status().unwrap();
    assert_eq!(
        (status.state, status.consecutive_failures),
        (SealState::Sealed, 1)
    );
    assert_eq!(
        with_result(&entries_of(&rig, Operation::Unseal), AccessResult::Failure),
        1
    );
    assert!(!format!("{:?}", rig.log.entries()).contains("not the passphrase"));
}

// py: unit/application/test_unseal_passphrase.py::test_wait_after_third_failure_blocks_even_the_right_passphrase
#[test]
fn wait_after_third_failure_blocks_even_the_right_passphrase() {
    let (rig, _) = initialized();
    for _ in 0..3 {
        assert!(is_auth(
            &rig.service
                .unseal_with_passphrase("wrong wrong wrong")
                .unwrap_err()
        ));
    }
    let error = rig
        .service
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap_err();
    assert_eq!(retry_after(&error), Some(60));
    assert_eq!(rig.service.status().unwrap().state, SealState::Sealed);
    assert_eq!(
        with_result(
            &entries_of(&rig, Operation::Unseal),
            AccessResult::RejectedRateLimited
        ),
        1
    );
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 3);
    rig.clock.advance(Duration::seconds(61));
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_unseal_passphrase.py::test_wait_grows_with_each_failure
#[test]
fn wait_grows_with_each_failure() {
    let (rig, _) = initialized();
    let mut waits = Vec::new();
    for _ in 0..5 {
        assert!(is_auth(
            &rig.service
                .unseal_with_passphrase("wrong wrong wrong")
                .unwrap_err()
        ));
        waits.push(rig.service.status().unwrap().retry_after_s);
        rig.clock.advance(Duration::minutes(20));
    }
    assert_eq!(waits, [0, 0, 60, 120, 240]);
}

// py: unit/application/test_unseal_passphrase.py::test_unsealing_when_unsealed_or_uninitialized_is_a_conflict
#[test]
fn unsealing_when_unsealed_or_uninitialized_is_a_conflict() {
    let (rig, _) = initialized();
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let error = rig
        .service
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Unsealed));
    assert!(
        with_result(
            &entries_of(&rig, Operation::Unseal),
            AccessResult::RejectedConflict
        ) > 0
    );
    let error = fresh_service()
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Uninitialized));
}

// py: unit/application/test_unseal_passphrase.py::test_non_text_passphrase_is_invalid_input_and_does_not_count
#[test]
fn non_text_passphrase_is_invalid_input_and_does_not_count() {
    let (rig, _) = initialized();
    assert!(is_invalid(
        &rig.service.unseal_with_passphrase("").unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_unseal_passphrase.py::test_failures_and_wait_survive_a_new_process
#[test]
fn failures_and_wait_survive_a_new_process() {
    let (rig, _) = initialized();
    for _ in 0..3 {
        assert!(is_auth(
            &rig.service
                .unseal_with_passphrase("wrong wrong wrong")
                .unwrap_err()
        ));
    }
    let restarted = rig.restart();
    assert_eq!(restarted.status().unwrap().state, SealState::Sealed);
    assert_eq!(restarted.status().unwrap().consecutive_failures, 3);
    assert!(
        retry_after(
            &restarted
                .unseal_with_passphrase(GOOD_PASSPHRASE)
                .unwrap_err()
        )
        .is_some()
    );
}
