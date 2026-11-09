//! In-memory doubles of the ports for the tests (feature `test-support`), as in `fakes.py`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, PoisonError};

use hecate_domain::sealing::access_log::AccessLogEntry;
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::SealConfig;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

use crate::errors::ServiceError;
use crate::ports::{
    AccessLog, Clock, ConcurrencyConflict, EventRecord, EventStore, KeyMaterial, MaterialStore,
    SealRepository, SealWork, SealWrites, SecretMaterial, SecretStore, SecretWork, SecretWrites,
};
use crate::sealing::settings::KdfLimits;

/// The cheapest Argon2id libsodium accepts: only for tests.
pub const MINIMAL_KDF: KdfLimits = KdfLimits {
    opslimit: 1,
    memlimit: 8192,
};
pub const START: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);
pub const GOOD_PASSPHRASE: &str = "correct horse battery";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct FakeClock {
    now: Mutex<OffsetDateTime>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self::at(START)
    }

    pub fn at(now: OffsetDateTime) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    pub fn advance(&self, delta: Duration) {
        *lock(&self.now) += delta;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> OffsetDateTime {
        *lock(&self.now)
    }
}

#[derive(Clone, Default)]
struct SealData {
    config: Option<SealConfig>,
    attempts: AttemptPolicy,
}

impl SealWrites for SealData {
    fn load_config(&mut self) -> Result<Option<SealConfig>, ServiceError> {
        Ok(self.config.clone())
    }

    fn save_config(&mut self, config: &SealConfig) -> Result<(), ServiceError> {
        self.config = Some(config.clone());
        Ok(())
    }

    fn load_attempts(&mut self) -> Result<AttemptPolicy, ServiceError> {
        Ok(self.attempts.clone())
    }

    fn save_attempts(&mut self, attempts: &AttemptPolicy) -> Result<(), ServiceError> {
        self.attempts = attempts.clone();
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemorySealRepository {
    data: Mutex<SealData>,
}

impl InMemorySealRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn config(&self) -> Option<SealConfig> {
        lock(&self.data).config.clone()
    }

    pub fn attempts(&self) -> AttemptPolicy {
        lock(&self.data).attempts.clone()
    }
}

impl SealRepository for InMemorySealRepository {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError> {
        Ok(self.config())
    }

    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError> {
        Ok(self.attempts())
    }

    fn transaction(&self, work: &mut SealWork<'_>) -> Result<(), ServiceError> {
        let mut data = lock(&self.data);
        let mut draft = data.clone();
        work(&mut draft)?;
        *data = draft;
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryAccessLog {
    items: Mutex<Vec<AccessLogEntry>>,
}

impl InMemoryAccessLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> Vec<AccessLogEntry> {
        lock(&self.items).clone()
    }

    pub fn clear(&self) {
        lock(&self.items).clear();
    }
}

impl AccessLog for InMemoryAccessLog {
    fn append(&self, entry: &AccessLogEntry) -> Result<(), ServiceError> {
        lock(&self.items).push(entry.clone());
        Ok(())
    }
}

#[derive(Clone, Default)]
struct SecretData {
    events: BTreeMap<String, Vec<EventRecord>>,
    secret_material: HashMap<(String, u64), SecretMaterial>,
    key_material: HashMap<(String, u64), KeyMaterial>,
}

impl SecretWrites for SecretData {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        Ok(self.events.get(stream).cloned().unwrap_or_default())
    }

    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError> {
        let current = self.events.get(stream).map_or(0, Vec::len) as u64;
        if current != expected_version {
            return Err(ConcurrencyConflict {
                current_version: current,
            }
            .into());
        }
        self.events
            .entry(stream.to_string())
            .or_default()
            .extend_from_slice(events);
        Ok(current + events.len() as u64)
    }

    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError> {
        self.secret_material
            .insert((path.to_string(), version), material.clone());
        Ok(())
    }

    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        Ok(self
            .secret_material
            .get(&(path.to_string(), version))
            .cloned())
    }

    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError> {
        self.secret_material.remove(&(path.to_string(), version));
        Ok(())
    }

    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError> {
        self.key_material
            .insert((identity.to_string(), key_version), material.clone());
        Ok(())
    }

    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        Ok(self
            .key_material
            .get(&(identity.to_string(), key_version))
            .cloned())
    }
}

