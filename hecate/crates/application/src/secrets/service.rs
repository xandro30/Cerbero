//! Stores, versions and resolves secrets (feature 002). Every operation needs Hécate unsealed,
//! except the expiry counts, which read metadata only. The order of the checks is the one of the
//! Python version (research R8), since it decides the error and the access log row.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use hecate_domain::events::{EventRecord, InvalidEvents};
use hecate_domain::instant::parse_iso;
use hecate_domain::keys::key::{Key, KeyEvent};
use hecate_domain::printable::is_printable;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::secrets::path::{IDENTITY_RULE, SecretPath, SecretReference, parse_identity};
use hecate_domain::secrets::secret::{Secret, SecretEvent, SecretOptions, VersionState};
use hecate_domain::secrets::value_type::ValueType;
use hecate_domain::sensitive::SecretKey;
use serde_json::Value;
use time::OffsetDateTime;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::errors::ServiceError;
use crate::ports::{AccessLog, Clock, CryptoProvider, SecretStore, SecretWrites};
use crate::sealing::service::SealingService;
use crate::secrets::envelope::Envelope;
use crate::secrets::settings::SecretsSettings;

pub const ADMIN: &str = "admin";
pub const MAX_ROTATE_EVERY: usize = 64;
pub const MAX_REFERENCES: usize = 100;
/// One message for every reason a reference cannot be resolved (FR-013, FR-014, SC-007).
pub const NOT_RESOLVED: &str = "secret not found";

