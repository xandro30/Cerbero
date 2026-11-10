//! The envelope of the copy of the root CA key (contracts/formats.md): Argon2id of the CA
//! passphrase with the parameters of the KDF profile, and AES-256-GCM of the PKCS#8 with the
//! header and the root fingerprint as associated data. The nonce is part of the header, so the
//! encryption uses aws-lc-rs directly instead of the `CryptoProvider`, which picks its own nonce.

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use hecate_application::ports::CryptoProvider;
use hecate_application::sealing::settings::KdfLimits;
use hecate_domain::ca::backup::{self, BackupHeader, NONCE_LEN, SALT_LEN, associated_data};
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use zeroize::Zeroizing;

/// A copy that does not open: a wrong passphrase, the copy of another root CA, a changed byte or a
/// file of another format all give this same error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the copy does not open with that passphrase")]
pub struct DoesNotOpen;

fn cipher(key: &SecretKey) -> Result<LessSafeKey, DoesNotOpen> {
    UnboundKey::new(&AES_256_GCM, key.expose())
        .map(LessSafeKey::new)
        .map_err(|_| DoesNotOpen)
}

fn array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut array = [0; N];
    array.copy_from_slice(&bytes[..N]);
    array
}

/// The copy of a PKCS#8, in PEM, with a fresh salt and nonce.
pub fn seal_backup(
    crypto: &dyn CryptoProvider,
    pkcs8: &SecretBytes,
    passphrase: &str,
    limits: KdfLimits,
    fingerprint: &RootFingerprint,
) -> Result<String, DoesNotOpen> {
    let header = BackupHeader {
        opslimit: limits.opslimit,
        memlimit: limits.memlimit as u64,
        salt: array::<SALT_LEN>(&crypto.random_bytes(SALT_LEN)),
        nonce: array::<NONCE_LEN>(&crypto.random_bytes(NONCE_LEN)),
    };
    let key = crypto
        .derive_key(passphrase, &header.salt, limits.opslimit, limits.memlimit)
        .map_err(|_| DoesNotOpen)?;
    let mut sealed = Zeroizing::new(pkcs8.expose().to_vec());
    cipher(&key)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(header.nonce),
            Aad::from(associated_data(&header, fingerprint)),
            &mut *sealed,
        )
        .map_err(|_| DoesNotOpen)?;
    Ok(backup::encode(&header, &sealed))
}

/// The PKCS#8 of a copy, with its passphrase and next to the certificate of its root CA.
pub fn open_backup(
    crypto: &dyn CryptoProvider,
    text: &str,
    passphrase: &str,
    fingerprint: &RootFingerprint,
) -> Result<SecretBytes, DoesNotOpen> {
    let (header, ciphertext) = backup::decode(text).map_err(|_| DoesNotOpen)?;
    let memlimit = usize::try_from(header.memlimit).map_err(|_| DoesNotOpen)?;
    let key = crypto
        .derive_key(passphrase, &header.salt, header.opslimit, memlimit)
        .map_err(|_| DoesNotOpen)?;
    let mut opened = Zeroizing::new(ciphertext);
    let plaintext = cipher(&key)?
        .open_in_place(
            Nonce::assume_unique_for_key(header.nonce),
            Aad::from(associated_data(&header, fingerprint)),
            opened.as_mut(),
        )
        .map_err(|_| DoesNotOpen)?;
    Ok(SecretBytes::new(plaintext.to_vec()))
}
