//! T041: the SQLite store of the identities: the streams `ca`, `identity/*` and `certificate/*`,
//! `ca_material` and `hecate_tls`, with transactions over several streams.

use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    CaMaterial, ConcurrencyConflict, EventRecord, EventStore, IdentityStore, OwnTlsKey,
};
use hecate_domain::identity::names::Serial;
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::store::ca::SqliteIdentityStore;
use rusqlite::Connection;
use serde_json::json;
use time::OffsetDateTime;
use time::macros::datetime;

const AT: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);

fn record(kind: &str) -> EventRecord {
    EventRecord {
        kind: kind.into(),
        data: json!({"generation": 1}).as_object().unwrap().clone(),
        at: AT,
    }
}

fn material(generation: u64, version: u64) -> CaMaterial {
    CaMaterial {
        generation,
        intermediate_version: version,
        root_certificate: b"root der".to_vec(),
        intermediate_certificate: vec![b'i', version as u8],
        key_nonce: vec![7; 12],
        key_ciphertext: vec![8; 48],
    }
}

fn own_key(serial: &str, key: &[u8]) -> OwnTlsKey {
    OwnTlsKey {
        serial: Serial::parse(serial).unwrap(),
        private_key: SecretBytes::new(key.to_vec()),
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    store: SqliteIdentityStore,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteIdentityStore::open(&dir.path().join("hecate.db")).unwrap();
        Self { dir, store }
    }

    fn reopened(&self) -> SqliteIdentityStore {
        SqliteIdentityStore::open(&self.dir.path().join("hecate.db")).unwrap()
    }

    fn count(&self, table: &str) -> i64 {
        Connection::open(self.dir.path().join("hecate.db"))
            .unwrap()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }
}

/// A transaction that writes the creation of a CA: two streams, its material and Hécate's key.
fn create_ca(store: &SqliteIdentityStore) -> Result<(), ServiceError> {
    store.transaction(&mut |writes| {
        writes.append("ca", &[record("CaCreated")], 0)?;
        writes.append("certificate/hecate/5f1c", &[record("CertificateIssued")], 0)?;
        writes.save_ca_material(&material(1, 1))?;
        writes.save_own_tls_key(&own_key("5f1c", b"pkcs8 of hecate"))?;
        Ok(())
    })
}

#[test]
fn several_streams_and_the_material_commit_together() {
    let fixture = Fixture::new();
    create_ca(&fixture.store).unwrap();
    let store = fixture.reopened();
    assert_eq!(store.load("ca").unwrap(), vec![record("CaCreated")]);
    assert_eq!(
        store.streams("certificate/hecate/").unwrap(),
        vec!["certificate/hecate/5f1c".to_string()]
    );
    assert_eq!(store.load_ca_material().unwrap(), vec![material(1, 1)]);
    let key = store.load_own_tls_key().unwrap().unwrap();
    assert_eq!(key.serial.as_str(), "5f1c");
    assert_eq!(key.private_key.expose(), b"pkcs8 of hecate");
}

#[test]
fn an_error_inside_the_transaction_leaves_nothing() {
    let fixture = Fixture::new();
    let result = fixture.store.transaction(&mut |writes| {
        writes.append("ca", &[record("CaCreated")], 0)?;
        writes.save_ca_material(&material(1, 1))?;
        writes.save_own_tls_key(&own_key("5f1c", b"pkcs8 of hecate"))?;
        Err(ServiceError::internal("PowerCut"))
    });
    assert!(result.is_err());
    assert!(fixture.store.load("ca").unwrap().is_empty());
    assert!(fixture.store.load_ca_material().unwrap().is_empty());
    assert!(fixture.store.load_own_tls_key().unwrap().is_none());
    assert_eq!(
        (
            fixture.count("events"),
            fixture.count("ca_material"),
            fixture.count("hecate_tls")
        ),
        (0, 0, 0)
    );
}

#[test]
fn appending_to_a_stream_that_moved_is_a_concurrency_conflict() {
    let fixture = Fixture::new();
    create_ca(&fixture.store).unwrap();
    let conflict = fixture
        .store
        .transaction(&mut |writes| {
            writes.append("ca", &[record("IntermediateRenewed")], 0)?;
            Ok(())
        })
        .unwrap_err();
    assert!(matches!(
        conflict,
        ServiceError::Concurrency(ConcurrencyConflict { current_version: 1 })
    ));
    let mut version = 0;
    fixture
        .store
        .transaction(&mut |writes| {
            assert_eq!(writes.load("ca")?.len(), 1);
            version = writes.append("ca", &[record("IntermediateRenewed")], 1)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(version, 2);
}

#[test]
fn the_key_of_an_intermediate_is_destroyed_and_hecate_key_is_replaced() {
    let fixture = Fixture::new();
    create_ca(&fixture.store).unwrap();
    fixture
        .store
        .transaction(&mut |writes| {
            writes.save_ca_material(&material(1, 2))?;
            writes.delete_ca_material(1, 1)?;
            writes.save_own_tls_key(&own_key("a2b3", b"new pkcs8"))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        fixture.store.load_ca_material().unwrap(),
        vec![material(1, 2)]
    );
    let key = fixture.store.load_own_tls_key().unwrap().unwrap();
    assert_eq!(key.serial.as_str(), "a2b3");
    assert_eq!(key.private_key.expose(), b"new pkcs8");
    assert_eq!(
        (fixture.count("ca_material"), fixture.count("hecate_tls")),
        (1, 1)
    );
}

/// SC-018: the ciphertext of a destroyed key is not left in the freed pages of the file.
#[test]
fn a_destroyed_key_does_not_stay_in_the_file() {
    let fixture = Fixture::new();
    let distinct = CaMaterial {
        key_ciphertext: b"a ciphertext that only the key of this intermediate has".to_vec(),
        ..material(1, 2)
    };
    fixture
        .store
        .transaction(&mut |writes| writes.save_ca_material(&distinct))
        .unwrap();
    let file = fixture.dir.path().join("hecate.db");
    let holds = |bytes: &[u8]| {
        std::fs::read(&file)
            .unwrap()
            .windows(bytes.len())
            .any(|window| window == bytes)
    };
    assert!(holds(&distinct.key_ciphertext));
    fixture
        .store
        .transaction(&mut |writes| writes.delete_ca_material(1, 2))
        .unwrap();
    assert!(!holds(&distinct.key_ciphertext));
}

#[test]
fn the_material_is_listed_by_generation_and_version() {
    let fixture = Fixture::new();
    fixture
        .store
        .transaction(&mut |writes| {
            writes.save_ca_material(&material(2, 1))?;
            writes.save_ca_material(&material(1, 2))?;
            writes.save_ca_material(&material(1, 1))?;
            Ok(())
        })
        .unwrap();
    let order: Vec<(u64, u64)> = fixture
        .store
        .load_ca_material()
        .unwrap()
        .iter()
        .map(|material| (material.generation, material.intermediate_version))
        .collect();
    assert_eq!(order, vec![(1, 1), (1, 2), (2, 1)]);
}