const EXPIRES_AT_RULE: &str = "expires_at must be an ISO 8601 date and time";
const EXPECTED_RULE: &str = "expected_version is required and must be a number";
const VERSION_RULE: &str = "version must be a number of 1 or more";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub path: String,
    pub version: u64,
    pub fingerprint: String,
    pub stream_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretSummary {
    pub path: String,
    pub value_type: ValueType,
    pub active_version: Option<u64>,
    pub version_count: u64,
    pub rotate_every: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDetails {
    pub version: u64,
    pub state: VersionState,
    pub created_at: OffsetDateTime,
    pub deprecated_at: Option<OffsetDateTime>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretDetails {
    pub path: String,
    pub owner: String,
    pub value_type: ValueType,
    pub max_versions: u64,
    pub rotate_every: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub stream_version: u64,
    pub versions: Vec<VersionDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub version: u64,
    pub fingerprint: String,
    pub stream_version: u64,
}

/// A resolved secret: it erases itself, value included, when dropped, and can be neither cloned
/// nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::secrets::service::Resolved>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::secrets::service::Resolved>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::secrets::service::Resolved>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Resolved {
    pub path: String,
    pub version: u64,
    pub value: Zeroizing<String>,
    pub cache_ttl_soft_s: u64,
    pub cache_ttl_hard_s: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryCounts {
    pub secrets_expired: u64,
    pub secrets_expiring_soon: u64,
}

/// The optional metadata of `create`, as the channel received it (`null` when absent).
#[derive(Debug, Clone, Default)]
pub struct CreateOptions {
    pub max_versions: Value,
    pub rotate_every: Value,
    pub expires_at: Value,
}

/// What an operation tells the access log about itself: never a value or a fingerprint.
#[derive(Default)]
struct Trace {
    path: Option<String>,
    version: Option<u64>,
}

fn invalid(message: impl Into<String>) -> ServiceError {
    HecateError::invalid(message).into()
}

fn conflict(message: &str, current_version: Option<u64>) -> ServiceError {
    HecateError::state_conflict(None, Some(message.to_string()), current_version).into()
}

fn stale(current_version: u64) -> ServiceError {
    conflict(
        "the secret changed since it was read",
        Some(current_version),
    )
}

fn corrupt(_: InvalidEvents) -> ServiceError {
    ServiceError::internal("InvalidEvents")
}

fn result_for(error: &ServiceError) -> AccessResult {
    match error {
        ServiceError::Hecate(HecateError::InvalidInput(_)) => AccessResult::InvalidInput,
        ServiceError::Hecate(HecateError::StateConflict { .. }) => AccessResult::RejectedConflict,
        ServiceError::Hecate(HecateError::RateLimited { .. }) => AccessResult::RejectedRateLimited,
        ServiceError::Hecate(HecateError::NotFound(_)) => AccessResult::NotFound,
        _ => AccessResult::Failure,
    }
}

/// Only a well-formed path reaches the log: a malformed one could be a pasted value.
fn loggable_path(text: &str) -> Option<String> {
    SecretPath::parse(text).ok().map(|path| path.to_string())
}

/// An optional instant in extended ISO 8601 (FR-018); without an offset it is UTC.
fn instant_of(raw: &Value) -> Result<Option<OffsetDateTime>, ServiceError> {
    match raw {
        Value::Null => Ok(None),
        Value::String(text) => parse_iso(text)
            .map(Some)
            .map_err(|_| invalid(EXPIRES_AT_RULE)),
        _ => Err(invalid(EXPIRES_AT_RULE)),
    }
}

fn rotate_every_of(raw: &Value) -> Result<Option<String>, ServiceError> {
    match raw {
        Value::Null => Ok(None),
        Value::String(text)
            if (1..=MAX_ROTATE_EVERY).contains(&text.chars().count()) && is_printable(text) =>
        {
            Ok(Some(text.clone()))
        }
        _ => Err(invalid(format!(
            "rotate_every must be a text of 1 to {MAX_ROTATE_EVERY} characters"
        ))),
    }
}

/// The requested maximum; a value that is not an integer becomes 0, so that the aggregate
/// refuses it at the same point and with the same message as an integer out of range.
fn max_versions_of(raw: &Value, default: u64) -> i64 {
    match raw {
        Value::Null => i64::try_from(default).unwrap_or(0),
        Value::Number(number) => number.as_i64().unwrap_or(0),
        _ => 0,
    }
}

fn version_number(raw: &Value) -> Result<u64, ServiceError> {
    raw.as_u64()
        .filter(|number| *number >= 1)
        .ok_or_else(|| invalid(VERSION_RULE))
}

fn check_expected(secret: &Secret, raw: &Value) -> Result<(), ServiceError> {
    let Value::Number(number) = raw else {
        return Err(invalid(EXPECTED_RULE));
    };
    if !(number.is_i64() || number.is_u64()) {
        return Err(invalid(EXPECTED_RULE));
    }
    if number.as_u64() != Some(secret.stream_version) {
        return Err(stale(secret.stream_version));
    }
    Ok(())
}

fn parse_references(raw: &Value) -> Result<Vec<SecretReference>, ServiceError> {
    let rule = || invalid(format!("send a list of 1 to {MAX_REFERENCES} references"));
    let Value::Array(items) = raw else {
        return Err(rule());
    };
    if !(1..=MAX_REFERENCES).contains(&items.len()) {
        return Err(rule());
    }
    items
        .iter()
        .map(|item| match item {
            Value::String(text) => SecretReference::parse(text).map_err(ServiceError::from),
            _ => Err(invalid("a secret reference must be a text")),
        })
        .collect()
}

fn decode_secret(records: &[EventRecord]) -> Result<Option<Secret>, ServiceError> {
    if records.is_empty() {
        return Ok(None);
    }
    let events = records
        .iter()
        .map(SecretEvent::decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(corrupt)?;
    Secret::from_events(&events).map(Some).map_err(corrupt)
}

fn append(
    writes: &mut dyn SecretWrites,
    stream: &str,
    records: &[EventRecord],
    expected_version: u64,
) -> Result<u64, ServiceError> {
    writes
        .append(stream, records, expected_version)
        .map_err(|error| match error {
            ServiceError::Concurrency(conflict) => stale(conflict.current_version),
            other => other,
        })
}

fn encoded(events: &[SecretEvent]) -> Vec<EventRecord> {
    events.iter().map(SecretEvent::encode).collect()
}

pub struct SecretsService {
    sealing: Arc<SealingService>,
    store: Arc<dyn SecretStore>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
    settings: SecretsSettings,
    envelope: Envelope,
}

impl SecretsService {
    pub fn new(
        sealing: Arc<SealingService>,
        store: Arc<dyn SecretStore>,
        crypto: Arc<dyn CryptoProvider>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
        settings: SecretsSettings,
    ) -> Self {
        Self {
            sealing,
            store,
            access_log,
            clock,
            settings,
            envelope: Envelope::new(crypto),
        }
    }

    fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        &self,
        operation: Operation,
        subject: Option<&str>,
        method: Method,
        result: AccessResult,
        path: Option<String>,
        version: Option<u64>,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.access_log.append(&AccessLogEntry {
            at: self.now(),
            operation,
            method,
            result,
            detail: detail.to_string(),
            subject: subject.map(str::to_string),
            path,
            version,
        })
    }

    /// Leaves a row for the operation of the admin: its success or why it was refused.
    fn audited<T>(
        &self,
        operation: Operation,
        path: Option<&str>,
        method: Method,
        detail: &str,
        work: impl FnOnce(&mut Trace) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let mut trace = Trace {
            path: path.and_then(loggable_path),
            version: None,
        };
        let outcome = work(&mut trace);
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(
            operation,
            Some(ADMIN),
            method,
            result,
            trace.path,
            trace.version,
            detail,
        )?;
        outcome
    }

    /// Leaves a row for a recognized operation whose arguments were malformed.
    pub fn record_invalid_request(
        &self,
        operation: Operation,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.log(
            operation,
            Some(ADMIN),
            Method::Local,
            AccessResult::InvalidInput,
            None,
            None,
            detail,
        )
    }

    fn load_secret(&self, path: &SecretPath) -> Result<Option<Secret>, ServiceError> {
        decode_secret(&self.store.load(&path.stream_name())?)
    }

    fn require_secret(&self, path: &SecretPath) -> Result<Secret, ServiceError> {
        self.load_secret(path)?
            .ok_or_else(|| HecateError::not_found("secret not found").into())
    }

    fn require_secret_in(
        &self,
        writes: &mut dyn SecretWrites,
        path: &SecretPath,
    ) -> Result<Secret, ServiceError> {
        decode_secret(&writes.load(&path.stream_name())?)?
            .ok_or_else(|| HecateError::not_found("secret not found").into())
    }

    fn secret_from_stream(&self, stream: &str) -> Result<Secret, ServiceError> {
        decode_secret(&self.store.load(stream)?)?
            .ok_or_else(|| ServiceError::internal("InvalidEvents"))
    }

    /// The scope key of an identity and its version; created the first time.
    fn scope_key_in(
        &self,
        writes: &mut dyn SecretWrites,
        root: &SecretKey,
        identity: &str,
        now: OffsetDateTime,
    ) -> Result<(SecretKey, u64), ServiceError> {
        let stream = format!("key/{identity}");
        let records = writes.load(&stream)?;
        if !records.is_empty() {
            let events = records
                .iter()
                .map(KeyEvent::decode)
                .collect::<Result<Vec<_>, _>>()
                .map_err(corrupt)?;
            let current = Key::from_events(&events)
                .map_err(corrupt)?
                .current_version();
            let material = writes
                .load_key_material(identity, current)?
                .ok_or_else(|| ServiceError::internal("MissingKeyMaterial"))?;
            let scope_key = self
                .envelope
                .open_scope_key(root, identity, current, &material)?;
            return Ok((scope_key, current));
        }
        let events: Vec<EventRecord> = Key::create(identity, now)
            .iter()
            .map(KeyEvent::encode)
            .collect();
        let (scope_key, material) = self.envelope.new_scope_key(root, identity, 1)?;
        append(writes, &stream, &events, 0)?;
        writes.save_key_material(identity, 1, &material)?;
        Ok((scope_key, 1))
    }

    pub fn create(
        &self,
        path: &str,
        value: &str,
        value_type: &str,
        options: &CreateOptions,
    ) -> Result<Created, ServiceError> {
        self.audited(
            Operation::SecretCreate,
            Some(path),
            Method::Local,
            "create",
            |trace| {
                let root = self.sealing.root()?;
                let secret_path = SecretPath::parse(path)?;
                let kind = ValueType::parse(value_type)?;
                let data = kind.validate(value)?;
                let max_versions =
                    max_versions_of(&options.max_versions, self.settings.default_max_versions);
                let rotate_every = rotate_every_of(&options.rotate_every)?;
                let expires_at = instant_of(&options.expires_at)?;
                let now = self.now();
                let text = secret_path.to_string();
                let stream = secret_path.stream_name();
                let mut created = None;
                self.store.transaction(&mut |writes| {
                    if !writes.load(&stream)?.is_empty() {
                        return Err(conflict("a secret already exists at this path", None));
                    }
                    let (scope_key, key_version) =
                        self.scope_key_in(writes, &root, secret_path.owner(), now)?;
                    let fingerprint = self.envelope.fingerprint(&scope_key, data.expose());
                    let options = SecretOptions {
                        max_versions,
                        rotate_every: rotate_every.clone(),
                        expires_at,
                    };
                    let events = Secret::create(
                        &secret_path,
                        kind,
                        now,
                        &fingerprint,
                        key_version,
                        options,
                    )?;
                    let material = self.envelope.seal_value(
                        &scope_key,
                        &text,
                        1,
                        secret_path.owner(),
                        key_version,
                        data.expose(),
                    )?;
                    let stream_version = append(writes, &stream, &encoded(&events), 0)?;
                    writes.save_secret_material(&text, 1, &material)?;
                    created = Some(Created {
                        path: text.clone(),
                        version: 1,
                        fingerprint,
                        stream_version,
                    });
                    Ok(())
                })?;
                trace.version = Some(1);
                created.ok_or_else(|| ServiceError::internal("TransactionWithoutResult"))
            },
        )
    }

    pub fn list_secrets(&self, identity: &Value) -> Result<Vec<SecretSummary>, ServiceError> {
        self.audited(Operation::SecretList, None, Method::Local, "list", |_| {
            self.sealing.root()?;
            let prefix = match identity {
                Value::Null => "secret/".to_string(),
                Value::String(text) => format!("secret/{}/", parse_identity(text)?),
                _ => return Err(invalid(IDENTITY_RULE)),
            };
            let mut items = Vec::new();
            for stream in self.store.streams(&prefix)? {
                let secret = self.secret_from_stream(&stream)?;
                items.push(SecretSummary {
                    path: secret.path.to_string(),
                    value_type: secret.value_type,
                    active_version: secret.active_version(),
                    version_count: secret.live_versions().len() as u64,
                    rotate_every: secret.rotate_every.clone(),
                    expires_at: secret.expires_at,
                });
            }
            items.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(items)
        })
    }

    pub fn get(&self, path: &str) -> Result<SecretDetails, ServiceError> {
        self.audited(
            Operation::SecretGet,
            Some(path),
            Method::Local,
            "get",
            |_| {
                self.sealing.root()?;
                let secret = self.require_secret(&SecretPath::parse(path)?)?;
                let now = self.now();
                let versions = secret
                    .versions
                    .values()
                    .map(|version| {
                        Ok(VersionDetails {
                            version: version.number,
                            state: secret.state_of(version.number, now, self.settings.grace)?,
                            created_at: version.created_at,
                            deprecated_at: version.deprecated_at,
                            fingerprint: version.fingerprint.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, ServiceError>>()?;
                Ok(SecretDetails {
                    path: secret.path.to_string(),
                    owner: secret.owner.clone(),
                    value_type: secret.value_type,
                    max_versions: secret.max_versions,
                    rotate_every: secret.rotate_every.clone(),
                    expires_at: secret.expires_at,
                    stream_version: secret.stream_version,
                    versions,
                })
            },
        )
    }

    /// Resolves secrets of `identity`: all or nothing, with one `not_found` for every refusal.
    pub fn resolve(
        &self,
        identity: &str,
        references: &Value,
    ) -> Result<Vec<Resolved>, ServiceError> {
        let mut subject = None;
        let mut requested = None;
        match self.resolve_all(identity, references, &mut subject, &mut requested) {
            Err(error) => {
                let result = result_for(&error);
                self.log(
                    Operation::SecretResolve,
                    subject,
                    Method::Identity,
                    result,
                    requested,
                    None,
                    "resolve",
                )?;
                Err(error)
            }
            Ok(resolved) => {
                for item in &resolved {
                    self.log(
                        Operation::SecretResolve,
                        subject,
                        Method::Identity,
                        AccessResult::Success,
                        Some(item.path.clone()),
                        Some(item.version),
                        "resolve",
                    )?;
                }
                Ok(resolved)
            }
        }
    }

    fn resolve_all<'a>(
        &self,
        identity: &'a str,
        references: &Value,
        subject: &mut Option<&'a str>,
        requested: &mut Option<String>,
    ) -> Result<Vec<Resolved>, ServiceError> {
        let identity = parse_identity(identity)?;
        *subject = Some(identity);
        let root = self.sealing.root()?;
        let references = parse_references(references)?;
        let now = self.now();
        let mut scope_keys = HashMap::new();
        let mut resolved = Vec::new();
        for reference in &references {
            *requested = Some(reference.path.to_string());
            resolved.push(self.resolve_one(&root, identity, reference, now, &mut scope_keys)?);
        }
        Ok(resolved)
    }

    fn resolve_one(
        &self,
        root: &SecretKey,
        identity: &str,
        reference: &SecretReference,
        now: OffsetDateTime,
        scope_keys: &mut HashMap<u64, SecretKey>,
    ) -> Result<Resolved, ServiceError> {
        let not_found = || ServiceError::from(HecateError::not_found(NOT_RESOLVED));
        if reference.path.owner() != identity {
            return Err(not_found());
        }
        let secret = self.load_secret(&reference.path)?.ok_or_else(not_found)?;
        let number = reference
            .version
            .or_else(|| secret.active_version())
            .filter(|number| secret.versions.contains_key(number))
            .ok_or_else(not_found)?;
        let state = secret.state_of(number, now, self.settings.grace)?;
        if !matches!(state, VersionState::Active | VersionState::Deprecated) {
            return Err(not_found());
        }
        let text = reference.path.to_string();
        let material = self
            .store
            .load_secret_material(&text, number)?
            .ok_or_else(not_found)?;
        let scope_key = match scope_keys.entry(material.key_version) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let wrapped = self
                    .store
                    .load_key_material(identity, material.key_version)?
                    .ok_or_else(not_found)?;
                entry.insert(self.envelope.open_scope_key(
                    root,
                    identity,
                    material.key_version,
                    &wrapped,
                )?)
            }
        };
        let opened = self
            .envelope
            .open_value(scope_key, &text, number, identity, &material)?;
        let value = std::str::from_utf8(opened.expose())
            .map(|text| Zeroizing::new(text.to_string()))
            .map_err(|_| ServiceError::internal("UnicodeDecodeError"))?;
        Ok(Resolved {
            path: text,
            version: number,
            value,
            cache_ttl_soft_s: self.settings.cache_ttl_soft_s,
            cache_ttl_hard_s: self.settings.cache_ttl_hard_s,
        })
    }

    pub fn add_version(
        &self,
        path: &str,
        value: &str,
        expected_version: &Value,
    ) -> Result<Added, ServiceError> {
        self.audited(
            Operation::SecretAddVersion,
            Some(path),
            Method::Local,
            "add_version",
            |trace| {
                let root = self.sealing.root()?;
                let secret_path = SecretPath::parse(path)?;
                let now = self.now();
                let text = secret_path.to_string();
                let stream = secret_path.stream_name();
                let mut added = None;
                self.store.transaction(&mut |writes| {
                    let secret = self.require_secret_in(writes, &secret_path)?;
                    check_expected(&secret, expected_version)?;
                    let data = secret.value_type.validate(value)?;
                    let (scope_key, key_version) =
                        self.scope_key_in(writes, &root, &secret.owner, now)?;
                    let fingerprint = self.envelope.fingerprint(&scope_key, data.expose());
                    let events = secret.add_version(now, &fingerprint, key_version);
                    let number = events
                        .iter()
                        .find_map(|event| match event {
                            SecretEvent::VersionAdded { version, .. } => Some(*version),
                            _ => None,
                        })
                        .ok_or_else(|| ServiceError::internal("VersionNotAdded"))?;
                    let material = self.envelope.seal_value(
                        &scope_key,
                        &text,
                        number,
                        &secret.owner,
                        key_version,
                        data.expose(),
                    )?;
                    let stream_version =
                        append(writes, &stream, &encoded(&events), secret.stream_version)?;
                    writes.save_secret_material(&text, number, &material)?;
                    for event in &events {
                        if let SecretEvent::VersionDestroyed { version, .. } = event {
                            // The oldest one, over the maximum.
                            writes.delete_secret_material(&text, *version)?;
                        }
                    }
                    added = Some(Added {
                        version: number,
                        fingerprint,
                        stream_version,
                    });
                    Ok(())
                })?;
                let added =
                    added.ok_or_else(|| ServiceError::internal("TransactionWithoutResult"))?;
                trace.version = Some(added.version);
                Ok(added)
            },
        )
    }

    fn transition(
        &self,
        operation: Operation,
        name: &str,
        path: &str,
        version: &Value,
        expected_version: &Value,
        decide: impl Fn(&Secret, u64, OffsetDateTime) -> Result<Vec<SecretEvent>, HecateError>,
    ) -> Result<u64, ServiceError> {
        self.audited(operation, Some(path), Method::Local, name, |trace| {
            self.sealing.root()?;
            let secret_path = SecretPath::parse(path)?;
            let number = version_number(version)?;
            trace.version = Some(number);
            let now = self.now();
            let mut stream_version = 0;
            self.store.transaction(&mut |writes| {
                let secret = self.require_secret_in(writes, &secret_path)?;
                check_expected(&secret, expected_version)?;
                let events = decide(&secret, number, now)?;
                stream_version = append(
                    writes,
                    &secret_path.stream_name(),
                    &encoded(&events),
                    secret.stream_version,
                )?;
                Ok(())
            })?;
            Ok(stream_version)
        })
    }

    pub fn revoke(
        &self,
        path: &str,
        version: &Value,
        expected_version: &Value,
    ) -> Result<u64, ServiceError> {
        let grace = self.settings.grace;
        self.transition(
            Operation::SecretRevoke,
            "revoke",
            path,
            version,
            expected_version,
            |secret, number, now| secret.revoke(number, now, grace),
        )
    }

    pub fn restore(
        &self,
        path: &str,
        version: &Value,
        expected_version: &Value,
    ) -> Result<u64, ServiceError> {
        let grace = self.settings.grace;
        self.transition(
            Operation::SecretRestore,
            "restore",
            path,
            version,
            expected_version,
            |secret, number, now| secret.restore(number, now, grace),
        )
    }

    pub fn promote(
        &self,
        path: &str,
        version: &Value,
        expected_version: &Value,
    ) -> Result<u64, ServiceError> {
        let grace = self.settings.grace;
        self.transition(
            Operation::SecretPromote,
            "promote",
            path,
            version,
            expected_version,
            |secret, number, now| secret.promote(number, now, grace),
        )
    }

    /// Cryptographic erasure of a revoked version, with a proof (the passphrase or two shares).
    pub fn destroy(
        &self,
        path: &str,
        version: &Value,
        expected_version: &Value,
        proof: &Value,
    ) -> Result<u64, ServiceError> {
        let method = SealingService::proof_method(proof);
        self.audited(
            Operation::SecretDestroy,
            Some(path),
            method,
            "destroy",
            |trace| {
                self.sealing.root()?;
                let secret_path = SecretPath::parse(path)?;
                let number = version_number(version)?;
                trace.version = Some(number);
                let now = self.now();
                let grace = self.settings.grace;
                let secret = self.require_secret(&secret_path)?;
                check_expected(&secret, expected_version)?;
                // Refused before any attempt is spent.
                secret.destroy(number, now, grace)?;
                self.sealing.verify_proof(proof)?;
                let mut stream_version = 0;
                self.store.transaction(&mut |writes| {
                    let secret = self.require_secret_in(writes, &secret_path)?;
                    check_expected(&secret, expected_version)?;
                    let events = secret.destroy(number, now, grace)?;
                    stream_version = append(
                        writes,
                        &secret_path.stream_name(),
                        &encoded(&events),
                        secret.stream_version,
                    )?;
                    writes.delete_secret_material(&secret_path.to_string(), number)?;
                    Ok(())
                })?;
                Ok(stream_version)
            },
        )
    }

    /// How many secrets have expired or expire soon: metadata only, so it works sealed too, and
    /// it leaves no row in the access log.
    pub fn expiry_counts(&self, now: Option<OffsetDateTime>) -> Result<ExpiryCounts, ServiceError> {
        let now = now.unwrap_or_else(|| self.now());
        let horizon = now + self.settings.expiring_soon;
        let mut counts = ExpiryCounts {
            secrets_expired: 0,
            secrets_expiring_soon: 0,
        };
        for stream in self.store.streams("secret/")? {
            let secret = self.secret_from_stream(&stream)?;
            let Some(expires_at) = secret.expires_at else {
                continue;
            };
            if secret.live_versions().is_empty() {
                continue;
            }
            if expires_at <= now {
                counts.secrets_expired += 1;
            } else if expires_at <= horizon {
                counts.secrets_expiring_soon += 1;
            }
        }
        Ok(counts)
    }
}
