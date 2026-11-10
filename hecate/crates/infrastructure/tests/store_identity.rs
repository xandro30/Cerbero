//! T061: the store of the identities in SQLite: the enrollment throttle in `enroll_attempts`,
//! which survives reopening the database, and the streams of identities and certificates listed
//! by prefix.

use hecate_application::ports::{
    EnrollAttempts, EnrollAttemptsRepository, EventRecord, EventStore, IdentityStore,
};
use hecate_infrastructure::store::ca::SqliteIdentityStore;
use hecate_infrastructure::store::identity::SqliteEnrollAttempts;
use serde_json::json;
use time::macros::datetime;

#[test]
fn the_throttle_survives_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let attempts = SqliteEnrollAttempts::open(&path).unwrap();
    assert_eq!(attempts.load().unwrap(), EnrollAttempts::default());
    let throttled = EnrollAttempts {
        consecutive_failures: 11,
        wait_until: Some(datetime!(2026-11-09 12:02:00.25 UTC)),
        rejected_during_wait: 3,
    };
    attempts.save(&throttled).unwrap();
    drop(attempts);
    let reopened = SqliteEnrollAttempts::open(&path).unwrap();
    assert_eq!(reopened.load().unwrap(), throttled);
    reopened.save(&EnrollAttempts::default()).unwrap();
    assert_eq!(reopened.load().unwrap(), EnrollAttempts::default());
}

#[test]
fn identity_and_certificate_streams_are_listed_by_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteIdentityStore::open(&dir.path().join("hecate.db")).unwrap();
    let record = EventRecord {
        kind: "EnrollmentTokenCreated".into(),
        data: json!({}).as_object().unwrap().clone(),
        at: datetime!(2026-11-09 12:00 UTC),
    };
    store
        .transaction(&mut |writes| {
            for stream in [
                "identity/hermes",
                "identity/hades",
                "certificate/hermes/aa",
                "certificate/hermes/bb",
                "certificate/hades/cc",
                "ca",
            ] {
                writes.append(stream, std::slice::from_ref(&record), 0)?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.streams("identity/").unwrap(),
        ["identity/hades", "identity/hermes"]
    );
    assert_eq!(
        store.streams("certificate/hermes/").unwrap(),
        ["certificate/hermes/aa", "certificate/hermes/bb"]
    );
}
