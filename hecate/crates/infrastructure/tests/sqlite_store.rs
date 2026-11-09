use std::os::unix::fs::PermissionsExt;

use hecate_application::errors::ServiceError;
use hecate_application::ports::{AccessLog, SealRepository};
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::{PassphraseProtection, RecoveryProtection, SealConfig};
use hecate_infrastructure::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use time::OffsetDateTime;
use time::macros::datetime;

const NOW: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);

fn make_config(version: u64) -> SealConfig {
    SealConfig {
        config_version: version,
        initialized_at: NOW,
        updated_at: NOW,
        passphrase: PassphraseProtection {
            version,
            kdf: "argon2id".into(),
            kdf_opslimit: 1,
            kdf_memlimit: 8192,
            salt: vec![b's'; 16],
            aead: "AES-256-GCM".into(),
            nonce: vec![b'n'; 12],
            ciphertext: vec![b'c'; 48],
        },
        recovery: RecoveryProtection {
            version,
            share_set_id: vec![1, 2, 3, 4],
            threshold: 2,
            share_count: 3,
            aead: "AES-256-GCM".into(),
            nonce: vec![b'm'; 12],
            ciphertext: vec![b'd'; 48],
        },
    }
}

fn entry(
    operation: Operation,
    method: Method,
    result: AccessResult,
    detail: &str,
) -> AccessLogEntry {
    AccessLogEntry {
        at: NOW,
        operation,
        method,
        result,
        detail: detail.into(),
        subject: None,
        path: None,
        version: None,
    }
}

// py: integration/test_sqlite_store.py::test_config_and_attempts_survive_reopen
#[test]
fn config_and_attempts_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let repo = SqliteSealRepository::open(&path).unwrap();
    assert_eq!(repo.load_config().unwrap(), None);
    assert_eq!(repo.load_attempts().unwrap(), AttemptPolicy::default());
    let attempts = AttemptPolicy::default()
        .register_failure(NOW)
        .register_failure(NOW)
        .register_failure(NOW);
    repo.transaction(&mut |writes| {
        writes.save_config(&make_config(1))?;
        writes.save_attempts(&attempts)
    })
    .unwrap();
    drop(repo);

    let reopened = SqliteSealRepository::open(&path).unwrap();
    assert_eq!(reopened.load_config().unwrap(), Some(make_config(1)));
    assert_eq!(reopened.load_attempts().unwrap(), attempts);
}

// py: integration/test_sqlite_store.py::test_failure_inside_transaction_leaves_no_changes
#[test]
fn failure_inside_transaction_leaves_no_changes() {
    let dir = tempfile::tempdir().unwrap();
    let repo = SqliteSealRepository::open(&dir.path().join("hecate.db")).unwrap();
    repo.transaction(&mut |writes| writes.save_config(&make_config(1)))
        .unwrap();
    let outcome = repo.transaction(&mut |writes| {
        writes.save_config(&make_config(2))?;
        Err(ServiceError::internal("PowerCut"))
    });
    assert!(matches!(
        outcome,
        Err(ServiceError::Internal { kind: "PowerCut" })
    ));
    assert_eq!(repo.load_config().unwrap().unwrap().config_version, 1);
}

// py: integration/test_sqlite_store.py::test_data_dir_files_are_private
#[test]
fn data_dir_files_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let _repo = SqliteSealRepository::open(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// py: integration/test_sqlite_store.py::test_access_log_appends_and_rejects_update_and_delete
#[test]
fn access_log_appends_and_rejects_update_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let repo = SqliteSealRepository::open(&path).unwrap();
    let log = SqliteAccessLog::open(&path).unwrap();
    log.append(&entry(
        Operation::Unseal,
        Method::Passphrase,
        AccessResult::Failure,
        "bad passphrase",
    ))
    .unwrap();
    log.append(&entry(
        Operation::Seal,
        Method::Local,
        AccessResult::Success,
        "",
    ))
    .unwrap();
    let operations: Vec<Operation> = log
        .entries()
        .unwrap()
        .iter()
        .map(|entry| entry.operation)
        .collect();
    assert_eq!(operations, vec![Operation::Unseal, Operation::Seal]);
    drop(log);
    drop(repo);

    let raw = rusqlite::Connection::open(&path).unwrap();
    assert!(
        raw.execute("UPDATE access_log SET result = 'success'", [])
            .is_err()
    );
    assert!(raw.execute("DELETE FROM access_log", []).is_err());
}
