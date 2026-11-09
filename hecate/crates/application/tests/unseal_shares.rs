mod common;

use common::*;
use hecate_application::sealing::service::Unsealed;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::{SealState, UnsealMethod};
use time::Duration;

// py: unit/application/test_unseal_shares.py::test_any_ordered_pair_of_shares_unseals_and_recommends_a_new_passphrase
#[test]
fn any_ordered_pair_of_shares_unseals_and_recommends_a_new_passphrase() {
    for order in [[0, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]] {
        let (rig, shares) = initialized();
        let pair = [shares[order[0]].clone(), shares[order[1]].clone()];
        let unsealed = rig.service.unseal_with_shares(&pair).unwrap();
        assert_eq!(
            unsealed,
            Unsealed {
                state: SealState::Unsealed,
                unsealed_via: UnsealMethod::Recovery,
                recommend_passphrase_change: true
            }
        );
        let status = rig.service.status().unwrap();
        assert!(status.state == SealState::Unsealed && status.recommend_passphrase_change);
        let last = entries_of(&rig, Operation::Unseal).pop().unwrap();
        assert_eq!(
            (last.result, last.method),
            (AccessResult::Success, Method::Recovery)
        );
    }
}

// py: unit/application/test_unseal_shares.py::test_all_three_shares_also_work
#[test]
fn all_three_shares_also_work() {
    let (rig, shares) = initialized();
    assert_eq!(
        rig.service.unseal_with_shares(&shares).unwrap().state,
        SealState::Unsealed
    );
}

// py: unit/application/test_unseal_shares.py::test_one_share_or_a_repeated_share_is_invalid_input_and_not_a_failure
#[test]
fn one_share_or_a_repeated_share_is_invalid_input_and_not_a_failure() {
    let (rig, shares) = initialized();
    assert!(is_invalid(
        &rig.service.unseal_with_shares(&shares[..1]).unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service
            .unseal_with_shares(&[shares[0].clone(), shares[0].clone()])
            .unwrap_err()
    ));
    let status = rig.service.status().unwrap();
    assert_eq!(
        (status.state, status.consecutive_failures),
        (SealState::Sealed, 0)
    );
}

// py: unit/application/test_unseal_shares.py::test_a_badly_copied_share_is_detected_without_spending_an_attempt
#[test]
fn a_badly_copied_share_is_detected_without_spending_an_attempt() {
    let (rig, shares) = initialized();
    let last = shares[1].chars().last().unwrap();
    let mangled = format!(
        "{}{}",
        &shares[1][..shares[1].len() - 1],
        if last != 'A' { 'A' } else { 'B' }
    );
    assert!(is_invalid(
        &rig.service
            .unseal_with_shares(&[shares[0].clone(), mangled])
            .unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
    assert!(rig.log.entries().iter().any(
        |entry| entry.result == AccessResult::InvalidInput && entry.method == Method::Recovery
    ));
}

// py: unit/application/test_unseal_shares.py::test_shares_from_another_set_are_an_authentication_failure
#[test]
fn shares_from_another_set_are_an_authentication_failure() {
    let (rig, shares) = initialized();
    let foreign = foreign_shares();
    assert!(is_auth(
        &rig.service.unseal_with_shares(&foreign[..2]).unwrap_err()
    ));
    assert!(is_auth(
        &rig.service
            .unseal_with_shares(&[shares[0].clone(), foreign[1].clone()])
            .unwrap_err()
    ));
    let status = rig.service.status().unwrap();
    assert_eq!(
        (status.state, status.consecutive_failures),
        (SealState::Sealed, 2)
    );
}

// py: unit/application/test_unseal_shares.py::test_failures_with_passphrase_and_with_shares_share_one_counter
#[test]
fn failures_with_passphrase_and_with_shares_share_one_counter() {
    let (rig, shares) = initialized();
    let foreign = foreign_shares();
    for _ in 0..2 {
        assert!(is_auth(
            &rig.service
                .unseal_with_passphrase("wrong wrong wrong")
                .unwrap_err()
        ));
    }
    assert!(is_auth(
        &rig.service.unseal_with_shares(&foreign[..2]).unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 3);
    assert_eq!(
        retry_after(&rig.service.unseal_with_shares(&shares[..2]).unwrap_err()),
        Some(60)
    );
    rig.clock.advance(Duration::seconds(61));
    assert_eq!(
        rig.service
            .unseal_with_shares(&shares[..2])
            .unwrap()
            .unsealed_via,
        UnsealMethod::Recovery
    );
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_unseal_shares.py::test_conflicts_when_unsealed_or_uninitialized
#[test]
fn conflicts_when_unsealed_or_uninitialized() {
    let (rig, shares) = initialized();
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let error = rig.service.unseal_with_shares(&shares[..2]).unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Unsealed));
    let error = fresh_service()
        .unseal_with_shares(&shares[..2])
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Uninitialized));
}