#[derive(Default)]
pub struct InMemorySecretStore {
    data: Mutex<SecretData>,
    failing: Mutex<Option<&'static str>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn secret_material_count(&self) -> usize {
        lock(&self.data).secret_material.len()
    }

    pub fn key_material_count(&self) -> usize {
        lock(&self.data).key_material.len()
    }

    pub fn has_key_material(&self, identity: &str, key_version: u64) -> bool {
        lock(&self.data)
            .key_material
            .contains_key(&(identity.to_string(), key_version))
    }

    /// Nothing at all: no events and no material.
    pub fn is_empty(&self) -> bool {
        let data = lock(&self.data);
        data.events.is_empty() && data.secret_material.is_empty() && data.key_material.is_empty()
    }

    /// Everything the store keeps, as bytes: the events as text and the raw material, to look
    /// for what must never be there.
    pub fn contents(&self) -> Vec<u8> {
        let data = lock(&self.data);
        let mut contents = format!("{:?}", data.events).into_bytes();
        for material in data.secret_material.values() {
            for part in [
                &material.value_nonce,
                &material.value_ciphertext,
                &material.dek_nonce,
                &material.dek_ciphertext,
            ] {
                contents.extend_from_slice(part);
            }
        }
        for material in data.key_material.values() {
            contents.extend_from_slice(&material.nonce);
            contents.extend_from_slice(&material.ciphertext);
        }
        contents
    }

    /// Makes one write of the transactions fail (`save_secret_material` or
    /// `delete_secret_material`), as a full disk would; `None` repairs it.
    pub fn fail_on(&self, write: Option<&'static str>) {
        *lock(&self.failing) = write;
    }
}

/// The draft of one transaction, with the injected failure.
struct Draft<'a> {
    data: &'a mut SecretData,
    failing: Option<&'static str>,
}

impl Draft<'_> {
    fn check(&self, write: &'static str) -> Result<(), ServiceError> {
        if self.failing == Some(write) {
            return Err(ServiceError::internal("DiskFull"));
        }
        Ok(())
    }
}

impl SecretWrites for Draft<'_> {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        self.data.load(stream)
    }

    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError> {
        self.data.append(stream, events, expected_version)
    }

    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError> {
        self.check("save_secret_material")?;
        self.data.save_secret_material(path, version, material)
    }

    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        self.data.load_secret_material(path, version)
    }

    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError> {
        self.check("delete_secret_material")?;
        self.data.delete_secret_material(path, version)
    }

    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError> {
        self.data.save_key_material(identity, key_version, material)
    }

    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        self.data.load_key_material(identity, key_version)
    }
}

impl EventStore for InMemorySecretStore {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        Ok(lock(&self.data)
            .events
            .get(stream)
            .cloned()
            .unwrap_or_default())
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        Ok(lock(&self.data)
            .events
            .keys()
            .filter(|stream| stream.starts_with(prefix))
            .cloned()
            .collect())
    }
}

impl MaterialStore for InMemorySecretStore {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        Ok(lock(&self.data)
            .secret_material
            .get(&(path.to_string(), version))
            .cloned())
    }

    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        Ok(lock(&self.data)
            .key_material
            .get(&(identity.to_string(), key_version))
            .cloned())
    }
}

impl SecretStore for InMemorySecretStore {
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError> {
        let failing = *lock(&self.failing);
        let mut data = lock(&self.data);
        let mut draft = data.clone();
        work(&mut Draft {
            data: &mut draft,
            failing,
        })?;
        *data = draft;
        Ok(())
    }
}

