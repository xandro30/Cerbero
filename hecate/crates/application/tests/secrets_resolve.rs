mod common;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::testing::SecretsRig;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use serde_json::{Value, json};

const TOKEN: &str = "secret://hermes/telegram/bot_token";
const PROXY: &str = "secret://hermes/certs/proxy";
const OTHER: &str = "secret://echidna/api_key";
const TOKEN_VALUE: &str = "hermes-token-value";
const PROXY_VALUE: &str = "{\"cert\": \"value\"}";
const OTHER_VALUE: &str = "echidna-key-value";
const MISSING: &str = "secret://hermes/does/not/exist";

fn rig() -> SecretsRig {
    let rig = secrets();
    create(&rig, TOKEN, TOKEN_VALUE, "string");
    create(&rig, PROXY, PROXY_VALUE, "json");
    create(&rig, OTHER, OTHER_VALUE, "string");
    rig.access_log.clear();
    rig
}

fn triples(rig: &SecretsRig, references: &[&str]) -> Vec<(String, u64, String)> {
    resolve(rig, "hermes", references)
        .unwrap()
        .into_iter()
        .map(|item| (item.path.clone(), item.version, item.value.to_string()))
        .collect()
}

// py: unit/application/test_secret_resolve.py::test_resolves_the_active_version_with_the_cache_times
#[test]
fn resolves_the_active_version_with_the_cache_times() {
    let rig = rig();
    let out = resolve(&rig, "hermes", &[TOKEN]).unwrap();
    let [item] = out.as_slice() else {
        panic!("one secret")
    };
    assert_eq!(
        (item.path.as_str(), item.version, item.value.as_str()),
        (TOKEN, 1, TOKEN_VALUE)
    );
    assert_eq!((item.cache_ttl_soft_s, item.cache_ttl_hard_s), (300, 3600));
}

// py: unit/application/test_secret_resolve.py::test_resolves_a_concrete_version
#[test]
fn resolves_a_concrete_version() {
    let rig = rig();
    assert_eq!(
        triples(&rig, &[&format!("{TOKEN}@1")]),
        [(TOKEN.into(), 1, TOKEN_VALUE.into())]
    );
}

// py: unit/application/test_secret_resolve.py::test_a_version_that_does_not_exist_is_not_found
#[test]
fn a_version_that_does_not_exist_is_not_found() {
    let rig = rig();
    let error = resolve(&rig, "hermes", &[&format!("{TOKEN}@2")])
        .err()
        .unwrap();
    assert!(is_not_found(&error));
}

// py: unit/application/test_secret_resolve.py::test_resolves_several_references_in_one_request_in_order
#[test]
fn resolves_several_references_in_one_request_in_order() {
    let rig = rig();
    assert_eq!(
        triples(&rig, &[PROXY, TOKEN, &format!("{PROXY}@1")]),
        [
            (PROXY.into(), 1, PROXY_VALUE.into()),
            (TOKEN.into(), 1, TOKEN_VALUE.into()),
            (PROXY.into(), 1, PROXY_VALUE.into()),
        ]
    );
}

// py: unit/application/test_secret_resolve.py::test_all_or_nothing
#[test]
fn all_or_nothing() {
    let rig = rig();
    let error = resolve(&rig, "hermes", &[TOKEN, MISSING]).err().unwrap();
    assert!(is_not_found(&error) && !error.to_string().contains(TOKEN_VALUE));
}

// py: unit/application/test_secret_resolve.py::test_another_identitys_secret_and_a_missing_one_are_indistinguishable
#[test]
fn another_identitys_secret_and_a_missing_one_are_indistinguishable() {
    let rig = rig();
    let refusals: Vec<ServiceError> = [
        OTHER.to_string(),
        MISSING.to_string(),
        format!("{OTHER}@1"),
        "secret://hermes/does/not/exist@1".to_string(),
    ]
    .iter()
    .map(|reference| resolve(&rig, "hermes", &[reference]).err().unwrap())
    .collect();
    let mut messages: Vec<String> = refusals.iter().map(ToString::to_string).collect();
    messages.dedup();
    assert_eq!(messages.len(), 1);
    for error in &refusals {
        let ServiceError::Hecate(error @ HecateError::NotFound(_)) = error else {
            panic!("{error:?}")
        };
        assert!(error.code() == "not_found" && !error.retryable());
    }
    assert!(
        ![OTHER, MISSING, OTHER_VALUE]
            .iter()
            .any(|text| messages[0].contains(text))
    );
}

// py: unit/application/test_secret_resolve.py::test_an_identity_never_resolves_outside_its_own_space
#[test]
fn an_identity_never_resolves_outside_its_own_space() {
    let rig = rig();
    create(&rig, "secret://hermes-eu/x", "eu-value", "string");
    assert!(is_not_found(
        &resolve(&rig, "hermes", &["secret://hermes-eu/x"])
            .err()
            .unwrap()
    ));
    assert!(is_not_found(
        &resolve(&rig, "hermes-eu", &[TOKEN]).err().unwrap()
    ));
    let out = resolve(&rig, "hermes-eu", &["secret://hermes-eu/x"]).unwrap();
    assert_eq!(out[0].value.as_str(), "eu-value");
}

