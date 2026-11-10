//! T029: the envelope of the copy of the root CA key (contracts/formats.md): Argon2id of the CA
//! passphrase with the parameters of the profile and AES-256-GCM of the PKCS#8, with the header
//! and the root fingerprint as associated data.

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use hecate_application::ports::CryptoProvider;
use hecate_application::testing::MINIMAL_KDF;
use hecate_domain::ca::backup::{self, HEADER_LEN, associated_data};
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::backup::{DoesNotOpen, open_backup, seal_backup};
use hecate_infrastructure::crypto::SoftwareCryptoProvider;

const PASSPHRASE: &str = "a long CA passphrase";
const PKCS8: &[u8] = b"the PKCS#8 of the root CA key";

fn sealed(crypto: &SoftwareCryptoProvider) -> (String, RootFingerprint) {
    let fingerprint = RootFingerprint::of_der(b"root certificate");
    let text = seal_backup(
        crypto,
        &SecretBytes::new(PKCS8.to_vec()),
        PASSPHRASE,
        MINIMAL_KDF,
        &fingerprint,
    )
    .unwrap();
    (text, fingerprint)
}

#[test]
fn the_copy_encrypts_the_pkcs8_with_argon2id_and_aes_256_gcm() {
    let crypto = SoftwareCryptoProvider::new();
    let (text, fingerprint) = sealed(&crypto);
    let (header, ciphertext) = backup::decode(&text).unwrap();
    assert_eq!(header.opslimit, MINIMAL_KDF.opslimit);
    assert_eq!(header.memlimit, MINIMAL_KDF.memlimit as u64);
    assert_eq!(ciphertext.len(), PKCS8.len() + 16);
    let key = crypto
        .derive_key(
            PASSPHRASE,
            &header.salt,
            header.opslimit,
            MINIMAL_KDF.memlimit,
        )
        .unwrap();
    let cipher = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key.expose()).unwrap());
    let mut in_out = ciphertext.clone();
    let plaintext = cipher
        .open_in_place(
            Nonce::assume_unique_for_key(header.nonce),
            Aad::from(associated_data(&header, &fingerprint)),
            &mut in_out,
        )
        .unwrap();
    assert_eq!(plaintext, PKCS8);
    let (again, _) = sealed(&crypto);
    let (other, _) = backup::decode(&again).unwrap();
    assert_ne!((other.salt, other.nonce), (header.salt, header.nonce));
}

#[test]
fn the_copy_opens_with_its_passphrase_and_its_root_fingerprint() {
    let crypto = SoftwareCryptoProvider::new();
    let (text, fingerprint) = sealed(&crypto);
    let pkcs8 = open_backup(&crypto, &text, PASSPHRASE, &fingerprint).unwrap();
    assert_eq!(pkcs8.expose(), PKCS8);
}

#[test]
fn another_passphrase_or_another_root_fail_without_saying_which() {
    let crypto = SoftwareCryptoProvider::new();
    let (text, fingerprint) = sealed(&crypto);
    let wrong_passphrase = open_backup(&crypto, &text, "another CA passphrase", &fingerprint)
        .err()
        .unwrap();
    let other_root = RootFingerprint::of_der(b"another root certificate");
    let wrong_root = open_backup(&crypto, &text, PASSPHRASE, &other_root)
        .err()
        .unwrap();
    assert_eq!(wrong_passphrase, DoesNotOpen);
    assert_eq!(wrong_root, wrong_passphrase);
    assert_eq!(wrong_root.to_string(), wrong_passphrase.to_string());
}

#[test]
fn a_changed_byte_is_detected() {
    let crypto = SoftwareCryptoProvider::new();
    let (text, fingerprint) = sealed(&crypto);
    let content = hecate_domain::pem::single(&text, backup::BACKUP_LABEL).unwrap();
    // The mark, the low bytes of both Argon2id parameters, the salt, the nonce and the ciphertext:
    // changing a high byte of a parameter would only ask for an absurd cost before failing.
    for position in [0, 11, 19, 20, 35, 36, 47, HEADER_LEN, content.len() - 1] {
        let mut changed = content.clone();
        changed[position] ^= 0x01;
        let changed = hecate_domain::pem::encode(backup::BACKUP_LABEL, &changed);
        assert_eq!(
            open_backup(&crypto, &changed, PASSPHRASE, &fingerprint).err(),
            Some(DoesNotOpen),
            "byte {position}"
        );
    }
    assert_eq!(
        open_backup(&crypto, "not a copy", PASSPHRASE, &fingerprint).err(),
        Some(DoesNotOpen)
    );
}
