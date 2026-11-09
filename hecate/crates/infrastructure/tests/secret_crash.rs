mod common;

use std::path::Path;
use std::sync::Arc;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    EventRecord, EventStore, KeyMaterial, MaterialStore, SecretMaterial, SecretStore, SecretWork,
    SecretWrites,
};
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE};
use hecate_infrastructure::store::secrets::SqliteSecretStore;
use hecate_infrastructure::testing::SqliteStack;
use serde_json::{Value, json};

const PATH: &str = "secret://hermes/telegram/bot_token";

fn proof() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

/// Stands for the process dying in the middle of a change: one write of the transactions fails.
struct Crashing {
    inner: Arc<SqliteSecretStore>,
    on: &'static str,
}

struct CrashOn<'a> {
    writes: &'a mut dyn SecretWrites,
    on: &'static str,
}

impl CrashOn<'_> {
    fn check(&self, write: &str) -> Result<(), ServiceError> {
        if self.on == write {
            return Err(ServiceError::internal("Crash"));
        }
        Ok(())
    }
}

impl SecretWrites for CrashOn<'_> {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        self.writes.load(stream)
    }

    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError> {
        self.writes.append(stream, events, expected_version)
    }

    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError> {
        self.check("save_secret_material")?;
        self.writes.save_secret_material(path, version, material)
    }

    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        self.writes.load_secret_material(path, version)
    }

    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError> {
        self.check("delete_secret_material")?;
        self.writes.delete_secret_material(path, version)
    }

    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError> {
        self.writes
            .save_key_material(identity, key_version, material)
    }

    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        self.writes.load_key_material(identity, key_version)
    }
}

impl EventStore for Crashing {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        self.inner.load(stream)
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        self.inner.streams(prefix)
    }
}

impl MaterialStore for Crashing {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        self.inner.load_secret_material(path, version)
    }

    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        self.inner.load_key_material(identity, key_version)
    }
}

impl SecretStore for Crashing {
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError> {
        let on = self.on;
        self.inner
            .transaction(&mut |writes| work(&mut CrashOn { writes, on }))
    }
}

/// A stack whose secrets service crashes on one write.
fn crashing(db: &Path, on: &'static str) -> SqliteStack {
    let stack = SqliteStack::open_with(db, Arc::new(FakeClock::new()), |store| {
        Arc::new(Crashing { inner: store, on })
    });
    initialize_if_needed(&stack);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    stack
}

fn initialize_if_needed(stack: &SqliteStack) {
    if stack.sealing.status().unwrap().state.as_str() == "uninitialized" {
        hecate_application::testing::initialize(&stack.sealing, GOOD_PASSPHRASE);
    }
}

fn reopen(db: &Path) -> SqliteStack {
    let stack = stack(db);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    stack
}

type Snapshot = (
    Vec<EventRecord>,
    Option<SecretMaterial>,
    Option<SecretMaterial>,
    Vec<EventRecord>,
);

fn snapshot(stack: &SqliteStack) -> Snapshot {
    (
        stack
            .store
            .load("secret/hermes/telegram/bot_token")
            .unwrap(),
        stack.store.load_secret_material(PATH, 1).unwrap(),
        stack.store.load_secret_material(PATH, 2).unwrap(),
        stack.store.load("key/hermes").unwrap(),
    )
}

fn crashed(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Internal { kind: "Crash" })
}

// py: integration/test_secret_crash.py::test_a_crash_while_creating_leaves_no_secret_and_no_key
#[test]
fn a_crash_while_creating_leaves_no_secret_and_no_key() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = crashing(&db, "save_secret_material");
    assert!(crashed(
        &stack
            .service
            .create(PATH, "value-one", "string", &no_options())
            .unwrap_err()
    ));
    stack.close();

    let again = reopen(&db);
    assert_eq!(snapshot(&again), (vec![], None, None, vec![]));
    assert_eq!(again.store.load_key_material("hermes", 1).unwrap(), None);
    assert!(again.service.list_secrets(&Value::Null).unwrap().is_empty());
    assert_eq!(
        again
            .service
            .create(PATH, "value-one", "string", &no_options())
            .unwrap()
            .version,
        1
    );
    again.close();
}

