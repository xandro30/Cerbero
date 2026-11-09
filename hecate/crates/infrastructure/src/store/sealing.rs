//! The sealing tables: the seal configuration, with each protection in JSON (the same content as
//! in the Python version, bytes in hexadecimal), the attempt counter and the append-only access log.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use data_encoding::{HEXLOWER, HEXLOWER_PERMISSIVE};
use hecate_application::errors::ServiceError;
use hecate_application::ports::{AccessLog, SealRepository, SealWork, SealWrites};
use hecate_domain::instant::{isoformat, parse_iso};
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::{PassphraseProtection, RecoveryProtection, SealConfig};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Map, Value};
use time::OffsetDateTime;

use super::{OpenError, open_database};

fn store_error(_: rusqlite::Error) -> ServiceError {
    ServiceError::internal("SqliteError")
}

fn corrupt() -> ServiceError {
    ServiceError::internal("CorruptStoredData")
}

fn lock(connection: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    connection.lock().unwrap_or_else(PoisonError::into_inner)
}

/// SQLite integers are signed; the counters and versions never reach the sign bit.
fn integer(value: u64) -> Result<i64, ServiceError> {
    i64::try_from(value).map_err(|_| ServiceError::internal("ValueOutOfRange"))
}

fn natural(value: i64) -> Result<u64, ServiceError> {
    u64::try_from(value).map_err(|_| corrupt())
}

fn instant(text: &str) -> Result<OffsetDateTime, ServiceError> {
    parse_iso(text).map_err(|_| corrupt())
}

fn object(text: &str, keys: &[&str]) -> Result<Map<String, Value>, ServiceError> {
    match serde_json::from_str(text) {
        Ok(Value::Object(fields))
            if fields.len() == keys.len() && keys.iter().all(|key| fields.contains_key(*key)) =>
        {
            Ok(fields)
        }
        _ => Err(corrupt()),
    }
}

fn number(fields: &Map<String, Value>, key: &str) -> Result<u64, ServiceError> {
    fields[key].as_u64().ok_or_else(corrupt)
}

fn small(fields: &Map<String, Value>, key: &str) -> Result<u32, ServiceError> {
    u32::try_from(number(fields, key)?).map_err(|_| corrupt())
}

fn text(fields: &Map<String, Value>, key: &str) -> Result<String, ServiceError> {
    fields[key].as_str().map(str::to_string).ok_or_else(corrupt)
}

fn bytes(fields: &Map<String, Value>, key: &str) -> Result<Vec<u8>, ServiceError> {
    let hex = fields[key].as_str().ok_or_else(corrupt)?;
    HEXLOWER_PERMISSIVE
        .decode(hex.as_bytes())
        .map_err(|_| corrupt())
}

fn document(fields: Vec<(&str, Value)>) -> String {
    let fields: Map<String, Value> = fields
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();
    Value::Object(fields).to_string()
}

const PASSPHRASE_KEYS: [&str; 8] = [
    "aead",
    "ciphertext",
    "kdf",
    "kdf_memlimit",
    "kdf_opslimit",
    "nonce",
    "salt",
    "version",
];

const RECOVERY_KEYS: [&str; 7] = [
    "aead",
    "ciphertext",
    "nonce",
    "share_count",
    "share_set_id",
    "threshold",
    "version",
];

/// The JSON that `seal_config.passphrase` keeps.
pub fn dump_passphrase(protection: &PassphraseProtection) -> String {
    document(vec![
        ("aead", Value::from(protection.aead.as_str())),
        (
            "ciphertext",
            Value::from(HEXLOWER.encode(&protection.ciphertext)),
        ),
        ("kdf", Value::from(protection.kdf.as_str())),
        ("kdf_memlimit", Value::from(protection.kdf_memlimit)),
        ("kdf_opslimit", Value::from(protection.kdf_opslimit)),
        ("nonce", Value::from(HEXLOWER.encode(&protection.nonce))),
        ("salt", Value::from(HEXLOWER.encode(&protection.salt))),
        ("version", Value::from(protection.version)),
    ])
}

pub fn load_passphrase(text_: &str) -> Result<PassphraseProtection, ServiceError> {
    let fields = object(text_, &PASSPHRASE_KEYS)?;
    Ok(PassphraseProtection {
        version: number(&fields, "version")?,
        kdf: text(&fields, "kdf")?,
        kdf_opslimit: number(&fields, "kdf_opslimit")?,
        kdf_memlimit: number(&fields, "kdf_memlimit")?,
        salt: bytes(&fields, "salt")?,
        aead: text(&fields, "aead")?,
        nonce: bytes(&fields, "nonce")?,
        ciphertext: bytes(&fields, "ciphertext")?,
    })
}

/// The JSON that `seal_config.recovery` keeps.
pub fn dump_recovery(protection: &RecoveryProtection) -> String {
    document(vec![
        ("aead", Value::from(protection.aead.as_str())),
        (
            "ciphertext",
            Value::from(HEXLOWER.encode(&protection.ciphertext)),
        ),
        ("nonce", Value::from(HEXLOWER.encode(&protection.nonce))),
        ("share_count", Value::from(protection.share_count)),
        (
            "share_set_id",
            Value::from(HEXLOWER.encode(&protection.share_set_id)),
        ),
        ("threshold", Value::from(protection.threshold)),
        ("version", Value::from(protection.version)),
    ])
}

pub fn load_recovery(text_: &str) -> Result<RecoveryProtection, ServiceError> {
    let fields = object(text_, &RECOVERY_KEYS)?;
    Ok(RecoveryProtection {
        version: number(&fields, "version")?,
        share_set_id: bytes(&fields, "share_set_id")?,
        threshold: small(&fields, "threshold")?,
        share_count: small(&fields, "share_count")?,
        aead: text(&fields, "aead")?,
        nonce: bytes(&fields, "nonce")?,
        ciphertext: bytes(&fields, "ciphertext")?,
    })
}

