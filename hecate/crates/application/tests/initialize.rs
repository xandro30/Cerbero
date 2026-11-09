mod common;

use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::share::RecoveryShare;
use hecate_domain::sealing::state::SealState;
use time::Duration;

fn texts(begun: &[zeroize::Zeroizing<String>]) -> Vec<String> {
    begun.iter().map(|share| share.to_string()).collect()
}

// py: unit/application/test_initialize.py::test_begin_returns_three_distinct_shares_and_persists_nothing
#[test]
fn begin_returns_three_distinct_shares_and_persists_nothing() {
    let rig = rig();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    let shares: Vec<RecoveryShare> = begun
        .shares
        .iter()
        .map(|text| RecoveryShare::parse(text).unwrap())
        .collect();
    let mut indices: Vec<u8> = shares.iter().map(RecoveryShare::index).collect();
    indices.sort_unstable();
    assert_eq!(indices, [1, 2, 3]);
    assert!(shares[0].values() != shares[1].values() && shares[1].values() != shares[2].values());
    assert!(
        shares
            .iter()
            .all(|share| share.share_set_id() == shares[0].share_set_id())
    );
    assert!(!begun.init_id.is_empty());
    assert_eq!(begun.expires_at, rig.clock.now() + Duration::minutes(15));
    assert!(begun.warning.to_lowercase().contains("cannot be recovered"));
    assert!(rig.repo.config().is_none());
    assert_eq!(
        rig.service.status().unwrap().state,
        SealState::Uninitialized
    );
}

// py: unit/application/test_initialize.py::test_confirm_with_any_two_shown_shares_leaves_it_sealed
#[test]
fn confirm_with_any_two_shown_shares_leaves_it_sealed() {
    let rig = rig();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    let state = rig
        .service
        .init_confirm(&begun.init_id, &begun.shares[..2])
        .unwrap();
    assert_eq!(state, SealState::Sealed);
    assert_eq!(rig.service.status().unwrap().state, SealState::Sealed);
    assert_eq!(rig.repo.config().unwrap().config_version, 1);
    assert!(
        rig.log
            .entries()
            .iter()
            .any(|entry| entry.operation == Operation::Init
                && entry.result == AccessResult::Success
                && entry.detail == "confirm"
                && entry.method == Method::Local)
    );
}

// py: unit/application/test_initialize.py::test_stored_protections_recover_the_same_root
#[test]
fn stored_protections_recover_the_same_root() {
    for pair in [[0, 1], [0, 2], [1, 2]] {
        let rig = rig();
        let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
        rig.service
            .init_confirm(&begun.init_id, &begun.shares[..2])
            .unwrap();
        let config = rig.repo.config().unwrap();
        let protection = &config.passphrase;
        let key = rig
            .crypto
            .derive_key(
                GOOD_PASSPHRASE,
                &protection.salt,
                protection.kdf_opslimit,
                protection.kdf_memlimit as usize,
            )
            .unwrap();
        let from_passphrase = rig
            .crypto
            .aead_decrypt(
                &key,
                &protection.nonce,
                &protection.ciphertext,
                &protection.aad(),
            )
            .unwrap();
        let shares: Vec<RecoveryShare> = pair
            .iter()
            .map(|at| RecoveryShare::parse(&begun.shares[*at]).unwrap())
            .collect();
        let pairs: Vec<(u8, &[u8])> = shares
            .iter()
            .map(|share| (share.index(), share.values()))
            .collect();
        let recovery_key = rig.crypto.shamir_combine(&pairs).unwrap();
        let recovery = &config.recovery;
        let from_shares = rig
            .crypto
            .aead_decrypt(
                &recovery_key,
                &recovery.nonce,
                &recovery.ciphertext,
                &recovery.aad(),
            )
            .unwrap();
        assert_eq!(from_passphrase.expose().len(), 32);
        assert_eq!(from_passphrase.expose(), from_shares.expose());
        assert_eq!(
            recovery.share_set_id.as_slice(),
            shares[0].share_set_id().as_slice()
        );
        assert_eq!((recovery.threshold, recovery.share_count), (2, 3));
    }
}

// py: unit/application/test_initialize.py::test_confirm_rejects_shares_that_were_not_shown
#[test]
fn confirm_rejects_shares_that_were_not_shown() {
    let rig = rig();
    let other = fresh_service().init_begin(GOOD_PASSPHRASE).unwrap();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    let error = rig
        .service
        .init_confirm(&begun.init_id, &other.shares[..2])
        .unwrap_err();
    assert!(is_invalid(&error));
    assert!(rig.repo.config().is_none());
}

// py: unit/application/test_initialize.py::test_confirm_needs_two_distinct_shares
#[test]
fn confirm_needs_two_distinct_shares() {
    let rig = rig();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    let shares = texts(&begun.shares);
    let mangled = format!("{}AAAA", &shares[1][..shares[1].len() - 4]);
    for attempt in [
        vec![shares[0].clone()],
        vec![shares[0].clone(), shares[0].clone()],
        vec![shares[0].clone(), mangled],
    ] {
        assert!(is_invalid(
            &rig.service
                .init_confirm(&begun.init_id, &attempt)
                .unwrap_err()
        ));
    }
    assert_eq!(
        rig.service.status().unwrap().state,
        SealState::Uninitialized
    );
}

// py: unit/application/test_initialize.py::test_abort_expiry_and_unknown_id_persist_nothing
#[test]
fn abort_expiry_and_unknown_id_persist_nothing() {
    let rig = rig();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    rig.service.init_abort(&begun.init_id).unwrap();
    assert!(is_invalid(
        &rig.service
            .init_confirm(&begun.init_id, &begun.shares[..2])
            .unwrap_err()
    ));
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    rig.clock.advance(Duration::minutes(16));
    assert!(is_invalid(
        &rig.service
            .init_confirm(&begun.init_id, &begun.shares[..2])
            .unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service
            .init_confirm("unknown", &begun.shares[..2])
            .unwrap_err()
    ));
    assert!(rig.repo.config().is_none());
    assert_eq!(
        rig.service.status().unwrap().state,
        SealState::Uninitialized
    );
}

// py: unit/application/test_initialize.py::test_short_passphrase_is_rejected_without_leaving_a_pending_init
#[test]
fn short_passphrase_is_rejected_without_leaving_a_pending_init() {
    let rig = rig();
    assert!(is_invalid(&rig.service.init_begin("short").err().unwrap()));
    assert!(
        rig.log
            .entries()
            .iter()
            .any(|entry| entry.result == AccessResult::InvalidInput)
    );
}

// py: unit/application/test_initialize.py::test_cannot_initialize_twice
#[test]
fn cannot_initialize_twice() {
    let rig = rig();
    let begun = rig.service.init_begin(GOOD_PASSPHRASE).unwrap();
    rig.service
        .init_confirm(&begun.init_id, &begun.shares[..2])
        .unwrap();
    let before = rig.repo.config();
    let error = rig.service.init_begin(GOOD_PASSPHRASE).err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    let error = rig
        .service
        .init_confirm(&begun.init_id, &begun.shares[..2])
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    assert_eq!(rig.repo.config(), before);
    assert!(
        rig.log
            .entries()
            .iter()
            .any(|entry| entry.result == AccessResult::RejectedConflict)
    );
}
