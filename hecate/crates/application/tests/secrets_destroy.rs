mod common;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, SecretsRig};
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

const PATH: &str = "secret://hermes/telegram/bot_token";
const V1: &str = "first-value-111";
const V2: &str = "second-value-222";

fn wrong() -> Value {
    json!({"passphrase": "wrong wrong wrong"})
}

fn right() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

/// Version 1 revoked, version 2 active.
fn rig() -> SecretsRig {
    let rig = secrets();
    create(&rig, PATH, V1, "string");
    rig.service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    rig.service
        .revoke(PATH, &json!(1), &current(&rig, PATH))
        .unwrap();
    rig.access_log.clear();
    rig
}

fn now(rig: &SecretsRig) -> Value {
    current(rig, PATH)
}

fn destroy(
    rig: &SecretsRig,
    version: u64,
    proof: &Value,
) -> Result<u64, hecate_application::errors::ServiceError> {
    rig.service.destroy(PATH, &json!(version), &now(rig), proof)
}

// py: unit/application/test_secret_destroy.py::test_destroying_with_the_passphrase_removes_the_material_and_keeps_the_events
#[test]
fn destroying_with_the_passphrase_removes_the_material_and_keeps_the_events() {
    let rig = rig();
    assert!(has_material(&rig, PATH, 1));
    let before = now(&rig).as_u64().unwrap();
    assert_eq!(destroy(&rig, 1, &right()).unwrap(), before + 1);
    assert_eq!(states(&rig, PATH), [(1, "destroyed"), (2, "active")]);
    assert!(!has_material(&rig, PATH, 1));
    assert!(has_material(&rig, PATH, 2));
    let records = events(&rig, "secret/hermes/telegram/bot_token");
    let last = records.last().unwrap();
    assert_eq!(last.kind, "VersionDestroyed");
    assert_eq!(
        serde_json::Value::Object(last.data.clone()),
        json!({"version": 1, "reason": "admin"})
    );
    assert!(
        records
            .iter()
            .any(|record| record.kind == "VersionAdded" && record.data["version"] == 1),
        "history stays"
    );
}

// py: unit/application/test_secret_destroy.py::test_destroying_with_two_recovery_shares
#[test]
fn destroying_with_two_recovery_shares() {
    let rig = rig();
    destroy(&rig, 1, &json!({"shares": &rig.shares[1..]})).unwrap();
    assert_eq!(state_of(&rig, PATH, 1), "destroyed");
}

// py: unit/application/test_secret_destroy.py::test_a_destroyed_version_is_not_resolved_nor_restored
#[test]
fn a_destroyed_version_is_not_resolved_nor_restored() {
    let rig = rig();
    destroy(&rig, 1, &right()).unwrap();
    assert!(!can_resolve(&rig, &format!("{PATH}@1")));
    assert!(is_conflict(
        &rig.service
            .restore(PATH, &json!(1), &now(&rig))
            .unwrap_err()
    ));
    assert!(is_conflict(&destroy(&rig, 1, &right()).unwrap_err()));
    assert_eq!(resolve_one(&rig, PATH).value.as_str(), V2);
}

// py: unit/application/test_secret_destroy.py::test_a_wrong_proof_is_refused_and_counts_in_the_shared_counter
#[test]
fn a_wrong_proof_is_refused_and_counts_in_the_shared_counter() {
    let rig = rig();
    let before = now(&rig);
    assert!(is_auth(&destroy(&rig, 1, &wrong()).unwrap_err()));
    assert!(state_of(&rig, PATH, 1) == "revoked" && now(&rig) == before);
    assert!(has_material(&rig, PATH, 1));
    assert_eq!(failures(&rig), 1);
}

// py: unit/application/test_secret_destroy.py::test_wrong_shares_are_refused_too
#[test]
fn wrong_shares_are_refused_too() {
    let rig = rig();
    let other = secrets().shares;
    assert!(is_auth(
        &destroy(&rig, 1, &json!({"shares": &other[..2]})).unwrap_err()
    ));
    assert_eq!(state_of(&rig, PATH, 1), "revoked");
}

// py: unit/application/test_secret_destroy.py::test_during_a_wait_even_the_right_proof_is_rate_limited
#[test]
fn during_a_wait_even_the_right_proof_is_rate_limited() {
    let rig = rig();
    for _ in 0..3 {
        assert!(is_auth(&destroy(&rig, 1, &wrong()).unwrap_err()));
    }
    assert_eq!(
        retry_after(&destroy(&rig, 1, &right()).unwrap_err()),
        Some(60)
    );
    assert_eq!(state_of(&rig, PATH, 1), "revoked");
    rig.clock.advance(Duration::seconds(61));
    destroy(&rig, 1, &right()).unwrap();
    assert_eq!(state_of(&rig, PATH, 1), "destroyed");
}

// py: unit/application/test_secret_destroy.py::test_the_counter_is_shared_with_unsealing
#[test]
fn the_counter_is_shared_with_unsealing() {
    let rig = rig();
    for _ in 0..2 {
        assert!(is_auth(&destroy(&rig, 1, &wrong()).unwrap_err()));
    }
    rig.sealing.seal().unwrap();
    assert!(is_auth(
        &rig.sealing
            .unseal_with_passphrase("wrong wrong wrong")
            .unwrap_err()
    ));
    assert_eq!(failures(&rig), 3);
}

