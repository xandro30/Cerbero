//! The seal lifecycle of Hécate (feature 001): the root lives in memory only while unsealed.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use data_encoding::BASE64URL_NOPAD;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::passphrase::Passphrase;
use hecate_domain::sealing::protection::{
    AEAD_NAME, KDF_NAME, PassphraseProtection, RecoveryProtection, SHARE_COUNT, SHARE_THRESHOLD,
    SealConfig, associated_data,
};
use hecate_domain::sealing::share::RecoveryShare;
use hecate_domain::sealing::state::{SealState, UnsealMethod};
use hecate_domain::sensitive::SecretKey;
use serde_json::{Map, Value};
use time::OffsetDateTime;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::errors::ServiceError;
use crate::ports::{AccessLog, Clock, CryptoProvider, SealRepository};
use crate::sealing::root_holder::RootHolder;
use crate::sealing::settings::SealingSettings;

const SALT_SIZE: usize = 16;
const SET_ID_SIZE: usize = 4;
const TOKEN_SIZE: usize = 16;

pub const INIT_WARNING: &str = "If you lose both the passphrase and two of the three recovery shares, \
                                the data in Hécate cannot be recovered.";

fn invalid(message: impl Into<String>) -> ServiceError {
    HecateError::invalid(message).into()
}

fn result_for(error: &HecateError) -> AccessResult {
    match error {
        HecateError::InvalidInput(_) => AccessResult::InvalidInput,
        HecateError::StateConflict { .. } => AccessResult::RejectedConflict,
        HecateError::RateLimited { .. } => AccessResult::RejectedRateLimited,
        HecateError::AuthenticationFailed(_) | HecateError::NotFound(_) => AccessResult::Failure,
    }
}

/// Compares two identifiers without leaking where they differ.
fn same_token(expected: &str, given: &str) -> bool {
    let (expected, given) = (expected.as_bytes(), given.as_bytes());
    expected.len() == given.len()
        && expected
            .iter()
            .zip(given)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

struct PendingInit {
    init_id: String,
    _root: RootHolder,
    config: SealConfig,
    shares: BTreeMap<u8, RecoveryShare>,
    expires_at: OffsetDateTime,
}

struct PendingRekey {
    rekey_id: String,
    recovery_key: RootHolder,
    share_set_id: [u8; SET_ID_SIZE],
    shares: BTreeMap<u8, RecoveryShare>,
    expires_at: OffsetDateTime,
}

#[derive(Default)]
struct Session {
    root: Option<RootHolder>,
    unsealed_via: Option<UnsealMethod>,
    recommend_passphrase_change: bool,
    pending_init: Option<PendingInit>,
    pending_rekey: Option<PendingRekey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: SealState,
    pub initialized_at: Option<OffsetDateTime>,
    pub unsealed_via: Option<UnsealMethod>,
    pub recommend_passphrase_change: bool,
    pub consecutive_failures: u32,
    pub retry_after_s: u64,
}

/// A pending initialization: its shares erase themselves when dropped, and the whole value can be
/// neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::sealing::service::InitBegun>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::sealing::service::InitBegun>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::sealing::service::InitBegun>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct InitBegun {
    pub init_id: String,
    pub shares: Vec<Zeroizing<String>>,
    #[zeroize(skip)]
    pub expires_at: OffsetDateTime,
    #[zeroize(skip)]
    pub warning: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsealed {
    pub state: SealState,
    pub unsealed_via: UnsealMethod,
    pub recommend_passphrase_change: bool,
}

/// A pending set of new shares: they erase themselves when dropped, and the whole value can be
/// neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::sealing::service::RekeyBegun>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::sealing::service::RekeyBegun>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::sealing::service::RekeyBegun>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RekeyBegun {
    pub rekey_id: String,
    pub shares: Vec<Zeroizing<String>>,
    #[zeroize(skip)]
    pub expires_at: OffsetDateTime,
}

pub struct SealingService {
    crypto: Arc<dyn CryptoProvider>,
    repo: Arc<dyn SealRepository>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
    settings: SealingSettings,
    session: Mutex<Session>,
}

