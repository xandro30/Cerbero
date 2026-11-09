//! SC-003 for secrets: with Hécate unsealed on SQLite, ten references resolve in under a second.

mod common;

use std::time::Instant;

use common::*;
use serde_json::json;

// py: integration/test_secret_resolve_sqlite.py::test_resolving_ten_references_takes_less_than_a_second
#[test]
fn resolving_ten_references_takes_less_than_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let stack = ready(&dir.path().join("hecate.db"));
    let paths: Vec<String> = (0..10)
        .map(|n| format!("secret://hermes/service/key_{n}"))
        .collect();
    for (n, path) in paths.iter().enumerate() {
        stack
            .service
            .create(
                path,
                &format!("value-number-{n}-xyz"),
                "string",
                &no_options(),
            )
            .unwrap();
    }
    let started = Instant::now();
    let resolved = stack.service.resolve("hermes", &json!(paths)).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(resolved.len(), 10);
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "10 references took {elapsed:?}"
    );
    stack.close();
}