// py: unit/application/test_secret_destroy.py::test_only_a_revoked_version_can_be_destroyed
#[test]
fn only_a_revoked_version_can_be_destroyed() {
    let rig = rig();
    assert!(is_conflict(&destroy(&rig, 2, &wrong()).unwrap_err()));
    assert_eq!(state_of(&rig, PATH, 2), "active");
    assert_eq!(failures(&rig), 0, "the state is checked before the proof");
}

// py: unit/application/test_secret_destroy.py::test_a_deprecated_version_is_not_destroyable_until_its_grace_ends
#[test]
fn a_deprecated_version_is_not_destroyable_until_its_grace_ends() {
    let rig = rig();
    rig.service.restore(PATH, &json!(1), &now(&rig)).unwrap();
    assert!(is_conflict(&destroy(&rig, 1, &right()).unwrap_err()));
    rig.clock.advance(Duration::hours(24));
    destroy(&rig, 1, &right()).unwrap();
    assert_eq!(state_of(&rig, PATH, 1), "destroyed");
}

// py: unit/application/test_secret_destroy.py::test_a_malformed_proof_is_invalid_without_counting
#[test]
fn a_malformed_proof_is_invalid_without_counting() {
    for proof in [
        Value::Null,
        json!({}),
        json!("text"),
        json!({"passphrase": ""}),
        json!({"passphrase": "x", "shares": []}),
        json!({"shares": "AB"}),
        json!({"shares": []}),
    ] {
        let rig = rig();
        assert!(
            is_invalid(&destroy(&rig, 1, &proof).unwrap_err()),
            "{proof}"
        );
        assert_eq!(failures(&rig), 0);
    }
}

// py: unit/application/test_secret_destroy.py::test_a_stale_expected_version_is_refused_before_the_proof_is_checked
#[test]
fn a_stale_expected_version_is_refused_before_the_proof_is_checked() {
    let rig = rig();
    let now_version = now(&rig).as_u64().unwrap();
    let error = rig
        .service
        .destroy(PATH, &json!(1), &json!(now_version - 1), &wrong())
        .unwrap_err();
    assert_eq!(current_version(&error), Some(now_version));
    assert!(is_invalid(
        &rig.service
            .destroy(PATH, &json!(1), &Value::Null, &right())
            .unwrap_err()
    ));
    assert_eq!(failures(&rig), 0);
}

// py: unit/application/test_secret_destroy.py::test_unknown_version_or_path_is_not_found
#[test]
fn unknown_version_or_path_is_not_found() {
    let rig = rig();
    assert!(is_not_found(&destroy(&rig, 9, &right()).unwrap_err()));
    let missing = rig
        .service
        .destroy("secret://hermes/missing", &json!(1), &json!(1), &right());
    assert!(is_not_found(&missing.unwrap_err()));
}

// py: unit/application/test_secret_destroy.py::test_sealed_hecate_refuses
#[test]
fn sealed_hecate_refuses() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    let error = rig
        .service
        .destroy(PATH, &json!(1), &json!(6), &right())
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
}

// py: unit/application/test_secret_destroy.py::test_the_log_records_the_method_of_the_proof_and_the_result
#[test]
fn the_log_records_the_method_of_the_proof_and_the_result() {
    let rig = rig();
    assert!(is_auth(&destroy(&rig, 1, &wrong()).unwrap_err()));
    destroy(&rig, 1, &json!({"shares": &rig.shares[..2]})).unwrap();
    let rows = rig.secret_rows();
    let [failed, done] = rows.as_slice() else {
        panic!("two rows: {rows:?}")
    };
    assert_eq!(
        (
            failed.operation,
            failed.method,
            failed.result,
            failed.version
        ),
        (
            Operation::SecretDestroy,
            Method::Passphrase,
            AccessResult::Failure,
            Some(1)
        )
    );
    assert_eq!(
        (
            done.operation,
            done.method,
            done.result,
            done.subject.as_deref(),
            done.path.as_deref()
        ),
        (
            Operation::SecretDestroy,
            Method::Recovery,
            AccessResult::Success,
            Some("admin"),
            Some(PATH)
        )
    );
    let shown = format!("{:?}", rig.access_log.entries());
    assert!(!shown.contains(GOOD_PASSPHRASE) && !shown.contains("wrong wrong"));
}

// py: unit/application/test_secret_destroy.py::test_a_failure_while_destroying_leaves_the_version_revoked_with_its_material
#[test]
fn a_failure_while_destroying_leaves_the_version_revoked_with_its_material() {
    let rig = rig();
    let before = now(&rig);
    rig.store.fail_on(Some("delete_secret_material"));
    assert_eq!(destroy(&rig, 1, &right()).unwrap_err().kind(), "DiskFull");
    rig.store.fail_on(None);
    assert!(state_of(&rig, PATH, 1) == "revoked" && now(&rig) == before);
    assert!(has_material(&rig, PATH, 1));
}
