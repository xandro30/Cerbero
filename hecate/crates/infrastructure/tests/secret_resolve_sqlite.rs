mod common;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_domain::sealing::errors::HecateError;
use serde_json::json;

fn values() -> Vec<(String, String)> {
    (0..10)
        .map(|n| {
            (
                format!("secret://hermes/service/key_{n}"),
                format!("value-number-{n}-xyz"),
            )
        })
        .collect()
}

fn paths() -> serde_json::Value {
    json!(
        values()
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>()
    )
}

// py: integration/test_secret_resolve_sqlite.py::test_store_restart_unseal_and_resolve
#[test]
fn store_restart_unseal_and_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let first = ready(&db);
    for (path, value) in values() {
        first
            .service
            .create(&path, &value, "string", &no_options())
            .unwrap();
    }
    first
        .service
        .create(
            "secret://echidna/key",
            "echidna-value",
            "string",
            &no_options(),
        )
        .unwrap();
    first.close();

    let second = stack(&db);
    let error = second.service.resolve("hermes", &paths()).err().unwrap();
    assert!(matches!(
        error,
        ServiceError::Hecate(HecateError::StateConflict { .. })
    ));
    second
        .sealing
        .unseal_with_passphrase(hecate_application::testing::GOOD_PASSPHRASE)
        .unwrap();
    let resolved: Vec<(String, String)> = second
        .service
        .resolve("hermes", &paths())
        .unwrap()
        .into_iter()
        .map(|item| (item.path.clone(), item.value.to_string()))
        .collect();
    assert_eq!(resolved, values());
    let foreign = second
        .service
        .resolve("hermes", &json!(["secret://echidna/key"]))
        .err()
        .unwrap();
    assert!(matches!(
        foreign,
        ServiceError::Hecate(HecateError::NotFound(_))
    ));
    let other = second
        .service
        .resolve("echidna", &json!(["secret://echidna/key"]))
        .unwrap();
    assert_eq!(other[0].value.as_str(), "echidna-value");
    second.close();
}

// py: integration/test_secret_resolve_sqlite.py::test_the_access_log_keeps_the_resolutions_across_restarts
#[test]
fn the_access_log_keeps_the_resolutions_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let stack = ready(&dir.path().join("hecate.db"));
    stack
        .service
        .create("secret://hermes/a", "value-a-xyz", "string", &no_options())
        .unwrap();
    stack
        .service
        .resolve("hermes", &json!(["secret://hermes/a"]))
        .unwrap();
    let rows: Vec<_> = stack
        .access_log
        .entries()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.operation.as_str() == "secret_resolve")
        .map(|entry| (entry.subject, entry.path, entry.version))
        .collect();
    assert_eq!(
        rows,
        [(
            Some("hermes".into()),
            Some("secret://hermes/a".into()),
            Some(1)
        )]
    );
    stack.close();
}
