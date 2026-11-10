//! SQLite (ADR 0003): the same schema, append-only triggers and pragmas as the Python version.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS seal_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    config_version INTEGER NOT NULL,
    initialized_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    passphrase TEXT NOT NULL,
    recovery TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS attempts (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    consecutive_failures INTEGER NOT NULL,
    next_attempt_at TEXT
);
CREATE TABLE IF NOT EXISTS access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    operation TEXT NOT NULL,
    method TEXT NOT NULL,
    result TEXT NOT NULL,
    detail TEXT NOT NULL,
    subject TEXT,
    path TEXT,
    version INTEGER,
    identity TEXT,
    certificate TEXT
);
CREATE TRIGGER IF NOT EXISTS access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TABLE IF NOT EXISTS events (
    stream TEXT NOT NULL,
    version INTEGER NOT NULL,
    type TEXT NOT NULL,
    data TEXT NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY (stream, version)
);
CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TABLE IF NOT EXISTS key_material (
    identity TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    PRIMARY KEY (identity, key_version)
);
CREATE TABLE IF NOT EXISTS secret_material (
    path TEXT NOT NULL,
    version INTEGER NOT NULL,
    key_version INTEGER NOT NULL,
    value_nonce BLOB NOT NULL,
    value_ciphertext BLOB NOT NULL,
    dek_nonce BLOB NOT NULL,
    dek_ciphertext BLOB NOT NULL,
    PRIMARY KEY (path, version)
);
CREATE TABLE IF NOT EXISTS ca_material (
    generation INTEGER NOT NULL,
    intermediate_version INTEGER NOT NULL,
    root_certificate BLOB NOT NULL,
    intermediate_certificate BLOB NOT NULL,
    key_nonce BLOB NOT NULL,
    key_ciphertext BLOB NOT NULL,
    PRIMARY KEY (generation, intermediate_version)
);
CREATE TABLE IF NOT EXISTS hecate_tls (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    serial TEXT NOT NULL,
    private_key BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS enroll_attempts (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    consecutive_failures INTEGER NOT NULL,
    wait_until TEXT,
    rejected_during_wait INTEGER NOT NULL
);
";

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// Opens (and creates, private to the user) the database with its pragmas and schema.
pub fn open_database(path: &Path) -> Result<Connection, OpenError> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(10))?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    // A deleted key must not linger in freed pages (cryptographic erasure).
    connection.pragma_update(None, "secure_delete", "ON")?;
    connection.execute_batch(SCHEMA)?;
    migrate_access_log(&connection)?;
    Ok(connection)
}

/// The columns that the access log of an older database lacks: `subject`, `path` and `version`
/// since the feature 001, and `identity` and `certificate` since the feature 003 (research R10).
const ACCESS_LOG_NEW_COLUMNS: [(&str, &str); 5] = [
    ("subject", "TEXT"),
    ("path", "TEXT"),
    ("version", "INTEGER"),
    ("identity", "TEXT"),
    ("certificate", "TEXT"),
];

fn access_log_columns(connection: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare("PRAGMA table_info(access_log)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    columns.collect()
}

/// Adds the columns that an older access log lacks.
fn migrate_access_log(connection: &Connection) -> rusqlite::Result<()> {
    let present = access_log_columns(connection)?;
    for (name, sql_type) in ACCESS_LOG_NEW_COLUMNS {
        if present.iter().any(|column| column == name) {
            continue;
        }
        if let Err(error) = connection.execute_batch(&format!(
            "ALTER TABLE access_log ADD COLUMN {name} {sql_type}"
        )) {
            // Another connection may have added it between the check and the change.
            if !access_log_columns(connection)?
                .iter()
                .any(|column| column == name)
            {
                return Err(error);
            }
        }
    }
    Ok(())
}

pub mod ca;
pub mod identity;
pub mod sealing;
pub mod secrets;
