use std::path::Path;

use hecate_application::ports::AccessLog;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_infrastructure::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use time::OffsetDateTime;
use time::macros::datetime;

const AT: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);

/// The access_log table exactly as the feature 001 created it.
const OLD_SCHEMA: &str = "
CREATE TABLE access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    operation TEXT NOT NULL,
    method TEXT NOT NULL,
    result TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE TRIGGER access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
";

fn make_old_database(path: &Path) {
    let raw = rusqlite::Connection::open(path).unwrap();
    raw.execute_batch(OLD_SCHEMA).unwrap();
    for (operation, method, detail) in [
        ("unseal", "passphrase", "passphrase"),
        ("seal", "local", "seal"),
    ] {
        raw.execute(
            "INSERT INTO access_log (at, operation, method, result, detail) VALUES (?1, ?2, ?3, 'success', ?4)",
            ("2026-11-07T12:00:00+00:00", operation, method, detail),
        )
        .unwrap();
    }
}

fn columns(path: &Path) -> Vec<String> {
    let raw = rusqlite::Connection::open(path).unwrap();
    let mut statement = raw.prepare("PRAGMA table_info(access_log)").unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn entry(
    operation: Operation,
    detail: &str,
    subject: Option<&str>,
    path: Option<&str>,
    version: Option<u64>,
) -> AccessLogEntry {
    AccessLogEntry {
        at: AT,
        operation,
        method: Method::Local,
        result: AccessResult::Success,
        detail: detail.into(),
        subject: subject.map(str::to_string),
        path: path.map(str::to_string),
        version,
        identity: None,
        certificate: None,
    }
}

// py: integration/test_access_log_migration.py::test_a_feature_001_database_gains_the_new_columns_without_losing_rows
#[test]
fn a_feature_001_database_gains_the_new_columns_without_losing_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_old_database(&path);
    assert!(!columns(&path).contains(&"subject".to_string()));
    let log = SqliteAccessLog::open(&path).unwrap();
    let present = columns(&path);
    assert!(
        ["subject", "path", "version"]
            .iter()
            .all(|name| present.contains(&name.to_string()))
    );
    let old = log.entries().unwrap();
    let summary: Vec<_> = old
        .iter()
        .map(|e| (e.operation, e.method, e.result))
        .collect();
    assert_eq!(
        summary,
        [
            (Operation::Unseal, Method::Passphrase, AccessResult::Success),
            (Operation::Seal, Method::Local, AccessResult::Success),
        ]
    );
    assert!(
        old.iter()
            .all(|e| e.subject.is_none() && e.path.is_none() && e.version.is_none())
    );
}

// py: integration/test_access_log_migration.py::test_new_rows_keep_subject_path_and_version_and_old_ones_stay_empty
#[test]
fn new_rows_keep_subject_path_and_version_and_old_ones_stay_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_old_database(&path);
    let log = SqliteAccessLog::open(&path).unwrap();
    log.append(&entry(
        Operation::SecretAddVersion,
        "add_version",
        Some("admin"),
        Some("secret://hermes/token"),
        Some(2),
    ))
    .unwrap();
    log.append(&entry(Operation::Seal, "seal", None, None, None))
        .unwrap();
    let entries = log.entries().unwrap();
    assert_eq!(entries.len(), 4);
    let fields = |e: &AccessLogEntry| (e.subject.clone(), e.path.clone(), e.version);
    assert_eq!(fields(&entries[0]), (None, None, None));
    assert_eq!(
        fields(&entries[2]),
        (
            Some("admin".into()),
            Some("secret://hermes/token".into()),
            Some(2)
        )
    );
    assert_eq!(fields(&entries[3]), (None, None, None));
}

// py: integration/test_access_log_migration.py::test_opening_twice_and_from_other_connections_is_harmless
#[test]
fn opening_twice_and_from_other_connections_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_old_database(&path);
    for _ in 0..2 {
        drop(SqliteAccessLog::open(&path).unwrap());
        drop(SqliteSealRepository::open(&path).unwrap());
    }
    assert_eq!(
        columns(&path)
            .iter()
            .filter(|name| *name == "subject")
            .count(),
        1
    );
    assert_eq!(
        SqliteAccessLog::open(&path)
            .unwrap()
            .entries()
            .unwrap()
            .len(),
        2
    );
}

// py: integration/test_access_log_migration.py::test_the_log_stays_append_only_after_the_migration
#[test]
fn the_log_stays_append_only_after_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_old_database(&path);
    drop(SqliteAccessLog::open(&path).unwrap());
    let raw = rusqlite::Connection::open(&path).unwrap();
    let error = raw.execute("DELETE FROM access_log", []).unwrap_err();
    assert!(error.to_string().contains("append-only"));
}

// py: integration/test_access_log_migration.py::test_a_fresh_database_has_the_columns_from_the_start
#[test]
fn a_fresh_database_has_the_columns_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    drop(SqliteAccessLog::open(&path).unwrap());
    let present = columns(&path);
    assert!(
        ["subject", "path", "version"]
            .iter()
            .all(|name| present.contains(&name.to_string()))
    );
}
