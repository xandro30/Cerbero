mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;

// py: unit/application/test_seal.py::test_sealing_unsealed_hecate_wipes_the_root_and_logs_it
#[test]
fn sealing_unsealed_hecate_wipes_the_root_and_logs_it() {
    let (rig, _) = unsealed();
    assert!(rig.service.root().is_ok());
    assert_eq!(rig.service.seal().unwrap(), SealState::Sealed);
    let error = rig.service.root().err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    let status = rig.service.status().unwrap();
    assert_eq!(status.state, SealState::Sealed);
    assert!(status.unsealed_via.is_none() && !status.recommend_passphrase_change);
    let last = entries_of(&rig, Operation::Seal).pop().unwrap();
    assert_eq!(
        (last.result, last.method),
        (AccessResult::Success, Method::Local)
    );
}

// py: unit/application/test_seal.py::test_sealing_when_sealed_is_a_conflict_that_changes_nothing
#[test]
fn sealing_when_sealed_is_a_conflict_that_changes_nothing() {
    let (rig, _) = initialized();
    assert_eq!(
        conflict_state(&rig.service.seal().unwrap_err()),
        Some(SealState::Sealed)
    );
    assert_eq!(rig.service.status().unwrap().state, SealState::Sealed);
    assert_eq!(
        entries_of(&rig, Operation::Seal).pop().unwrap().result,
        AccessResult::RejectedConflict
    );
}

// py: unit/application/test_seal.py::test_sealing_when_uninitialized_is_a_conflict
#[test]
fn sealing_when_uninitialized_is_a_conflict() {
    assert_eq!(
        conflict_state(&fresh_service().seal().unwrap_err()),
        Some(SealState::Uninitialized)
    );
}

// py: unit/application/test_seal.py::test_can_unseal_again_after_sealing
#[test]
fn can_unseal_again_after_sealing() {
    let (rig, _) = unsealed();
    rig.service.seal().unwrap();
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert_eq!(rig.service.status().unwrap().state, SealState::Unsealed);
}
