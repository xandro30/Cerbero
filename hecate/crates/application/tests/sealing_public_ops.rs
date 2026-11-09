mod common;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, initialize};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

// py: unit/application/test_sealing_public_ops.py::test_root_is_available_only_while_unsealed
#[test]
fn root_is_available_only_while_unsealed() {
    let rig = rig();
    assert_eq!(
        conflict_state(&rig.service.root().err().unwrap()),
        Some(SealState::Uninitialized)
    );
    initialize(&rig.service, GOOD_PASSPHRASE);
    assert_eq!(
        conflict_state(&rig.service.root().err().unwrap()),
        Some(SealState::Sealed)
    );
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert_eq!(rig.service.root().unwrap().expose().len(), 32);
    rig.service.seal().unwrap();
    assert!(conflict_state(&rig.service.root().err().unwrap()).is_some());
}

// py: unit/application/test_sealing_public_ops.py::test_root_is_the_same_across_calls_and_unseal_methods
#[test]
fn root_is_the_same_across_calls_and_unseal_methods() {
    let (rig, shares) = unsealed();
    let first = *rig.service.root().unwrap().expose();
    assert_eq!(*rig.service.root().unwrap().expose(), first);
    rig.service.seal().unwrap();
    rig.service.unseal_with_shares(&shares[..2]).unwrap();
    assert_eq!(*rig.service.root().unwrap().expose(), first);
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_accepts_the_current_passphrase
#[test]
fn verify_proof_accepts_the_current_passphrase() {
    let (rig, _) = unsealed();
    rig.service
        .verify_proof(&json!({"passphrase": GOOD_PASSPHRASE}))
        .unwrap();
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_accepts_two_shares
#[test]
fn verify_proof_accepts_two_shares() {
    let (rig, shares) = unsealed();
    rig.service
        .verify_proof(&json!({"shares": shares[1..]}))
        .unwrap();
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_a_wrong_passphrase_and_counts_the_failure
#[test]
fn verify_proof_rejects_a_wrong_passphrase_and_counts_the_failure() {
    let (rig, _) = unsealed();
    assert!(is_auth(
        &rig.service
            .verify_proof(&json!({"passphrase": "wrong wrong wrong"}))
            .unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 1);
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_wrong_shares_and_counts_the_failure
#[test]
fn verify_proof_rejects_wrong_shares_and_counts_the_failure() {
    let (rig, shares) = unsealed();
    let other = foreign_shares();
    assert!(is_auth(
        &rig.service
            .verify_proof(&json!({"shares": other[..2]}))
            .unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 1);
    rig.service
        .verify_proof(&json!({"shares": shares[..2]}))
        .unwrap();
}

// py: unit/application/test_sealing_public_ops.py::test_the_counter_is_shared_with_unsealing_and_resets_on_success
#[test]
fn the_counter_is_shared_with_unsealing_and_resets_on_success() {
    let (rig, _) = unsealed();
    for _ in 0..2 {
        assert!(is_auth(
            &rig.service
                .verify_proof(&json!({"passphrase": "wrong wrong wrong"}))
                .unwrap_err()
        ));
    }
    rig.service.seal().unwrap();
    assert!(is_auth(
        &rig.service
            .unseal_with_passphrase("wrong wrong wrong")
            .unwrap_err()
    ));
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 3);
    rig.clock.advance(Duration::seconds(61));
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    rig.service
        .verify_proof(&json!({"passphrase": GOOD_PASSPHRASE}))
        .unwrap();
    assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_respects_the_wait_even_with_a_correct_proof
#[test]
fn verify_proof_respects_the_wait_even_with_a_correct_proof() {
    let (rig, _) = unsealed();
    for _ in 0..3 {
        assert!(is_auth(
            &rig.service
                .verify_proof(&json!({"passphrase": "wrong wrong wrong"}))
                .unwrap_err()
        ));
    }
    let error = rig
        .service
        .verify_proof(&json!({"passphrase": GOOD_PASSPHRASE}))
        .unwrap_err();
    assert_eq!(retry_after(&error), Some(60));
    rig.clock.advance(Duration::seconds(61));
    rig.service
        .verify_proof(&json!({"passphrase": GOOD_PASSPHRASE}))
        .unwrap();
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_malformed_proofs_without_counting
#[test]
fn verify_proof_rejects_malformed_proofs_without_counting() {
    for proof in [
        Value::Null,
        json!({}),
        json!({"passphrase": "x", "shares": []}),
        json!({"passphrase": ""}),
        json!("text"),
    ] {
        let (rig, _) = unsealed();
        assert!(
            is_invalid(&rig.service.verify_proof(&proof).unwrap_err()),
            "{proof}"
        );
        assert_eq!(rig.service.status().unwrap().consecutive_failures, 0);
    }
}

// py: unit/application/test_sealing_public_ops.py::test_verify_proof_needs_the_service_unsealed
#[test]
fn verify_proof_needs_the_service_unsealed() {
    let (rig, _) = unsealed();
    rig.service.seal().unwrap();
    assert!(
        conflict_state(
            &rig.service
                .verify_proof(&json!({"passphrase": GOOD_PASSPHRASE}))
                .unwrap_err()
        )
        .is_some()
    );
}