// py: integration/test_secret_crash.py::test_a_crash_while_adding_a_version_keeps_the_previous_state
#[test]
fn a_crash_while_adding_a_version_keeps_the_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let first = ready(&db);
    first
        .service
        .create(PATH, "value-one", "string", &no_options())
        .unwrap();
    let before = snapshot(&first);
    first.close();
    let stack = crashing(&db, "save_secret_material");
    assert!(crashed(
        &stack
            .service
            .add_version(PATH, "value-two", &json!(2))
            .unwrap_err()
    ));
    stack.close();

    let again = reopen(&db);
    assert_eq!(snapshot(&again), before);
    assert_eq!(
        again.service.resolve("hermes", &json!([PATH])).unwrap()[0]
            .value
            .as_str(),
        "value-one"
    );
    assert_eq!(
        again
            .service
            .add_version(PATH, "value-two", &json!(2))
            .unwrap()
            .version,
        2
    );
    again.close();
}

// py: integration/test_secret_crash.py::test_a_crash_while_destroying_keeps_the_version_revoked_with_its_material
#[test]
fn a_crash_while_destroying_keeps_the_version_revoked_with_its_material() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let first = ready(&db);
    first
        .service
        .create(PATH, "value-one", "string", &no_options())
        .unwrap();
    first
        .service
        .add_version(PATH, "value-two", &json!(2))
        .unwrap();
    first.service.revoke(PATH, &json!(1), &json!(4)).unwrap();
    let before = snapshot(&first);
    first.close();
    let stack = crashing(&db, "delete_secret_material");
    assert!(crashed(
        &stack
            .service
            .destroy(PATH, &json!(1), &json!(5), &proof())
            .unwrap_err()
    ));
    stack.close();

    let again = reopen(&db);
    assert_eq!(snapshot(&again), before);
    let state = again.service.get(PATH).unwrap().versions[0].state;
    assert_eq!(state.as_str(), "revoked");
    again
        .service
        .destroy(PATH, &json!(1), &json!(5), &proof())
        .unwrap();
    assert_eq!(again.store.load_secret_material(PATH, 1).unwrap(), None);
    again.close();
}

// py: integration/test_secret_crash.py::test_a_connection_that_dies_without_committing_changes_nothing
#[test]
fn a_connection_that_dies_without_committing_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = ready(&db);
    stack
        .service
        .create(PATH, "value-one", "string", &no_options())
        .unwrap();
    let before = snapshot(&stack);
    let material = stack.store.load_secret_material(PATH, 1).unwrap().unwrap();
    stack.close();
    // Begin a change by hand and drop the connection before COMMIT, like a killed process.
    let raw = rusqlite::Connection::open(&db).unwrap();
    raw.execute_batch("BEGIN IMMEDIATE").unwrap();
    raw.execute(
        "INSERT INTO key_material (identity, key_version, nonce, ciphertext) VALUES ('echidna', 1, ?1, ?2)",
        (vec![b'n'; 12], b"wrapped".to_vec()),
    )
    .unwrap();
    raw.execute(
        "INSERT INTO secret_material (path, version, key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext)
         VALUES (?1, 9, ?2, ?3, ?4, ?5, ?6)",
        (
            PATH,
            1,
            material.value_nonce.clone(),
            material.value_ciphertext.clone(),
            material.dek_nonce.clone(),
            material.dek_ciphertext.clone(),
        ),
    )
    .unwrap();
    drop(raw);

    let again = reopen(&db);
    assert_eq!(snapshot(&again), before);
    assert_eq!(again.store.load_key_material("echidna", 1).unwrap(), None);
    assert_eq!(again.store.load_secret_material(PATH, 9).unwrap(), None);
    again.close();
}
