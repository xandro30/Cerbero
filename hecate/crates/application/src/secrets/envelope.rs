//! Layered encryption (research R3): root -> scope key (one per identity) -> version key ->
//! value. Every layer is AES-256-GCM with associated data that ties it to its purpose, path,
//! version and identity, so moving encrypted data elsewhere makes it undecryptable.

use std::sync::Arc;

use hecate_domain::sensitive::{SecretBytes, SecretKey};
use serde_json::{Map, Value};

use crate::ports::{CryptoError, CryptoProvider, KeyMaterial, SecretMaterial};

pub const FINGERPRINT_BYTES: usize = 8;
const FINGERPRINT_INFO: &[u8] = b"hecate/fingerprint";

/// JSON with sorted keys and no spaces, byte for byte as the Python version writes it.
fn aad(fields: Vec<(&str, Value)>) -> Vec<u8> {
    let fields: Map<String, Value> = fields
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    Value::Object(fields).to_string().into_bytes()
}

pub fn scope_aad(identity: &str, key_version: u64) -> Vec<u8> {
    aad(vec![
        ("purpose", Value::from("scope_key")),
        ("identity", Value::from(identity)),
        ("key_version", Value::from(key_version)),
    ])
}

pub fn dek_aad(path: &str, version: u64, identity: &str, key_version: u64) -> Vec<u8> {
    aad(vec![
        ("purpose", Value::from("dek")),
        ("path", Value::from(path)),
        ("version", Value::from(version)),
        ("scope", Value::from(identity)),
        ("key_version", Value::from(key_version)),
    ])
}

pub fn value_aad(path: &str, version: u64, identity: &str) -> Vec<u8> {
    aad(vec![
        ("purpose", Value::from("secret")),
        ("path", Value::from(path)),
        ("version", Value::from(version)),
        ("scope", Value::from(identity)),
    ])
}

fn key_from(bytes: &SecretBytes) -> Result<SecretKey, CryptoError> {
    SecretKey::from_slice(bytes.expose())
        .ok_or_else(|| CryptoError("a wrapped key is not 32 bytes".into()))
}

pub struct Envelope {
    crypto: Arc<dyn CryptoProvider>,
}

impl Envelope {
    pub fn new(crypto: Arc<dyn CryptoProvider>) -> Self {
        Self { crypto }
    }

    /// A new scope key and its material, wrapped by the root.
    pub fn new_scope_key(
        &self,
        root: &SecretKey,
        identity: &str,
        key_version: u64,
    ) -> Result<(SecretKey, KeyMaterial), CryptoError> {
        let key = self.crypto.random_key();
        let (nonce, ciphertext) =
            self.crypto
                .aead_encrypt(root, key.expose(), &scope_aad(identity, key_version))?;
        Ok((key, KeyMaterial { nonce, ciphertext }))
    }

    pub fn open_scope_key(
        &self,
        root: &SecretKey,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<SecretKey, CryptoError> {
        let opened = self.crypto.aead_decrypt(
            root,
            &material.nonce,
            &material.ciphertext,
            &scope_aad(identity, key_version),
        )?;
        key_from(&opened)
    }

    /// Encrypts a value with a new version key, which the scope key wraps.
    pub fn seal_value(
        &self,
        scope_key: &SecretKey,
        path: &str,
        version: u64,
        identity: &str,
        key_version: u64,
        value: &[u8],
    ) -> Result<SecretMaterial, CryptoError> {
        let dek = self.crypto.random_key();
        let (value_nonce, value_ciphertext) =
            self.crypto
                .aead_encrypt(&dek, value, &value_aad(path, version, identity))?;
        let (dek_nonce, dek_ciphertext) = self.crypto.aead_encrypt(
            scope_key,
            dek.expose(),
            &dek_aad(path, version, identity, key_version),
        )?;
        Ok(SecretMaterial {
            key_version,
            value_nonce,
            value_ciphertext,
            dek_nonce,
            dek_ciphertext,
        })
    }

    pub fn open_value(
        &self,
        scope_key: &SecretKey,
        path: &str,
        version: u64,
        identity: &str,
        material: &SecretMaterial,
    ) -> Result<SecretBytes, CryptoError> {
        let dek = key_from(&self.crypto.aead_decrypt(
            scope_key,
            &material.dek_nonce,
            &material.dek_ciphertext,
            &dek_aad(path, version, identity, material.key_version),
        )?)?;
        self.crypto.aead_decrypt(
            &dek,
            &material.value_nonce,
            &material.value_ciphertext,
            &value_aad(path, version, identity),
        )
    }

    /// Keyed fingerprint: only Hécate can compute it, so it does not help guess weak values.
    pub fn fingerprint(&self, scope_key: &SecretKey, value: &[u8]) -> String {
        let key = self.crypto.hkdf_sha256(scope_key, FINGERPRINT_INFO);
        let digest = self.crypto.hmac_sha256(&key, value);
        let hex: String = digest[..FINGERPRINT_BYTES]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        hex.as_bytes()
            .chunks(4)
            .map(|group| std::str::from_utf8(group).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("-")
    }
}
