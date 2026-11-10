//! T036, T112: the root CA key in the CLI, the opened copy and Hécate's key read from the store
//! live in memory that is erased (FR-028).

mod common;

use common::*;
use hecate_application::ports::{IdentityStore, OwnTlsKey};
use hecate_application::testing::MINIMAL_KDF;
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::identity::names::Serial;
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::backup::{open_backup, seal_backup};
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::pki::RootCaKey;
use hecate_infrastructure::store::ca::SqliteIdentityStore;
use zeroize::{Zeroize, ZeroizeOnDrop};

fn erased_when_dropped<T: ZeroizeOnDrop>() {}

#[test]
fn the_guardian_of_the_root_ca_key_erases_it() {
    erased_when_dropped::<RootCaKey>();
    let mut key = RootCaKey::generate().unwrap();
    assert!(!key.pkcs8().expose().is_empty());
    key.zeroize();
    assert!(key.pkcs8().expose().is_empty());
}

#[test]
fn the_opened_copy_lives_in_memory_that_is_erased() {
    erased_when_dropped::<SecretBytes>();
    let crypto = SoftwareCryptoProvider::new();
    let key = RootCaKey::generate().unwrap();
    let fingerprint = RootFingerprint::of_der(b"root");
    let copy = seal_backup(
        &crypto,
        key.pkcs8(),
        CA_PASSPHRASE,
        MINIMAL_KDF,
        &fingerprint,
    )
    .unwrap();
    let mut opened = open_backup(&crypto, &copy, CA_PASSPHRASE, &fingerprint).unwrap();
    assert_eq!(opened.expose(), key.pkcs8().expose());
    opened.zeroize();
    assert!(opened.expose().is_empty());
}

#[test]
fn hecate_key_read_from_the_store_lives_in_memory_that_is_erased() {
    erased_when_dropped::<OwnTlsKey>();
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteIdentityStore::open(&dir.path().join("hecate.db")).unwrap();
    store
        .transaction(&mut |writes| {
            writes.save_own_tls_key(&OwnTlsKey {
                serial: Serial::parse("5f1c").unwrap(),
                private_key: SecretBytes::new(b"pkcs8 of hecate".to_vec()),
            })
        })
        .unwrap();
    let mut key = store.load_own_tls_key().unwrap().unwrap();
    assert_eq!(key.private_key.expose(), b"pkcs8 of hecate");
    key.zeroize();
    assert!(key.private_key.expose().is_empty());
}
