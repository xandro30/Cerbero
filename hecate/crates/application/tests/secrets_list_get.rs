mod common;

use common::*;
use hecate_application::secrets::service::{CreateOptions, SecretSummary};
use hecate_application::testing::SecretsRig;
use hecate_domain::instant::{isoformat, parse_iso};
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::secrets::value_type::ValueType;
use serde_json::{Value, json};

const TOKEN: &str = "secret://hermes/telegram/bot_token";
const PROXY: &str = "secret://hermes/certs/proxy";
const ECHIDNA: &str = "secret://echidna/api_key";
const VALUE: &str = "the-value-that-must-never-be-shown";

fn rig() -> SecretsRig {
    let rig = secrets();
    let options = CreateOptions {
        rotate_every: json!("90d"),
        expires_at: json!("2027-06-01T00:00:00+00:00"),
        ..CreateOptions::default()
    };
    rig.service
        .create(TOKEN, VALUE, "string", &options)
        .unwrap();
    create(&rig, PROXY, "{\"k\": \"v\"}", "json");
    create(&rig, ECHIDNA, VALUE, "string");
    rig
}

fn paths(items: &[SecretSummary]) -> Vec<&str> {
    items.iter().map(|item| item.path.as_str()).collect()
}

// py: unit/application/test_secret_list_get.py::test_list_returns_paths_and_metadata_sorted
#[test]
fn list_returns_paths_and_metadata_sorted() {
    let rig = rig();
    let items = rig.service.list_secrets(&Value::Null).unwrap();
    assert_eq!(paths(&items), [ECHIDNA, PROXY, TOKEN]);
    assert_eq!(
        items[2],
        SecretSummary {
            path: TOKEN.into(),
            value_type: ValueType::String,
            active_version: Some(1),
            version_count: 1,
            rotate_every: Some("90d".into()),
            expires_at: Some(parse_iso("2027-06-01T00:00:00+00:00").unwrap()),
        }
    );
    assert_eq!(
        items[2].expires_at.map(isoformat).as_deref(),
        Some("2027-06-01T00:00:00+00:00")
    );
    assert!(items[1].rotate_every.is_none() && items[1].expires_at.is_none());
}

// py: unit/application/test_secret_list_get.py::test_list_filters_by_identity
#[test]
fn list_filters_by_identity() {
    let rig = rig();
    assert_eq!(
        paths(&rig.service.list_secrets(&json!("hermes")).unwrap()),
        [PROXY, TOKEN]
    );
    assert_eq!(
        paths(&rig.service.list_secrets(&json!("echidna")).unwrap()),
        [ECHIDNA]
    );
    assert!(
        rig.service
            .list_secrets(&json!("nobody"))
            .unwrap()
            .is_empty()
    );
}

// py: unit/application/test_secret_list_get.py::test_list_does_not_treat_a_prefix_of_an_identity_as_the_identity
#[test]
fn list_does_not_treat_a_prefix_of_an_identity_as_the_identity() {
    let rig = rig();
    create(&rig, "secret://hermes-eu/x", VALUE, "string");
    assert_eq!(
        paths(&rig.service.list_secrets(&json!("hermes")).unwrap()),
        [PROXY, TOKEN]
    );
}

// py: unit/application/test_secret_list_get.py::test_list_rejects_a_malformed_identity
#[test]
fn list_rejects_a_malformed_identity() {
    let rig = rig();
    for identity in [
        json!("Hermes"),
        json!("1abc"),
        json!("he/rmes"),
        json!(""),
        json!(5),
    ] {
        assert!(
            is_invalid(&rig.service.list_secrets(&identity).unwrap_err()),
            "{identity}"
        );
    }
}

// py: unit/application/test_secret_list_get.py::test_get_returns_metadata_stream_version_and_versions
#[test]
fn get_returns_metadata_stream_version_and_versions() {
    let rig = rig();
    let got = rig.service.get(TOKEN).unwrap();
    assert!(got.path == TOKEN && got.owner == "hermes" && got.value_type == ValueType::String);
    assert_eq!(
        (got.max_versions, got.rotate_every.as_deref()),
        (10, Some("90d"))
    );
    assert_eq!(
        got.expires_at.map(isoformat).as_deref(),
        Some("2027-06-01T00:00:00+00:00")
    );
    assert_eq!(got.stream_version, 2);
    let [version] = got.versions.as_slice() else {
        panic!("one version")
    };
    assert_eq!(
        (
            version.version,
            version.state.as_str(),
            version.deprecated_at
        ),
        (1, "active", None)
    );
    assert_eq!(
        isoformat(version.created_at),
        isoformat(hecate_application::ports::Clock::now(&*rig.clock))
    );
    assert_eq!(version.fingerprint.len(), 19);
}

// py: unit/application/test_secret_list_get.py::test_no_response_contains_the_value
#[test]
fn no_response_contains_the_value() {
    let rig = rig();
    let dumped = format!(
        "{:?} {:?} {:?}",
        rig.service.list_secrets(&Value::Null).unwrap(),
        rig.service.get(TOKEN).unwrap(),
        rig.service.get(PROXY).unwrap()
    );
    assert!(!dumped.contains(VALUE));
    assert!(!dumped.contains(r#"\"k\""#) && !dumped.contains(r#""k""#));
}

// py: unit/application/test_secret_list_get.py::test_unknown_path_is_not_found
#[test]
fn unknown_path_is_not_found() {
    assert!(is_not_found(
        &rig().service.get("secret://hermes/missing").unwrap_err()
    ));
}

// py: unit/application/test_secret_list_get.py::test_a_malformed_path_is_invalid
#[test]
fn a_malformed_path_is_invalid() {
    assert!(is_invalid(&rig().service.get("nonsense").unwrap_err()));
}

// py: unit/application/test_secret_list_get.py::test_sealed_hecate_refuses_both
#[test]
fn sealed_hecate_refuses_both() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    assert!(is_conflict(
        &rig.service.list_secrets(&Value::Null).unwrap_err()
    ));
    assert!(is_conflict(&rig.service.get(TOKEN).unwrap_err()));
}

// py: unit/application/test_secret_list_get.py::test_every_consultation_is_logged_without_the_value
#[test]
fn every_consultation_is_logged_without_the_value() {
    let rig = rig();
    rig.access_log.clear();
    rig.service.list_secrets(&json!("hermes")).unwrap();
    rig.service.get(TOKEN).unwrap();
    rig.service.get("secret://hermes/missing").unwrap_err();
    let rows = rig.secret_rows();
    let [listed, got, missing] = rows.as_slice() else {
        panic!("three rows: {rows:?}")
    };
    assert_eq!(
        (listed.operation, listed.subject.as_deref(), listed.result),
        (Operation::SecretList, Some("admin"), AccessResult::Success)
    );
    assert_eq!(
        (got.operation, got.path.as_deref(), got.result),
        (Operation::SecretGet, Some(TOKEN), AccessResult::Success)
    );
    assert_eq!(
        (missing.operation, missing.path.as_deref(), missing.result),
        (
            Operation::SecretGet,
            Some("secret://hermes/missing"),
            AccessResult::NotFound
        )
    );
    assert!(!format!("{:?}", rig.access_log.entries()).contains(VALUE));
}