fn read_config(connection: &Connection) -> Result<Option<SealConfig>, ServiceError> {
    let row = connection
        .query_row(
            "SELECT config_version, initialized_at, updated_at, passphrase, recovery FROM seal_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    let Some((config_version, initialized_at, updated_at, passphrase, recovery)) = row else {
        return Ok(None);
    };
    Ok(Some(SealConfig {
        config_version: natural(config_version)?,
        initialized_at: instant(&initialized_at)?,
        updated_at: instant(&updated_at)?,
        passphrase: load_passphrase(&passphrase)?,
        recovery: load_recovery(&recovery)?,
    }))
}

fn write_config(connection: &Connection, config: &SealConfig) -> Result<(), ServiceError> {
    connection
        .execute(
            "INSERT INTO seal_config (id, config_version, initialized_at, updated_at, passphrase, recovery)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET config_version = excluded.config_version,
             initialized_at = excluded.initialized_at, updated_at = excluded.updated_at,
             passphrase = excluded.passphrase, recovery = excluded.recovery",
            params![
                integer(config.config_version)?,
                isoformat(config.initialized_at),
                isoformat(config.updated_at),
                dump_passphrase(&config.passphrase),
                dump_recovery(&config.recovery),
            ],
        )
        .map_err(store_error)?;
    Ok(())
}

fn read_attempts(connection: &Connection) -> Result<AttemptPolicy, ServiceError> {
    let row = connection
        .query_row(
            "SELECT consecutive_failures, next_attempt_at FROM attempts WHERE id = 1",
            [],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(store_error)?;
    let Some((consecutive_failures, next_attempt_at)) = row else {
        return Ok(AttemptPolicy::default());
    };
    Ok(AttemptPolicy {
        consecutive_failures,
        next_attempt_at: next_attempt_at.as_deref().map(instant).transpose()?,
    })
}

fn write_attempts(connection: &Connection, attempts: &AttemptPolicy) -> Result<(), ServiceError> {
    connection
        .execute(
            "INSERT INTO attempts (id, consecutive_failures, next_attempt_at) VALUES (1, ?1, ?2)
             ON CONFLICT (id) DO UPDATE SET consecutive_failures = excluded.consecutive_failures,
             next_attempt_at = excluded.next_attempt_at",
            params![
                attempts.consecutive_failures,
                attempts.next_attempt_at.map(isoformat)
            ],
        )
        .map_err(store_error)?;
    Ok(())
}

/// The seal configuration and the attempt counter.
pub struct SqliteSealRepository {
    connection: Mutex<Connection>,
}

impl SqliteSealRepository {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        Ok(Self {
            connection: Mutex::new(open_database(path)?),
        })
    }
}

struct Writes<'a>(&'a Connection);

impl SealWrites for Writes<'_> {
    fn load_config(&mut self) -> Result<Option<SealConfig>, ServiceError> {
        read_config(self.0)
    }

    fn save_config(&mut self, config: &SealConfig) -> Result<(), ServiceError> {
        write_config(self.0, config)
    }

    fn load_attempts(&mut self) -> Result<AttemptPolicy, ServiceError> {
        read_attempts(self.0)
    }

    fn save_attempts(&mut self, attempts: &AttemptPolicy) -> Result<(), ServiceError> {
        write_attempts(self.0, attempts)
    }
}

impl SealRepository for SqliteSealRepository {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError> {
        read_config(&lock(&self.connection))
    }

    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError> {
        read_attempts(&lock(&self.connection))
    }

    /// `BEGIN IMMEDIATE`; the writes commit together, and an error or a panic rolls them back.
    fn transaction(&self, work: &mut SealWork<'_>) -> Result<(), ServiceError> {
        let mut connection = lock(&self.connection);
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        work(&mut Writes(&transaction))?;
        transaction.commit().map_err(store_error)
    }
}

/// The access log, on its own connection so that a rolled-back change still leaves its row.
pub struct SqliteAccessLog {
    connection: Mutex<Connection>,
}

impl SqliteAccessLog {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        Ok(Self {
            connection: Mutex::new(open_database(path)?),
        })
    }

    pub fn entries(&self) -> Result<Vec<AccessLogEntry>, ServiceError> {
        let connection = lock(&self.connection);
        let mut statement = connection
            .prepare("SELECT at, operation, method, result, detail, subject, path, version FROM access_log ORDER BY id")
            .map_err(store_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .map_err(store_error)?;
        rows.map(|row| {
            let (at, operation, method, result, detail, subject, path, version) =
                row.map_err(store_error)?;
            Ok(AccessLogEntry {
                at: instant(&at)?,
                operation: Operation::parse(&operation).ok_or_else(corrupt)?,
                method: Method::parse(&method).ok_or_else(corrupt)?,
                result: AccessResult::parse(&result).ok_or_else(corrupt)?,
                detail,
                subject,
                path,
                version: version.map(natural).transpose()?,
            })
        })
        .collect()
    }
}

impl AccessLog for SqliteAccessLog {
    fn append(&self, entry: &AccessLogEntry) -> Result<(), ServiceError> {
        lock(&self.connection)
            .execute(
                "INSERT INTO access_log (at, operation, method, result, detail, subject, path, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    isoformat(entry.at),
                    entry.operation.as_str(),
                    entry.method.as_str(),
                    entry.result.as_str(),
                    entry.detail,
                    entry.subject,
                    entry.path,
                    entry.version.map(integer).transpose()?,
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }
}
