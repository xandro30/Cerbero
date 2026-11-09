//! FR-007 (research R14): the values that carry transit secrets in the application layer erase
//! themselves when dropped, and the root lives in memory only while Hécate is unsealed.

mod common;

use common::*;
use hecate_application::sealing::root_holder::RootHolder;
use hecate_application::sealing::service::{InitBegun, RekeyBegun};
use hecate_application::secrets::service::Resolved;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::state::SealState;
use hecate_domain::sensitive::SecretKey;
use serde_json::json;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Compiles only for types that erase themselves when dropped.
fn erased_when_dropped<T: ZeroizeOnDrop>() {}

#[test]
fn the_values_that_carry_transit_secrets_are_erased_when_dropped() {
    erased_when_dropped::<RootHolder>();
    erased_when_dropped::<InitBegun>();
    erased_when_dropped::<RekeyBegun>();
    erased_when_dropped::<Resolved>();
}

#[test]
fn zeroize_leaves_the_root_at_zero_and_empties_shares_and_values() {
    let mut holder = RootHolder::new(SecretKey::new([0x5A; 32]));
    holder.zeroize();
    assert_eq!(holder.key().expose(), &[0; 32]);

    let mut begun = fresh_service().init_begin(GOOD_PASSPHRASE).unwrap();
    assert_eq!(begun.shares.len(), 3);
    begun.zeroize();
    assert!(begun.shares.is_empty());

    let (rig, _) = unsealed();
    let mut rekey = rig
        .service
        .rekey_begin(&json!({"passphrase": GOOD_PASSPHRASE}))
        .unwrap();
    rekey.zeroize();
    assert!(rekey.shares.is_empty());

    let secrets = secrets();
    create(
        &secrets,
        "secret://hermes/token",
        "resolved-value",
        "string",
    );
    let mut resolved = resolve_one(&secrets, "secret://hermes/token");
    resolved.zeroize();
    assert!(resolved.value.is_empty());
}

#[test]
fn the_root_lives_in_memory_only_while_unsealed() {
    let (rig, _) = unsealed();
    assert!(rig.service.root().is_ok());
    rig.service.seal().unwrap();
    assert_eq!(
        conflict_state(&rig.service.root().err().unwrap()),
        Some(SealState::Sealed)
    );
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    rig.service.shutdown();
    assert_eq!(
        conflict_state(&rig.service.root().err().unwrap()),
        Some(SealState::Sealed)
    );
}
