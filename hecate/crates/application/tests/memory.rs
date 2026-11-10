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

// --- The CA (feature 003, T036) ---------------------------------------------------------------

use hecate_application::ca::service::TlsMaterial;
use hecate_application::ports::OwnTlsKey;
use hecate_domain::sensitive::SecretBytes;

#[test]
fn the_values_that_carry_keys_of_the_ca_are_erased_when_dropped() {
    erased_when_dropped::<TlsMaterial>();
    erased_when_dropped::<OwnTlsKey>();
    // The key of an intermediate, decrypted for one operation that signs.
    erased_when_dropped::<SecretBytes>();
}

#[test]
fn the_ca_passphrase_is_dropped_after_begin() {
    let rig = ca_rig();
    let begun = begin(&rig);
    assert!(!format!("{begun:?}").contains(CA_PASSPHRASE));
    let signed = sign(&rig, &begun);
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    for entry in rig.access_log.entries() {
        assert!(!format!("{entry:?}").contains(CA_PASSPHRASE));
    }
    let contents = rig.store.contents();
    assert!(
        !contents
            .windows(CA_PASSPHRASE.len())
            .any(|window| window == CA_PASSPHRASE.as_bytes())
    );
}

#[test]
fn zeroize_empties_the_material_of_the_api_and_hecate_key() {
    let (rig, _) = created_ca();
    let mut material = rig.service.tls_material().unwrap().unwrap();
    material.zeroize();
    assert!(material.chain.is_empty() && material.root.is_empty());
    assert!(material.key.expose().is_empty());
    let mut own = hecate_application::ports::IdentityStore::load_own_tls_key(rig.store.as_ref())
        .unwrap()
        .unwrap();
    own.zeroize();
    assert!(own.private_key.expose().is_empty());
}

#[test]
fn the_enrollment_token_lives_in_memory_that_is_erased() {
    erased_when_dropped::<SecretKey>();
    erased_when_dropped::<zeroize::Zeroizing<String>>();
    let (rig, _) = created_ca();
    let mut created = rig.identities.tokens_create("hermes").unwrap();
    let text = created.token.to_string();
    let parsed = hecate_domain::identity::token::EnrollmentToken::parse(&text).unwrap();
    assert!(!format!("{parsed:?}").contains(text.split('.').nth(3).unwrap()));
    created.token.zeroize();
    assert!(created.token.is_empty());
    for entry in rig.access_log.entries() {
        assert!(!format!("{entry:?}").contains(&text));
    }
}
