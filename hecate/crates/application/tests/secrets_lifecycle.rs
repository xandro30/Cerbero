mod common;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::testing::SecretsRig;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

const PATH: &str = "secret://hermes/telegram/bot_token";
const V1: &str = "first-value-111";
const V2: &str = "second-value-222";

/// Version 1 deprecated, version 2 active.
fn rig() -> SecretsRig {
    let rig = secrets();
    create(&rig, PATH, V1, "string");
    rig.service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    rig.access_log.clear();
    rig
}

fn act(
    rig: &SecretsRig,
    action: &str,
    path: &str,
    version: &Value,
    expected: &Value,
) -> Result<u64, ServiceError> {
    match action {
        "revoke" => rig.service.revoke(path, version, expected),
        "restore" => rig.service.restore(path, version, expected),
        "promote" => rig.service.promote(path, version, expected),
        _ => unreachable!(),
    }
}

fn now(rig: &SecretsRig) -> Value {
    current(rig, PATH)
}

fn at(version: u64) -> String {
    format!("{PATH}@{version}")
}

// py: unit/application/test_secret_lifecycle.py::test_revoking_the_active_version_stops_it_at_once_and_leaves_none_active
#[test]
fn revoking_the_active_version_stops_it_at_once_and_leaves_none_active() {
    let rig = rig();
    let out = rig.service.revoke(PATH, &json!(2), &now(&rig)).unwrap();
    assert_eq!(json!(out), now(&rig));
    assert_eq!(states(&rig, PATH), [(1, "deprecated"), (2, "revoked")]);
    assert!(!can_resolve(&rig, &at(2)));
    assert!(
        !can_resolve(&rig, PATH),
        "without an active version the resolution is refused"
    );
    assert!(can_resolve(&rig, &at(1)));
}

// py: unit/application/test_secret_lifecycle.py::test_revoking_a_deprecated_version_stops_it_before_its_grace_ends
#[test]
fn revoking_a_deprecated_version_stops_it_before_its_grace_ends() {
    let rig = rig();
    assert!(can_resolve(&rig, &at(1)));
    rig.service.revoke(PATH, &json!(1), &now(&rig)).unwrap();
    assert_eq!(state_of(&rig, PATH, 1), "revoked");
    assert!(!can_resolve(&rig, &at(1)));
    assert!(can_resolve(&rig, PATH));
}

// py: unit/application/test_secret_lifecycle.py::test_restoring_a_revoked_version_leaves_it_deprecated_with_a_new_grace
#[test]
fn restoring_a_revoked_version_leaves_it_deprecated_with_a_new_grace() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(1), &now(&rig)).unwrap();
    rig.clock.advance(Duration::hours(30));
    rig.service.restore(PATH, &json!(1), &now(&rig)).unwrap();
    assert_eq!(state_of(&rig, PATH, 1), "deprecated");
    assert!(can_resolve(&rig, &at(1)));
    rig.clock.advance(Duration::hours(23));
    assert!(can_resolve(&rig, &at(1)));
    rig.clock.advance(Duration::hours(1));
    assert!(!can_resolve(&rig, &at(1)));
    assert_eq!(state_of(&rig, PATH, 1), "revoked");
}

// py: unit/application/test_secret_lifecycle.py::test_an_expired_deprecated_version_can_be_restored
#[test]
fn an_expired_deprecated_version_can_be_restored() {
    let rig = rig();
    rig.clock.advance(Duration::hours(25));
    assert_eq!(state_of(&rig, PATH, 1), "revoked");
    rig.service.restore(PATH, &json!(1), &now(&rig)).unwrap();
    assert!(state_of(&rig, PATH, 1) == "deprecated" && can_resolve(&rig, &at(1)));
}

// py: unit/application/test_secret_lifecycle.py::test_promoting_a_deprecated_version_makes_it_active_and_demotes_the_active_one
#[test]
fn promoting_a_deprecated_version_makes_it_active_and_demotes_the_active_one() {
    let rig = rig();
    rig.service.promote(PATH, &json!(1), &now(&rig)).unwrap();
    assert_eq!(states(&rig, PATH), [(1, "active"), (2, "deprecated")]);
    assert_eq!(resolve_one(&rig, PATH).value.as_str(), V1);
    assert!(can_resolve(&rig, &at(2)));
}

// py: unit/application/test_secret_lifecycle.py::test_promoting_when_nothing_is_active_only_promotes
#[test]
fn promoting_when_nothing_is_active_only_promotes() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(2), &now(&rig)).unwrap();
    rig.service.promote(PATH, &json!(1), &now(&rig)).unwrap();
    assert_eq!(states(&rig, PATH), [(1, "active"), (2, "revoked")]);
}

// py: unit/application/test_secret_lifecycle.py::test_a_new_version_after_revoking_the_active_one_becomes_active
#[test]
fn a_new_version_after_revoking_the_active_one_becomes_active() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(2), &now(&rig)).unwrap();
    rig.service
        .add_version(PATH, "third-value-333", &now(&rig))
        .unwrap();
    assert_eq!(
        states(&rig, PATH),
        [(1, "deprecated"), (2, "revoked"), (3, "active")]
    );
    assert_eq!(resolve_one(&rig, PATH).version, 3);
}

