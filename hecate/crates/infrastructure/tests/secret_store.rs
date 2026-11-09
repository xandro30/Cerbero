use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    ConcurrencyConflict, EventRecord, EventStore, KeyMaterial, MaterialStore, SecretMaterial,
    SecretStore,
};
use hecate_infrastructure::store::secrets::SqliteSecretStore;
use serde_json::json;
use time::OffsetDateTime;
use time::macros::datetime;

const AT: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);
const STREAM: &str = "secret/hermes/token";
const PATH: &str = "secret://hermes/token";

fn record(n: u64) -> EventRecord {
    EventRecord {
        kind: "VersionAdded".into(),
        data: json!({"version": n, "fingerprint": "aaaa-bbbb-cccc-dddd"})
            .as_object()
            .unwrap()
            .clone(),
        at: AT,
    }
}

fn material(tag: u8) -> SecretMaterial {
    SecretMaterial {
        key_version: 1,
        value_nonce: vec![tag; 12],
        value_ciphertext: [b"value-ct".as_slice(), &[tag]].concat(),
        dek_nonce: vec![tag + 1; 12],
        dek_ciphertext: [b"dek-ct".as_slice(), &[tag]].concat(),
    }
}

fn wrapped(ciphertext: &[u8]) -> KeyMaterial {
    KeyMaterial {
        nonce: vec![b'n'; 12],
        ciphertext: ciphertext.to_vec(),
    }
}

fn open(dir: &tempfile::TempDir) -> SqliteSecretStore {
    SqliteSecretStore::open(&dir.path().join("hecate.db")).unwrap()
}

/// An append in a transaction of its own, as Python's store does outside a transaction.
fn append(
    store: &SqliteSecretStore,
    stream: &str,
    records: &[EventRecord],
    expected: u64,
) -> Result<u64, ServiceError> {
    let mut version = 0;
    store.transaction(&mut |writes| {
        version = writes.append(stream, records, expected)?;
        Ok(())
    })?;
    Ok(version)
}

fn conflict_version(error: ServiceError) -> u64 {
    match error {
        ServiceError::Concurrency(ConcurrencyConflict { current_version }) => current_version,
        other => panic!("not a concurrency conflict: {other:?}"),
    }
}

// py: integration/test_secret_store.py::test_events_are_appended_with_the_expected_version
#[test]
fn events_are_appended_with_the_expected_version() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert_eq!(
        append(&store, STREAM, &[record(1), record(2)], 0).unwrap(),
        2
    );
    assert_eq!(append(&store, STREAM, &[record(3)], 2).unwrap(), 3);
    let loaded = store.load(STREAM).unwrap();
    let versions: Vec<u64> = loaded
        .iter()
        .map(|r| r.data["version"].as_u64().unwrap())
        .collect();
    assert_eq!(versions, [1, 2, 3]);
    assert_eq!(loaded[0], record(1));
}

// py: integration/test_secret_store.py::test_a_stale_expected_version_conflicts_and_writes_nothing
#[test]
fn a_stale_expected_version_conflicts_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    append(&store, STREAM, &[record(1)], 0).unwrap();
    assert_eq!(
        conflict_version(append(&store, STREAM, &[record(2), record(3)], 0).unwrap_err()),
        1
    );
    assert!(append(&store, STREAM, &[record(2)], 5).is_err());
    assert_eq!(store.load(STREAM).unwrap().len(), 1);
}

// py: integration/test_secret_store.py::test_streams_are_independent_and_listed_by_prefix
#[test]
fn streams_are_independent_and_listed_by_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    for stream in [
        "secret/hermes/token",
        "secret/hermes/certs/proxy",
        "secret/echidna/key",
        "key/hermes",
    ] {
        append(&store, stream, &[record(1)], 0).unwrap();
    }
    assert!(store.load("secret/none").unwrap().is_empty());
    assert_eq!(
        store.streams("secret/hermes/").unwrap(),
        ["secret/hermes/certs/proxy", "secret/hermes/token"]
    );
    assert_eq!(store.streams("secret/").unwrap().len(), 3);
    assert_eq!(store.streams("key/").unwrap(), ["key/hermes"]);
    assert!(
        store.streams("secret/he_mes/").unwrap().is_empty(),
        "`_` is not a wildcard"
    );
}

