//! The secrets tables: the append-only events of every stream, the scope keys wrapped by the
//! root and the encrypted material of every version, on one connection with one transaction.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    ConcurrencyConflict, EventRecord, EventStore, KeyMaterial, MaterialStore, SecretMaterial,
    SecretStore, SecretWork, SecretWrites,
};
use hecate_domain::instant::{isoformat, parse_iso};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;

use super::{OpenError, open_database};

fn store_error(_: rusqlite::Error) -> ServiceError {
    ServiceError::internal("SqliteError")
}

fn corrupt() -> ServiceError {
    ServiceError::internal("CorruptStoredData")
}

fn integer(value: u64) -> Result<i64, ServiceError> {
    i64::try_from(value).map_err(|_| ServiceError::internal("ValueOutOfRange"))
}

fn natural(value: i64) -> Result<u64, ServiceError> {
    u64::try_from(value).map_err(|_| corrupt())
}

fn lock(connection: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    connection.lock().unwrap_or_else(PoisonError::into_inner)
}

fn load(connection: &Connection, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
    let mut statement = connection
        .prepare("SELECT type, data, at FROM events WHERE stream = ?1 ORDER BY version")
        .map_err(store_error)?;
    let rows = statement
        .query_map([stream], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(store_error)?;
    rows.map(|row| {
        let (kind, data, at) = row.map_err(store_error)?;
        let Ok(Value::Object(data)) = serde_json::from_str(&data) else {
            return Err(corrupt());
        };
        Ok(EventRecord {
            kind,
            data,
            at: parse_iso(&at).map_err(|_| corrupt())?,
        })
    })
    .collect()
}

fn streams(connection: &Connection, prefix: &str) -> Result<Vec<String>, ServiceError> {
    let length = integer(prefix.chars().count() as u64)?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT stream FROM events WHERE substr(stream, 1, ?1) = ?2 ORDER BY stream",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![length, prefix], |row| row.get::<_, String>(0))
        .map_err(store_error)?;
    rows.map(|row| row.map_err(store_error)).collect()
}

fn load_secret_material(
    connection: &Connection,
    path: &str,
    version: u64,
) -> Result<Option<SecretMaterial>, ServiceError> {
    let row = connection
        .query_row(
            "SELECT key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext
             FROM secret_material WHERE path = ?1 AND version = ?2",
            params![path, integer(version)?],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    row.map(
        |(key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext)| {
            Ok(SecretMaterial {
                key_version: natural(key_version)?,
                value_nonce,
                value_ciphertext,
                dek_nonce,
                dek_ciphertext,
            })
        },
    )
    .transpose()
}

fn load_key_material(
    connection: &Connection,
    identity: &str,
    key_version: u64,
) -> Result<Option<KeyMaterial>, ServiceError> {
    connection
        .query_row(
            "SELECT nonce, ciphertext FROM key_material WHERE identity = ?1 AND key_version = ?2",
            params![identity, integer(key_version)?],
            |row| {
                Ok(KeyMaterial {
                    nonce: row.get(0)?,
                    ciphertext: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(store_error)
}

pub struct SqliteSecretStore {
    connection: Mutex<Connection>,
}

impl SqliteSecretStore {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        Ok(Self {
            connection: Mutex::new(open_database(path)?),
        })
    }
}

impl EventStore for SqliteSecretStore {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        load(&lock(&self.connection), stream)
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        streams(&lock(&self.connection), prefix)
    }
}

impl MaterialStore for SqliteSecretStore {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        load_secret_material(&lock(&self.connection), path, version)
    }

    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        load_key_material(&lock(&self.connection), identity, key_version)
    }
}

impl SecretStore for SqliteSecretStore {
    /// `BEGIN IMMEDIATE`; events and material commit together, and an error or a panic rolls
    /// them back.
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError> {
        let mut connection = lock(&self.connection);
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        work(&mut Writes(&transaction))?;
        transaction.commit().map_err(store_error)
    }
}

struct Writes<'a>(&'a Connection);

impl SecretWrites for Writes<'_> {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        load(self.0, stream)
    }

    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError> {
        let current: i64 = self
            .0
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM events WHERE stream = ?1",
                [stream],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        let current = natural(current)?;
        if current != expected_version {
            return Err(ConcurrencyConflict {
                current_version: current,
            }
            .into());
        }
        for (offset, event) in (1..).zip(events) {
            self.0
                .execute(
                    "INSERT INTO events (stream, version, type, data, at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        stream,
                        integer(current + offset)?,
                        event.kind,
                        Value::Object(event.data.clone()).to_string(),
                        isoformat(event.at),
                    ],
                )
                .map_err(store_error)?;
        }
        Ok(current + events.len() as u64)
    }

    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO secret_material
                 (path, version, key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    path,
                    integer(version)?,
                    integer(material.key_version)?,
                    material.value_nonce,
                    material.value_ciphertext,
                    material.dek_nonce,
                    material.dek_ciphertext,
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        load_secret_material(self.0, path, version)
    }

    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError> {
        self.0
            .execute(
                "DELETE FROM secret_material WHERE path = ?1 AND version = ?2",
                params![path, integer(version)?],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO key_material (identity, key_version, nonce, ciphertext) VALUES (?1, ?2, ?3, ?4)",
                params![identity, integer(key_version)?, material.nonce, material.ciphertext],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        load_key_material(self.0, identity, key_version)
    }
}