impl SealingService {
    pub fn new(
        crypto: Arc<dyn CryptoProvider>,
        repo: Arc<dyn SealRepository>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
        settings: SealingSettings,
    ) -> Self {
        Self {
            crypto,
            repo,
            access_log,
            clock,
            settings,
            session: Mutex::new(Session::default()),
        }
    }

    fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    fn state(&self, session: &Session) -> Result<SealState, ServiceError> {
        if session.root.is_some() {
            return Ok(SealState::Unsealed);
        }
        Ok(match self.repo.load_config()? {
            None => SealState::Uninitialized,
            Some(_) => SealState::Sealed,
        })
    }

    fn require_state(&self, session: &Session, expected: SealState) -> Result<(), ServiceError> {
        let state = self.state(session)?;
        if state != expected {
            return Err(HecateError::state_conflict(Some(state), None, None).into());
        }
        Ok(())
    }

    fn config(&self) -> Result<SealConfig, ServiceError> {
        self.repo
            .load_config()?
            .ok_or_else(|| ServiceError::internal("MissingConfig"))
    }

    fn log(
        &self,
        operation: Operation,
        method: Method,
        result: AccessResult,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.access_log.append(&AccessLogEntry {
            at: self.now(),
            operation,
            method,
            result,
            detail: detail.to_string(),
            subject: None,
            path: None,
            version: None,
        })
    }

    /// Runs `work` and leaves a row when Hécate refuses it; internal failures leave no row.
    fn logged<T>(
        &self,
        operation: Operation,
        method: Method,
        detail: &str,
        work: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let outcome = work();
        if let Err(ServiceError::Hecate(error)) = &outcome {
            self.log(operation, method, result_for(error), detail)?;
        }
        outcome
    }

    fn token(&self) -> String {
        BASE64URL_NOPAD.encode(&self.crypto.random_bytes(TOKEN_SIZE))
    }

    fn share_set_id(&self) -> [u8; SET_ID_SIZE] {
        let mut id = [0u8; SET_ID_SIZE];
        id.copy_from_slice(&self.crypto.random_bytes(SET_ID_SIZE));
        id
    }

