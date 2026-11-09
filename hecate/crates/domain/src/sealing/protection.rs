use serde_json::{Map, Value};
use time::OffsetDateTime;

pub const AEAD_NAME: &str = "AES-256-GCM";
pub const KDF_NAME: &str = "argon2id";
pub const SHARE_THRESHOLD: u32 = 2;
pub const SHARE_COUNT: u32 = 3;

/// AEAD associated data that binds the root's ciphertext to its purpose, protection and version:
/// JSON with sorted keys and no spaces, byte for byte as in the Python version.
pub fn associated_data(protection: &str, config_version: u64) -> Vec<u8> {
    let mut fields = Map::new();
    fields.insert("config_version".into(), Value::from(config_version));
    fields.insert("protection".into(), Value::from(protection));
    fields.insert("purpose".into(), Value::from("root"));
    Value::Object(fields).to_string().into_bytes()
}

/// The root encrypted with a key derived from the admin's passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseProtection {
    pub version: u64,
    pub kdf: String,
    pub kdf_opslimit: u64,
    pub kdf_memlimit: u64,
    pub salt: Vec<u8>,
    pub aead: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl PassphraseProtection {
    pub fn aad(&self) -> Vec<u8> {
        associated_data("passphrase", self.version)
    }
}

/// The root encrypted with the recovery key that is split into Shamir shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryProtection {
    pub version: u64,
    pub share_set_id: Vec<u8>,
    pub threshold: u32,
    pub share_count: u32,
    pub aead: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl RecoveryProtection {
    pub fn aad(&self) -> Vec<u8> {
        associated_data("recovery", self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealConfig {
    pub config_version: u64,
    pub initialized_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub passphrase: PassphraseProtection,
    pub recovery: RecoveryProtection,
}