// py: unit/application/test_secret_lifecycle.py::test_transitions_that_are_not_allowed_conflict_and_change_nothing
#[test]
fn transitions_that_are_not_allowed_conflict_and_change_nothing() {
    for (action, version) in [
        ("revoke", 2),
        ("restore", 2),
        ("restore", 1),
        ("promote", 2),
    ] {
        let rig = rig();
        if action == "revoke" {
            rig.service.revoke(PATH, &json!(2), &now(&rig)).unwrap();
        }
        let before = now(&rig);
        let error = act(&rig, action, PATH, &json!(version), &before).unwrap_err();
        assert!(
            is_conflict(&error) && conflict_state(&error).is_none(),
            "{action} {version}"
        );
        assert!(error.to_string().contains("version"));
        assert_eq!(now(&rig), before);
    }
}

// py: unit/application/test_secret_lifecycle.py::test_a_revoked_version_cannot_be_promoted
#[test]
fn a_revoked_version_cannot_be_promoted() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(1), &now(&rig)).unwrap();
    assert!(is_conflict(
        &rig.service
            .promote(PATH, &json!(1), &now(&rig))
            .unwrap_err()
    ));
}

// py: unit/application/test_secret_lifecycle.py::test_an_unknown_version_or_path_is_not_found
#[test]
fn an_unknown_version_or_path_is_not_found() {
    for action in ["revoke", "restore", "promote"] {
        let rig = rig();
        assert!(
            is_not_found(&act(&rig, action, PATH, &json!(9), &now(&rig)).unwrap_err()),
            "{action}"
        );
        let missing = act(
            &rig,
            action,
            "secret://hermes/missing",
            &json!(1),
            &json!(1),
        );
        assert!(is_not_found(&missing.unwrap_err()), "{action}");
    }
}

// py: unit/application/test_secret_lifecycle.py::test_a_stale_or_missing_expected_version_is_refused
#[test]
fn a_stale_or_missing_expected_version_is_refused() {
    for action in ["revoke", "restore", "promote"] {
        let rig = rig();
        let now_version = now(&rig).as_u64().unwrap();
        let error = act(&rig, action, PATH, &json!(1), &json!(now_version - 1)).unwrap_err();
        assert_eq!(current_version(&error), Some(now_version), "{action}");
        assert!(is_invalid(
            &act(&rig, action, PATH, &json!(1), &Value::Null).unwrap_err()
        ));
        assert_eq!(now(&rig), json!(now_version));
    }
}

// py: unit/application/test_secret_lifecycle.py::test_a_malformed_version_is_invalid
#[test]
fn a_malformed_version_is_invalid() {
    let rig = rig();
    for version in [
        Value::Null,
        json!("1"),
        json!(0),
        json!(-1),
        json!(true),
        json!(1.5),
    ] {
        assert!(
            is_invalid(&rig.service.revoke(PATH, &version, &now(&rig)).unwrap_err()),
            "{version}"
        );
    }
}

// py: unit/application/test_secret_lifecycle.py::test_none_of_these_operations_asks_for_a_proof_or_touches_the_attempt_counter
#[test]
fn none_of_these_operations_asks_for_a_proof_or_touches_the_attempt_counter() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(1), &now(&rig)).unwrap();
    rig.service.restore(PATH, &json!(1), &now(&rig)).unwrap();
    rig.service.promote(PATH, &json!(1), &now(&rig)).unwrap();
    assert_eq!(failures(&rig), 0);
}

// py: unit/application/test_secret_lifecycle.py::test_each_operation_is_logged_with_its_version
#[test]
fn each_operation_is_logged_with_its_version() {
    let rig = rig();
    rig.service.revoke(PATH, &json!(1), &now(&rig)).unwrap();
    rig.service.restore(PATH, &json!(1), &now(&rig)).unwrap();
    rig.service.promote(PATH, &json!(1), &now(&rig)).unwrap();
    assert!(is_conflict(
        &rig.service
            .restore(PATH, &json!(1), &now(&rig))
            .unwrap_err()
    ));
    let rows: Vec<_> = rig
        .secret_rows()
        .into_iter()
        .map(|row| {
            (
                row.operation,
                row.subject,
                row.path,
                row.version,
                row.result,
            )
        })
        .collect();
    let row = |operation, result| {
        (
            operation,
            Some("admin".to_string()),
            Some(PATH.to_string()),
            Some(1),
            result,
        )
    };
    assert_eq!(
        rows,
        [
            row(Operation::SecretRevoke, AccessResult::Success),
            row(Operation::SecretRestore, AccessResult::Success),
            row(Operation::SecretPromote, AccessResult::Success),
            row(Operation::SecretRestore, AccessResult::RejectedConflict),
        ]
    );
}

// py: unit/application/test_secret_lifecycle.py::test_sealed_hecate_refuses_all_three
#[test]
fn sealed_hecate_refuses_all_three() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    for action in ["revoke", "restore", "promote"] {
        let error = act(&rig, action, PATH, &json!(1), &json!(4)).unwrap_err();
        assert_eq!(conflict_state(&error), Some(SealState::Sealed), "{action}");
    }
}