// py: integration/test_secret_store.py::test_events_and_material_survive_closing_and_reopening
#[test]
fn events_and_material_survive_closing_and_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let first = open(&dir);
    append(&first, STREAM, &[record(1)], 0).unwrap();
    first
        .transaction(&mut |writes| {
            writes.save_secret_material(PATH, 1, &material(1))?;
            writes.save_key_material("hermes", 1, &wrapped(b"wrapped"))
        })
        .unwrap();
    drop(first);
    let second = open(&dir);
    assert_eq!(second.load(STREAM).unwrap(), [record(1)]);
    assert_eq!(
        second.load_secret_material(PATH, 1).unwrap(),
        Some(material(1))
    );
    assert_eq!(
        second.load_key_material("hermes", 1).unwrap(),
        Some(wrapped(b"wrapped"))
    );
}

// py: integration/test_secret_store.py::test_missing_material_is_none
#[test]
fn missing_material_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert_eq!(store.load_secret_material(PATH, 1).unwrap(), None);
    assert_eq!(store.load_key_material("hermes", 1).unwrap(), None);
}

// py: integration/test_secret_store.py::test_deleting_the_material_of_a_version_keeps_its_events
#[test]
fn deleting_the_material_of_a_version_keeps_its_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    append(&store, STREAM, &[record(1), record(2)], 0).unwrap();
    store
        .transaction(&mut |writes| {
            writes.save_secret_material(PATH, 1, &material(1))?;
            writes.save_secret_material(PATH, 2, &material(2))?;
            writes.delete_secret_material(PATH, 1)
        })
        .unwrap();
    assert_eq!(store.load_secret_material(PATH, 1).unwrap(), None);
    assert_eq!(
        store.load_secret_material(PATH, 2).unwrap(),
        Some(material(2))
    );
    assert_eq!(store.load(STREAM).unwrap().len(), 2);
    store
        .transaction(&mut |writes| writes.delete_secret_material(PATH, 1))
        .unwrap();
}

// py: integration/test_secret_store.py::test_a_failure_inside_the_transaction_leaves_neither_events_nor_material
#[test]
fn a_failure_inside_the_transaction_leaves_neither_events_nor_material() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let outcome = store.transaction(&mut |writes| {
        writes.append(STREAM, &[record(1)], 0)?;
        writes.save_secret_material(PATH, 1, &material(1))?;
        writes.save_key_material("hermes", 1, &wrapped(b"w"))?;
        Err(ServiceError::internal("Boom"))
    });
    assert!(outcome.is_err());
    assert!(store.load(STREAM).unwrap().is_empty());
    assert_eq!(store.load_secret_material(PATH, 1).unwrap(), None);
    assert_eq!(store.load_key_material("hermes", 1).unwrap(), None);
}

// py: integration/test_secret_store.py::test_a_committed_transaction_keeps_everything
#[test]
fn a_committed_transaction_keeps_everything() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store
        .transaction(&mut |writes| {
            writes.append(STREAM, &[record(1)], 0)?;
            writes.save_secret_material(PATH, 1, &material(1))
        })
        .unwrap();
    assert_eq!(store.load(STREAM).unwrap().len(), 1);
    assert_eq!(
        store.load_secret_material(PATH, 1).unwrap(),
        Some(material(1))
    );
}

// py: integration/test_secret_store.py::test_a_conflict_inside_a_transaction_rolls_back_the_rest
#[test]
fn a_conflict_inside_a_transaction_rolls_back_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    append(&store, STREAM, &[record(1)], 0).unwrap();
    let outcome = store.transaction(&mut |writes| {
        writes.save_secret_material(PATH, 9, &material(1))?;
        writes.append(STREAM, &[record(2)], 0).map(|_| ())
    });
    assert_eq!(conflict_version(outcome.unwrap_err()), 1);
    assert_eq!(store.load_secret_material(PATH, 9).unwrap(), None);
}

// py: integration/test_secret_store.py::test_the_events_table_is_append_only
#[test]
fn the_events_table_is_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    append(&store, STREAM, &[record(1)], 0).unwrap();
    drop(store);
    let raw = rusqlite::Connection::open(dir.path().join("hecate.db")).unwrap();
    for statement in ["UPDATE events SET type = 'x'", "DELETE FROM events"] {
        let error = raw.execute(statement, []).unwrap_err();
        assert!(error.to_string().contains("append-only"), "{error}");
    }
}
