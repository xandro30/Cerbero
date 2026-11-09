//! The ports of the application layer (research R13): synchronous traits used as `Arc<dyn …>`,
//! with transactions as closures that receive a write handle.

use hecate_domain::sealing::access_log::AccessLogEntry;
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::SealConfig;
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use time::OffsetDateTime;

use crate::errors::ServiceError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CryptoError(pub String);

pub trait CryptoProvider: Send + Sync {
    /// Random bytes that are not secret (salts, nonces, identifiers).
    fn random_bytes(&self, n: usize) -> Vec<u8>;
    /// A random 32-byte secret (root, recovery, scope or version key).
    fn random_key(&self) -> SecretKey;
    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        opslimit: u64,
        memlimit: usize,
    ) -> Result<SecretKey, CryptoError>;
    /// AES-256-GCM with a fresh 12-byte nonce; returns the nonce and the ciphertext with its tag.
    fn aead_encrypt(
        &self,
        key: &SecretKey,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CryptoError>;
    fn aead_decrypt(
        &self,
        key: &SecretKey,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, CryptoError>;
    fn hmac_sha256(&self, key: &SecretKey, data: &[u8]) -> [u8; 32];
    /// HKDF-SHA256 without salt, 32 bytes of output.
    fn hkdf_sha256(&self, key: &SecretKey, info: &[u8]) -> SecretKey;
    /// Splits a 32-byte secret into three shares (index 1 to 3); any two recover it.
    fn shamir_split(&self, secret: &SecretKey) -> Result<Vec<(u8, SecretBytes)>, CryptoError>;
    fn shamir_combine(&self, shares: &[(u8, &[u8])]) -> Result<SecretKey, CryptoError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// The writes of one sealing transaction: they all commit together or not at all.
pub trait SealWrites {
    fn load_config(&mut self) -> Result<Option<SealConfig>, ServiceError>;
    fn save_config(&mut self, config: &SealConfig) -> Result<(), ServiceError>;
    fn load_attempts(&mut self) -> Result<AttemptPolicy, ServiceError>;
    fn save_attempts(&mut self, attempts: &AttemptPolicy) -> Result<(), ServiceError>;
}

pub type SealWork<'a> = dyn FnMut(&mut dyn SealWrites) -> Result<(), ServiceError> + 'a;

pub trait SealRepository: Send + Sync {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError>;
    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError>;
    fn transaction(&self, work: &mut SealWork<'_>) -> Result<(), ServiceError>;
}

pub trait AccessLog: Send + Sync {
    fn append(&self, entry: &AccessLogEntry) -> Result<(), ServiceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the stream is at version {current_version}")]
pub struct ConcurrencyConflict {
    pub current_version: u64,
}

/// A stored event: its type, its JSON data (never a value or a key) and its instant.
pub use hecate_domain::events::EventRecord;

/// A scope key wrapped by the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// The encrypted value of one version and its version key wrapped by the scope key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMaterial {
    pub key_version: u64,
    pub value_nonce: Vec<u8>,
    pub value_ciphertext: Vec<u8>,
    pub dek_nonce: Vec<u8>,
    pub dek_ciphertext: Vec<u8>,
}

pub trait EventStore: Send + Sync {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError>;
    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError>;
}

pub trait MaterialStore: Send + Sync {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError>;
    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError>;
}

/// The writes of one secrets transaction: events and material commit together or not at all.
pub trait SecretWrites {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError>;
    /// Appends events if the stream is still at `expected_version`; otherwise
    /// [`ServiceError::Concurrency`]. Returns the new stream version.
    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError>;
    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError>;
    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError>;
    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError>;
    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError>;
    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError>;
}

pub type SecretWork<'a> = dyn FnMut(&mut dyn SecretWrites) -> Result<(), ServiceError> + 'a;

pub trait SecretStore: EventStore + MaterialStore {
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError>;
}
