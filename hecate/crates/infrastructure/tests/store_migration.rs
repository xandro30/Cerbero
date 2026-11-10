//! The migration of the identities (research R10): a database of the change 005 gains three
//! tables and two access log columns, idempotently and with two connections at a time.

mod common;

use std::path::Path;
use std::sync::{Arc, Barrier};

use hecate_infrastructure::store;
use rusqlite::Connection;

const NEW_TABLES: [&str; 3] = ["ca_material", "hecate_tls", "enroll_attempts"];

type SchemaRow = (String, String, String, Option<String>);

/// The statements of the schema vector of the change 005 that can be run by hand: SQLite makes
/// `sqlite_sequence` and the automatic indexes itself.
fn change_005_statements() -> Vec<String> {
    let text = std::fs::read_to_string(common::vectors_dir().join("schema.sql")).unwrap();
    text.split("-- object: ")
        .skip(1)
        .filter_map(|block| {
            let (header, body) = block.split_once('\n').unwrap();
            let name = header.split(' ').nth(1).unwrap();
            let body = body.trim_end();
            (!name.starts_with("sqlite_") && body != "-- no sql").then(|| body.to_string())
        })
        .collect()
}

/// A database exactly as the change 005 leaves it, with one access log row and one event.
fn make_change_005_database(path: &Path) {
    let connection = Connection::open(path).unwrap();
    for statement in change_005_statements() {
        connection.execute_batch(&statement).unwrap();
    }
    connection
        .execute(
            "INSERT INTO access_log (at, operation, method, result, detail, subject, path, version)
             VALUES ('2026-11-08T12:00:00.000000+00:00', 'secret_create', 'local', 'success',
                     'create', 'admin', 'secret://hermes/token', 1)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO events (stream, version, type, data, at)
             VALUES ('key/hermes', 1, 'ScopeKeyCreated', '{\"identity\":\"hermes\",\"key_version\":1}',
                     '2026-11-08T12:00:00.000000+00:00')",
            [],
        )
        .unwrap();
}

fn columns(connection: &Connection, table: &str) -> Vec<(String, String, bool)> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn column(name: &str, sql_type: &str, not_null: bool) -> (String, String, bool) {
    (name.to_string(), sql_type.to_string(), not_null)
}

fn snapshot(connection: &Connection) -> Vec<SchemaRow> {
    let mut statement = connection
        .prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn a_change_005_database_gains_the_tables_and_columns_of_the_identities_and_keeps_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_change_005_database(&path);
    let connection = store::open_database(&path).unwrap();
    let tables: Vec<String> = snapshot(&connection)
        .into_iter()
        .filter(|row| row.0 == "table")
        .map(|row| row.1)
        .collect();
    for table in NEW_TABLES {
        assert!(tables.contains(&table.to_string()), "{table}: {tables:?}");
    }
    let access_log = columns(&connection, "access_log");
    for name in ["identity", "certificate"] {
        assert!(access_log.contains(&column(name, "TEXT", false)), "{name}");
    }
    let kept: (String, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT operation, identity, certificate FROM access_log",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(kept, ("secret_create".to_string(), None, None));
    let events: i64 = connection
        .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(events, 1);
}

#[test]
fn opening_the_database_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_change_005_database(&path);
    let first = snapshot(&store::open_database(&path).unwrap());
    let second = store::open_database(&path).unwrap();
    assert_eq!(snapshot(&second), first);
    let names: Vec<String> = columns(&second, "access_log")
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "id",
            "at",
            "operation",
            "method",
            "result",
            "detail",
            "subject",
            "path",
            "version",
            "identity",
            "certificate"
        ]
    );
}

#[test]
fn two_connections_that_open_a_change_005_database_at_once_both_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    make_change_005_database(&path);
    let barrier = Arc::new(Barrier::new(2));
    let openers: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store::open_database(&path).map(|_| ())
            })
        })
        .collect();
    for opener in openers {
        assert!(opener.join().unwrap().is_ok());
    }
    let connection = store::open_database(&path).unwrap();
    let identities = columns(&connection, "access_log")
        .into_iter()
        .filter(|(name, _, _)| name == "identity")
        .count();
    assert_eq!(identities, 1);
}

#[test]
fn the_new_tables_have_their_columns() {
    let dir = tempfile::tempdir().unwrap();
    let connection = store::open_database(&dir.path().join("hecate.db")).unwrap();
    assert_eq!(
        columns(&connection, "ca_material"),
        [
            column("generation", "INTEGER", true),
            column("intermediate_version", "INTEGER", true),
            column("root_certificate", "BLOB", true),
            column("intermediate_certificate", "BLOB", true),
            column("key_nonce", "BLOB", true),
            column("key_ciphertext", "BLOB", true),
        ]
    );
    assert_eq!(
        columns(&connection, "hecate_tls"),
        [
            column("id", "INTEGER", false),
            column("serial", "TEXT", true),
            column("private_key", "BLOB", true),
        ]
    );
    assert_eq!(
        columns(&connection, "enroll_attempts"),
        [
            column("id", "INTEGER", false),
            column("consecutive_failures", "INTEGER", true),
            column("wait_until", "TEXT", false),
            column("rejected_during_wait", "INTEGER", true),
        ]
    );
}
