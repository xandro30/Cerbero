//! SC-003 for secrets: with Hécate unsealed on SQLite, ten references resolve in under a second.
//! And T015 of feature 006: the first phase of a refusal of 100 references stays below a fifth
//! of the refusal floor of production (research R2).

mod common;

use std::sync::Arc;
use std::time::Instant;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::identity::service::Credentials;
use hecate_application::identity::settings::REFUSAL_FLOOR;
use hecate_application::testing::FakeClock;
use hecate_domain::sealing::errors::HecateError;
use hecate_infrastructure::testing::short_dir;
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

#[test]
fn the_first_phase_of_a_hundred_references_takes_less_than_a_fifth_of_the_floor() {
    let dir = short_dir();
    let clock = Arc::new(FakeClock::new());
    let stack = sqlite_with_ca(&dir.path().join("hecate.db"), clock.clone());
    let hermes = enrolled_on(&stack, &dir.path().join("hermes"), "hermes", clock);
    let mut references: Vec<String> = (0..99)
        .map(|n| format!("secret://hermes/app/key_{n}"))
        .collect();
    for path in &references {
        stack
            .service
            .create(path, "a value", "string", &no_options())
            .unwrap();
    }
    // The last reference fails in the costliest way: a secret without a resolvable version.
    let unresolvable = "secret://hermes/app/old";
    stack
        .service
        .create(unresolvable, "an old value", "string", &no_options())
        .unwrap();
    let expected = stack.service.get(unresolvable).unwrap().stream_version;
    stack
        .service
        .revoke(unresolvable, &json!(1), &json!(expected))
        .unwrap();
    references.push(unresolvable.to_string());
    let body = json!({ "references": references });
    let mut times = Vec::new();
    for _ in 0..20 {
        let credentials = Credentials {
            certificates: hermes
                .certificate_chain_of(None)
                .unwrap()
                .into_iter()
                .take(2)
                .collect(),
            bearer: Some(hermes.call_token(None).unwrap()),
        };
        let started = Instant::now();
        let refused = stack.identities.resolve(&credentials, &body);
        times.push(started.elapsed());
        assert!(matches!(
            refused,
            Err(ServiceError::Hecate(HecateError::NotFound(_)))
        ));
    }
    times.sort();
    let median = times[times.len() / 2];
    assert!(median < REFUSAL_FLOOR / 5, "{median:?} of {times:?}");
    stack.close();
}