    fn parse_shares<S: AsRef<str>>(texts: &[S]) -> Result<Vec<RecoveryShare>, ServiceError> {
        let shares = texts
            .iter()
            .map(|text| RecoveryShare::parse(text.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut indices: Vec<u8> = shares.iter().map(RecoveryShare::index).collect();
        indices.sort_unstable();
        indices.dedup();
        if indices.len() != shares.len() {
            return Err(invalid("the same share was given twice"));
        }
        if shares.len() < SHARE_THRESHOLD as usize {
            return Err(invalid(format!("{SHARE_THRESHOLD} shares are needed")));
        }
        Ok(shares)
    }

    fn parse_share_value(value: &Value) -> Result<Vec<RecoveryShare>, ServiceError> {
        let not_texts = || invalid("shares must be a list of texts");
        let items = value.as_array().ok_or_else(not_texts)?;
        let texts = items
            .iter()
            .map(|item| item.as_str().ok_or_else(not_texts))
            .collect::<Result<Vec<_>, _>>()?;
        Self::parse_shares(&texts)
    }

    fn passphrase_protection(
        &self,
        root: &SecretKey,
        passphrase: &str,
        version: u64,
    ) -> Result<PassphraseProtection, ServiceError> {
        let salt = self.crypto.random_bytes(SALT_SIZE);
        let limits = self.settings.kdf;
        let key = self
            .crypto
            .derive_key(passphrase, &salt, limits.opslimit, limits.memlimit)?;
        let (nonce, ciphertext) = self.crypto.aead_encrypt(
            &key,
            root.expose(),
            &associated_data("passphrase", version),
        )?;
        Ok(PassphraseProtection {
            version,
            kdf: KDF_NAME.to_string(),
            kdf_opslimit: limits.opslimit,
            kdf_memlimit: limits.memlimit as u64,
            salt,
            aead: AEAD_NAME.to_string(),
            nonce,
            ciphertext,
        })
    }

    fn recovery_protection(
        &self,
        root: &SecretKey,
        recovery_key: &SecretKey,
        share_set_id: &[u8; SET_ID_SIZE],
        version: u64,
    ) -> Result<RecoveryProtection, ServiceError> {
        let (nonce, ciphertext) = self.crypto.aead_encrypt(
            recovery_key,
            root.expose(),
            &associated_data("recovery", version),
        )?;
        Ok(RecoveryProtection {
            version,
            share_set_id: share_set_id.to_vec(),
            threshold: SHARE_THRESHOLD,
            share_count: SHARE_COUNT,
            aead: AEAD_NAME.to_string(),
            nonce,
            ciphertext,
        })
    }

    fn new_shares(
        &self,
        recovery_key: &SecretKey,
        share_set_id: [u8; SET_ID_SIZE],
    ) -> Result<BTreeMap<u8, RecoveryShare>, ServiceError> {
        self.crypto
            .shamir_split(recovery_key)?
            .into_iter()
            .map(|(index, values)| {
                let share = RecoveryShare::new(index, share_set_id, values.expose().to_vec())?;
                Ok((index, share))
            })
            .collect()
    }

    pub fn status(&self) -> Result<Status, ServiceError> {
        let session = self.session();
        let config = self.repo.load_config()?;
        let attempts = self.repo.load_attempts()?;
        Ok(Status {
            state: self.state(&session)?,
            initialized_at: config.map(|config| config.initialized_at),
            unsealed_via: session.unsealed_via,
            recommend_passphrase_change: session.recommend_passphrase_change,
            consecutive_failures: attempts.consecutive_failures,
            retry_after_s: attempts.retry_after(self.now()),
        })
    }

    /// Releases the root when the process stops.
    pub fn shutdown(&self) {
        *self.session() = Session::default();
    }

    /// Leaves a row for a recognized operation whose arguments were malformed.
    pub fn record_invalid_request(
        &self,
        operation: Operation,
        method: Method,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.log(operation, method, AccessResult::InvalidInput, detail)
    }

    pub fn init_begin(&self, passphrase: &str) -> Result<InitBegun, ServiceError> {
        let mut session = self.session();
        let begun = self.logged(Operation::Init, Method::Local, "begin", || {
            self.require_state(&session, SealState::Uninitialized)?;
            let secret = Passphrase::new(passphrase)?;
            session.pending_init = None;
            let root = self.crypto.random_key();
            let recovery_key = self.crypto.random_key();
            let share_set_id = self.share_set_id();
            let now = self.now();
            let config = SealConfig {
                config_version: 1,
                initialized_at: now,
                updated_at: now,
                passphrase: self.passphrase_protection(&root, secret.expose(), 1)?,
                recovery: self.recovery_protection(&root, &recovery_key, &share_set_id, 1)?,
            };
            let shares = self.new_shares(&recovery_key, share_set_id)?;
            let begun = InitBegun {
                init_id: self.token(),
                shares: shares.values().map(RecoveryShare::encode).collect(),
                expires_at: now + self.settings.pending_ttl,
                warning: INIT_WARNING,
            };
            session.pending_init = Some(PendingInit {
                init_id: begun.init_id.clone(),
                _root: RootHolder::new(root),
                config,
                shares,
                expires_at: begun.expires_at,
            });
            Ok(begun)
        })?;
        self.log(
            Operation::Init,
            Method::Local,
            AccessResult::Success,
            "begin",
        )?;
        Ok(begun)
    }

    pub fn init_confirm<S: AsRef<str>>(
        &self,
        init_id: &str,
        shares: &[S],
    ) -> Result<SealState, ServiceError> {
        let mut session = self.session();
        self.logged(Operation::Init, Method::Local, "confirm", || {
            self.require_state(&session, SealState::Uninitialized)?;
            let pending = session
                .pending_init
                .as_ref()
                .filter(|pending| same_token(&pending.init_id, init_id))
                .ok_or_else(|| invalid("unknown initialization: start it again"))?;
            if self.now() > pending.expires_at {
                session.pending_init = None;
                return Err(invalid("the initialization expired: start it again"));
            }
            let given = Self::parse_shares(shares)?;
            if given
                .iter()
                .any(|share| pending.shares.get(&share.index()) != Some(share))
            {
                return Err(invalid("the shares do not match the ones that were shown"));
            }
            let config = pending.config.clone();
            self.repo
                .transaction(&mut |writes| writes.save_config(&config))?;
            session.pending_init = None;
            Ok(())
        })?;
        self.log(
            Operation::Init,
            Method::Local,
            AccessResult::Success,
            "confirm",
        )?;
        Ok(SealState::Sealed)
    }

    pub fn init_abort(&self, init_id: &str) -> Result<(), ServiceError> {
        let mut session = self.session();
        self.logged(Operation::Init, Method::Local, "abort", || {
            self.require_state(&session, SealState::Uninitialized)?;
            if session
                .pending_init
                .as_ref()
                .is_some_and(|pending| same_token(&pending.init_id, init_id))
            {
                session.pending_init = None;
            }
            Ok(())
        })?;
        self.log(
            Operation::Init,
            Method::Local,
            AccessResult::Success,
            "abort",
        )
    }

    fn check_wait(&self) -> Result<(), ServiceError> {
        let wait = self.repo.load_attempts()?.retry_after(self.now());
        if wait > 0 {
            return Err(HecateError::RateLimited {
                retry_after_s: wait,
            }
            .into());
        }
        Ok(())
    }

    fn fail_attempt(&self, message: &str) -> ServiceError {
        let attempts = match self.repo.load_attempts() {
            Ok(attempts) => attempts.register_failure(self.now()),
            Err(error) => return error,
        };
        if let Err(error) = self
            .repo
            .transaction(&mut |writes| writes.save_attempts(&attempts))
        {
            return error;
        }
        HecateError::auth(message).into()
    }

    fn reset_attempts(&self) -> Result<(), ServiceError> {
        if self.repo.load_attempts()? != AttemptPolicy::default() {
            self.repo
                .transaction(&mut |writes| writes.save_attempts(&AttemptPolicy::default()))?;
        }
        Ok(())
    }

    fn open(&self, session: &mut Session, root: SecretKey, via: UnsealMethod) -> Unsealed {
        session.root = Some(RootHolder::new(root));
        session.unsealed_via = Some(via);
        session.recommend_passphrase_change = via == UnsealMethod::Recovery;
        Unsealed {
            state: SealState::Unsealed,
            unsealed_via: via,
            recommend_passphrase_change: session.recommend_passphrase_change,
        }
    }

    fn root_from_shares(
        &self,
        config: &SealConfig,
        shares: &[RecoveryShare],
    ) -> Result<SecretKey, ServiceError> {
        let recovery = &config.recovery;
        if shares
            .iter()
            .any(|share| share.share_set_id().as_slice() != recovery.share_set_id)
        {
            return Err(self.fail_attempt("a share belongs to another set (an old or foreign one)"));
        }
        let pairs: Vec<(u8, &[u8])> = shares
            .iter()
            .map(|share| (share.index(), share.values()))
            .collect();
        let root = self.crypto.shamir_combine(&pairs).and_then(|recovery_key| {
            self.crypto.aead_decrypt(
                &recovery_key,
                &recovery.nonce,
                &recovery.ciphertext,
                &recovery.aad(),
            )
        });
        match root {
            Ok(root) => SecretKey::from_slice(root.expose())
                .ok_or_else(|| ServiceError::internal("InvalidRoot")),
            Err(_) => Err(self.fail_attempt("the shares do not recover the root")),
        }
    }

    fn root_from_passphrase(
        &self,
        config: &SealConfig,
        passphrase: &str,
    ) -> Result<SecretKey, ServiceError> {
        let protection = &config.passphrase;
        let key = self.crypto.derive_key(
            passphrase,
            &protection.salt,
            protection.kdf_opslimit,
            usize::try_from(protection.kdf_memlimit)
                .map_err(|_| ServiceError::internal("InvalidLimits"))?,
        )?;
        match self.crypto.aead_decrypt(
            &key,
            &protection.nonce,
            &protection.ciphertext,
            &protection.aad(),
        ) {
            Ok(root) => SecretKey::from_slice(root.expose())
                .ok_or_else(|| ServiceError::internal("InvalidRoot")),
            Err(_) => Err(self.fail_attempt("incorrect passphrase")),
        }
    }

    pub fn unseal_with_passphrase(&self, passphrase: &str) -> Result<Unsealed, ServiceError> {
        let mut session = self.session();
        let root = self.logged(Operation::Unseal, Method::Passphrase, "passphrase", || {
            self.require_state(&session, SealState::Sealed)?;
            if passphrase.is_empty() {
                return Err(invalid("the passphrase must be a non-empty text"));
            }
            self.check_wait()?;
            let root = self.root_from_passphrase(&self.config()?, passphrase)?;
            self.reset_attempts()?;
            Ok(root)
        })?;
        self.log(
            Operation::Unseal,
            Method::Passphrase,
            AccessResult::Success,
            "passphrase",
        )?;
        Ok(self.open(&mut session, root, UnsealMethod::Passphrase))
    }

    pub fn unseal_with_shares<S: AsRef<str>>(
        &self,
        shares: &[S],
    ) -> Result<Unsealed, ServiceError> {
        let mut session = self.session();
        let root = self.logged(Operation::Unseal, Method::Recovery, "shares", || {
            self.require_state(&session, SealState::Sealed)?;
            let shares = Self::parse_shares(shares)?;
            self.check_wait()?;
            let root = self.root_from_shares(&self.config()?, &shares)?;
            self.reset_attempts()?;
            Ok(root)
        })?;
        self.log(
            Operation::Unseal,
            Method::Recovery,
            AccessResult::Success,
            "shares",
        )?;
        Ok(self.open(&mut session, root, UnsealMethod::Recovery))
    }

    pub fn seal(&self) -> Result<SealState, ServiceError> {
        let mut session = self.session();
        self.logged(Operation::Seal, Method::Local, "seal", || {
            self.require_state(&session, SealState::Unsealed)?;
            session.root = None;
            session.unsealed_via = None;
            session.recommend_passphrase_change = false;
            session.pending_rekey = None;
            Ok(())
        })?;
        self.log(
            Operation::Seal,
            Method::Local,
            AccessResult::Success,
            "seal",
        )?;
        Ok(SealState::Sealed)
    }

    /// A copy of the root, only while unsealed; otherwise `conflict` with the seal state.
    pub fn root(&self) -> Result<SecretKey, ServiceError> {
        let session = self.session();
        match &session.root {
            Some(root) => Ok(root.copy()),
            None => {
                Err(HecateError::state_conflict(Some(self.state(&session)?), None, None).into())
            }
        }
    }

    /// Checks a proof (the current passphrase or two shares) with the shared attempt counter.
    pub fn verify_proof(&self, proof: &Value) -> Result<(), ServiceError> {
        let session = self.session();
        self.require_state(&session, SealState::Unsealed)?;
        self.root_from_proof(&self.config()?, proof)?;
        self.reset_attempts()
    }

    pub fn proof_method(proof: &Value) -> Method {
        proof.as_object().map_or(Method::Local, Self::fields_method)
    }

    /// The method that a proof's fields name: the shares alone, the passphrase alone, or neither.
    pub fn fields_method(fields: &Map<String, Value>) -> Method {
        match (
            fields.contains_key("passphrase"),
            fields.contains_key("shares"),
        ) {
            (false, true) => Method::Recovery,
            (true, false) => Method::Passphrase,
            _ => Method::Local,
        }
    }

    fn root_from_proof(
        &self,
        config: &SealConfig,
        proof: &Value,
    ) -> Result<SecretKey, ServiceError> {
        let either = || invalid("the proof must be either 'passphrase' or 'shares'");
        let proof = proof.as_object().ok_or_else(either)?;
        if proof.contains_key("passphrase") == proof.contains_key("shares") {
            return Err(either());
        }
        if let Some(passphrase) = proof.get("passphrase") {
            let passphrase = passphrase
                .as_str()
                .filter(|passphrase| !passphrase.is_empty())
                .ok_or_else(|| invalid("the passphrase must be a non-empty text"))?;
            self.check_wait()?;
            return self.root_from_passphrase(config, passphrase);
        }
        let shares = Self::parse_share_value(&proof["shares"])?;
        self.check_wait()?;
        self.root_from_shares(config, &shares)
    }

    pub fn change_passphrase(
        &self,
        proof: &Value,
        new_passphrase: &str,
    ) -> Result<u64, ServiceError> {
        let method = Self::proof_method(proof);
        let mut session = self.session();
        let version = self.logged(Operation::PassphraseChange, method, "change", || {
            self.require_state(&session, SealState::Unsealed)?;
            let new_secret = Passphrase::new(new_passphrase)?;
            let config = self.config()?;
            let root = self.root_from_proof(&config, proof)?;
            let version = config.config_version + 1;
            let updated = SealConfig {
                config_version: version,
                initialized_at: config.initialized_at,
                updated_at: self.now(),
                passphrase: self.passphrase_protection(&root, new_secret.expose(), version)?,
                recovery: config.recovery.clone(),
            };
            self.repo.transaction(&mut |writes| {
                writes.save_config(&updated)?;
                writes.save_attempts(&AttemptPolicy::default())
            })?;
            session.recommend_passphrase_change = false;
            Ok(version)
        })?;
        self.log(
            Operation::PassphraseChange,
            method,
            AccessResult::Success,
            "change",
        )?;
        Ok(version)
    }

    pub fn rekey_begin(&self, proof: &Value) -> Result<RekeyBegun, ServiceError> {
        let method = Self::proof_method(proof);
        let mut session = self.session();
        let begun = self.logged(Operation::SharesRekey, method, "begin", || {
            self.require_state(&session, SealState::Unsealed)?;
            self.root_from_proof(&self.config()?, proof)?;
            self.reset_attempts()?;
            session.pending_rekey = None;
            let recovery_key = self.crypto.random_key();
            let share_set_id = self.share_set_id();
            let shares = self.new_shares(&recovery_key, share_set_id)?;
            let begun = RekeyBegun {
                rekey_id: self.token(),
                shares: shares.values().map(RecoveryShare::encode).collect(),
                expires_at: self.now() + self.settings.pending_ttl,
            };
            session.pending_rekey = Some(PendingRekey {
                rekey_id: begun.rekey_id.clone(),
                recovery_key: RootHolder::new(recovery_key),
                share_set_id,
                shares,
                expires_at: begun.expires_at,
            });
            Ok(begun)
        })?;
        self.log(
            Operation::SharesRekey,
            method,
            AccessResult::Success,
            "begin",
        )?;
        Ok(begun)
    }

    pub fn rekey_confirm<S: AsRef<str>>(
        &self,
        rekey_id: &str,
        shares: &[S],
    ) -> Result<u64, ServiceError> {
        let mut session = self.session();
        let version = self.logged(Operation::SharesRekey, Method::Local, "confirm", || {
            self.require_state(&session, SealState::Unsealed)?;
            let pending = session
                .pending_rekey
                .as_ref()
                .filter(|pending| same_token(&pending.rekey_id, rekey_id))
                .ok_or_else(|| invalid("unknown share set: start again"))?;
            if self.now() > pending.expires_at {
                session.pending_rekey = None;
                return Err(invalid("the share set expired: start again"));
            }
            let given = Self::parse_shares(shares)?;
            if given
                .iter()
                .any(|share| pending.shares.get(&share.index()) != Some(share))
            {
                return Err(invalid("the shares do not match the ones that were shown"));
            }
            let root = session
                .root
                .as_ref()
                .ok_or_else(|| ServiceError::internal("MissingRoot"))?;
            let config = self.config()?;
            let version = config.config_version + 1;
            let updated = SealConfig {
                config_version: version,
                initialized_at: config.initialized_at,
                updated_at: self.now(),
                passphrase: config.passphrase.clone(),
                recovery: self.recovery_protection(
                    root.key(),
                    pending.recovery_key.key(),
                    &pending.share_set_id,
                    version,
                )?,
            };
            self.repo
                .transaction(&mut |writes| writes.save_config(&updated))?;
            session.pending_rekey = None;
            Ok(version)
        })?;
        self.log(
            Operation::SharesRekey,
            Method::Local,
            AccessResult::Success,
            "confirm",
        )?;
        Ok(version)
    }

    pub fn rekey_abort(&self, rekey_id: &str) -> Result<(), ServiceError> {
        let mut session = self.session();
        self.logged(Operation::SharesRekey, Method::Local, "abort", || {
            self.require_state(&session, SealState::Unsealed)?;
            if session
                .pending_rekey
                .as_ref()
                .is_some_and(|pending| same_token(&pending.rekey_id, rekey_id))
            {
                session.pending_rekey = None;
            }
            Ok(())
        })?;
        self.log(
            Operation::SharesRekey,
            Method::Local,
            AccessResult::Success,
            "abort",
        )
    }
}