use std::sync::Arc;

use crate::ports::CryptoProvider;
use crate::sealing::service::SealingService;
use crate::sealing::settings::SealingSettings;

/// A sealing service with the cheapest key derivation, as `make_service` in `fakes.py`.
pub fn make_service(
    crypto: Arc<dyn CryptoProvider>,
    repo: Arc<dyn SealRepository>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
) -> SealingService {
    SealingService::new(
        crypto,
        repo,
        access_log,
        clock,
        SealingSettings::new(MINIMAL_KDF),
    )
}

/// Runs `init_begin` and `init_confirm` and returns the three share texts.
pub fn initialize(service: &SealingService, passphrase: &str) -> Vec<String> {
    let begun = service.init_begin(passphrase).expect("init_begin");
    let shares: Vec<String> = begun.shares.iter().map(|share| share.to_string()).collect();
    service
        .init_confirm(&begun.init_id, &shares[..2])
        .expect("init_confirm");
    shares
}

/// The in-memory doubles and a sealing service on them.
pub struct SealingRig {
    pub crypto: Arc<dyn CryptoProvider>,
    pub repo: Arc<InMemorySealRepository>,
    pub log: Arc<InMemoryAccessLog>,
    pub clock: Arc<FakeClock>,
    pub service: Arc<SealingService>,
}

impl SealingRig {
    pub fn new(crypto: Arc<dyn CryptoProvider>) -> Self {
        let repo = Arc::new(InMemorySealRepository::new());
        let log = Arc::new(InMemoryAccessLog::new());
        let clock = Arc::new(FakeClock::new());
        let service = Arc::new(make_service(
            crypto.clone(),
            repo.clone(),
            log.clone(),
            clock.clone(),
        ));
        Self {
            crypto,
            repo,
            log,
            clock,
            service,
        }
    }

    /// A new process on the same storage, clock and access log.
    pub fn restart(&self) -> SealingService {
        make_service(
            self.crypto.clone(),
            self.repo.clone(),
            self.log.clone(),
            self.clock.clone(),
        )
    }
}

use crate::secrets::service::SecretsService;
use crate::secrets::settings::SecretsSettings;

/// An initialized Hécate with a secrets service over the in-memory doubles, as in `fakes.py`.
pub struct SecretsRig {
    pub clock: Arc<FakeClock>,
    pub access_log: Arc<InMemoryAccessLog>,
    pub sealing: Arc<SealingService>,
    pub shares: Vec<String>,
    pub store: Arc<InMemorySecretStore>,
    pub service: Arc<SecretsService>,
}

impl SecretsRig {
    pub fn new(crypto: Arc<dyn CryptoProvider>, unsealed: bool, settings: SecretsSettings) -> Self {
        let clock = Arc::new(FakeClock::new());
        let access_log = Arc::new(InMemoryAccessLog::new());
        let sealing = Arc::new(make_service(
            crypto.clone(),
            Arc::new(InMemorySealRepository::new()),
            access_log.clone(),
            clock.clone(),
        ));
        let shares = initialize(&sealing, GOOD_PASSPHRASE);
        if unsealed {
            sealing
                .unseal_with_passphrase(GOOD_PASSPHRASE)
                .expect("unseal");
        }
        let store = Arc::new(InMemorySecretStore::new());
        let service = Arc::new(SecretsService::new(
            sealing.clone(),
            store.clone(),
            crypto,
            access_log.clone(),
            clock.clone(),
            settings,
        ));
        Self {
            clock,
            access_log,
            sealing,
            shares,
            store,
            service,
        }
    }

    /// The rows of the access log about secrets.
    pub fn secret_rows(&self) -> Vec<AccessLogEntry> {
        self.access_log
            .entries()
            .into_iter()
            .filter(|entry| entry.operation.as_str().starts_with("secret_"))
            .collect()
    }
}
