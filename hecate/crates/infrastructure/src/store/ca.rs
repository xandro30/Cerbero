//! The tables of the identities: the streams `ca`, `identity/<name>` and
//! `certificate/<identity>/<serial>` in the shared `events` table, the certificates of the CA with
//! the intermediate keys wrapped by the root (`ca_material`) and the key of Hécate's own
//! certificate (`hecate_tls`), on one connection with one transaction (research R10).

use std::path::Path;
use std::sync::Mutex;

use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    CaMaterial, EventRecord, EventStore, IdentityStore, IdentityWork, IdentityWrites, OwnTlsKey,
};
use hecate_domain::identity::names::Serial;
use hecate_domain::sensitive::SecretBytes;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use zeroize::Zeroizing;

use super::secrets::{append, corrupt, integer, load, lock, natural, store_error, streams};
use super::{OpenError, open_database};

fn load_ca_material(connection: &Connection) -> Result<Vec<CaMaterial>, ServiceError> {
    let mut statement = connection
        .prepare(
            "SELECT generation, intermediate_version, root_certificate, intermediate_certificate,
                    key_nonce, key_ciphertext
             FROM ca_material ORDER BY generation, intermediate_version",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })
        .map_err(store_error)?;
    rows.map(|row| {
        let (generation, version, root, intermediate, key_nonce, key_ciphertext) =
            row.map_err(store_error)?;
        Ok(CaMaterial {
            generation: natural(generation)?,
            intermediate_version: natural(version)?,
            root_certificate: root,
            intermediate_certificate: intermediate,
            key_nonce,
            key_ciphertext,
        })
    })
    .collect()
}

/// The key is in clear (accepted risk, research R8); it is read into memory that is erased.
fn load_own_tls_key(connection: &Connection) -> Result<Option<OwnTlsKey>, ServiceError> {
    let row = connection
        .query_row(
            "SELECT serial, private_key FROM hecate_tls WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Zeroizing::new(row.get::<_, Vec<u8>>(1)?),
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    row.map(|(serial, private_key)| {
        Ok(OwnTlsKey {
            serial: Serial::parse(&serial).map_err(|_| corrupt())?,
            private_key: SecretBytes::new(private_key.to_vec()),
        })
    })
    .transpose()
}

pub struct SqliteIdentityStore {
    connection: Mutex<Connection>,
}

impl SqliteIdentityStore {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        Ok(Self {
            connection: Mutex::new(open_database(path)?),
        })
    }
}

impl EventStore for SqliteIdentityStore {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        load(&lock(&self.connection), stream)
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        streams(&lock(&self.connection), prefix)
    }
}

impl IdentityStore for SqliteIdentityStore {
    /// `BEGIN IMMEDIATE`; the events of every stream and the material commit together, and an
    /// error or a panic rolls them back.
    fn transaction(&self, work: &mut IdentityWork<'_>) -> Result<(), ServiceError> {
        let mut connection = lock(&self.connection);
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        work(&mut Writes(&transaction))?;
        transaction.commit().map_err(store_error)
    }

    fn load_ca_material(&self) -> Result<Vec<CaMaterial>, ServiceError> {
        load_ca_material(&lock(&self.connection))
    }

    fn load_own_tls_key(&self) -> Result<Option<OwnTlsKey>, ServiceError> {
        load_own_tls_key(&lock(&self.connection))
    }
}

struct Writes<'a>(&'a Connection);

impl IdentityWrites for Writes<'_> {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        load(self.0, stream)
    }

    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError> {
        append(self.0, stream, events, expected_version)
    }

    fn save_ca_material(&mut self, material: &CaMaterial) -> Result<(), ServiceError> {
        self.0
            .execute(
                "INSERT INTO ca_material
                 (generation, intermediate_version, root_certificate, intermediate_certificate,
                  key_nonce, key_ciphertext)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    integer(material.generation)?,
                    integer(material.intermediate_version)?,
                    material.root_certificate,
                    material.intermediate_certificate,
                    material.key_nonce,
                    material.key_ciphertext,
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn delete_ca_material(
        &mut self,
        generation: u64,
        intermediate_version: u64,
    ) -> Result<(), ServiceError> {
        self.0
            .execute(
                "DELETE FROM ca_material WHERE generation = ?1 AND intermediate_version = ?2",
                params![integer(generation)?, integer(intermediate_version)?],
            )
            .map_err(store_error)?;
        Ok(())
    }

    fn save_own_tls_key(&mut self, key: &OwnTlsKey) -> Result<(), ServiceError> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO hecate_tls (id, serial, private_key) VALUES (1, ?1, ?2)",
                params![key.serial.as_str(), key.private_key.expose()],
            )
            .map_err(store_error)?;
        Ok(())
    }
}