// py: unit/application/test_secret_resolve.py::test_malformed_requests_are_invalid
#[test]
fn malformed_requests_are_invalid() {
    let rig = rig();
    let too_many: Vec<&str> = vec![TOKEN; 101];
    for references in [
        json!([]),
        json!("secret://hermes/x"),
        Value::Null,
        json!([5]),
        json!(["not a reference"]),
        json!([format!("{TOKEN}@0")]),
        json!(too_many),
    ] {
        let error = rig.service.resolve("hermes", &references).err().unwrap();
        assert!(is_invalid(&error), "{references}");
    }
}

// py: unit/application/test_secret_resolve.py::test_a_malformed_identity_is_invalid
#[test]
fn a_malformed_identity_is_invalid() {
    let rig = rig();
    for identity in ["", "Hermes", "he/rmes"] {
        assert!(
            is_invalid(&resolve(&rig, identity, &[TOKEN]).err().unwrap()),
            "{identity:?}"
        );
    }
}

// py: unit/application/test_secret_resolve.py::test_resolving_while_sealed_conflicts
#[test]
fn resolving_while_sealed_conflicts() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    assert!(is_conflict(
        &resolve(&rig, "hermes", &[TOKEN]).err().unwrap()
    ));
}

// py: unit/application/test_secret_resolve.py::test_every_resolution_is_logged_with_the_identity_as_subject_and_no_value
#[test]
fn every_resolution_is_logged_with_the_identity_as_subject_and_no_value() {
    let rig = rig();
    resolve(&rig, "hermes", &[TOKEN, PROXY]).unwrap();
    let rows = rig.secret_rows();
    let [first, second] = rows.as_slice() else {
        panic!("two rows: {rows:?}")
    };
    for (row, path) in [(first, TOKEN), (second, PROXY)] {
        assert_eq!(
            (
                row.operation,
                row.subject.as_deref(),
                row.method,
                row.result
            ),
            (
                Operation::SecretResolve,
                Some("hermes"),
                Method::Identity,
                AccessResult::Success
            )
        );
        assert_eq!((row.path.as_deref(), row.version), (Some(path), Some(1)));
    }
    let shown = format!("{:?}", rig.access_log.entries());
    let escaped = format!("{PROXY_VALUE:?}");
    assert!(
        !shown.contains(TOKEN_VALUE)
            && !shown.contains(PROXY_VALUE)
            && !shown.contains(&escaped[1..escaped.len() - 1])
    );
}

// py: unit/application/test_secret_resolve.py::test_a_rejection_is_logged_with_the_requested_path_and_logs_no_success
#[test]
fn a_rejection_is_logged_with_the_requested_path_and_logs_no_success() {
    let rig = rig();
    assert!(is_not_found(
        &resolve(&rig, "hermes", &[TOKEN, OTHER]).err().unwrap()
    ));
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (
            row.operation,
            row.subject.as_deref(),
            row.result,
            row.path.as_deref()
        ),
        (
            Operation::SecretResolve,
            Some("hermes"),
            AccessResult::NotFound,
            Some(OTHER)
        )
    );
}

// py: unit/application/test_secret_resolve.py::test_rejections_are_logged_the_same_for_foreign_and_missing_secrets
#[test]
fn rejections_are_logged_the_same_for_foreign_and_missing_secrets() {
    let rig = rig();
    for path in [OTHER, MISSING] {
        assert!(is_not_found(
            &resolve(&rig, "hermes", &[path]).err().unwrap()
        ));
    }
    let rows = rig.secret_rows();
    let [foreign, missing] = rows.as_slice() else {
        panic!("two rows: {rows:?}")
    };
    assert_eq!(
        (foreign.result, foreign.version),
        (missing.result, missing.version)
    );
}

// py: unit/application/test_secret_resolve.py::test_a_sealed_refusal_is_logged_too
#[test]
fn a_sealed_refusal_is_logged_too() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    assert!(is_conflict(
        &resolve(&rig, "hermes", &[TOKEN]).err().unwrap()
    ));
    assert_eq!(
        rig.secret_rows().last().unwrap().result,
        AccessResult::RejectedConflict
    );
}

// py: unit/application/test_secret_resolve.py::test_what_is_resolved_is_exactly_what_was_written
#[test]
fn what_is_resolved_is_exactly_what_was_written() {
    let rig = rig();
    create(
        &rig,
        "secret://hermes/unicode",
        "pässwörd €\n\ttabs",
        "string",
    );
    assert_eq!(
        resolve_one(&rig, "secret://hermes/unicode").value.as_str(),
        "pässwörd €\n\ttabs"
    );
}
