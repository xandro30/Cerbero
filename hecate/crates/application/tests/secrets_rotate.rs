mod common;

use common::*;
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::SecretsRig;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};
use time::Duration;

const PATH: &str = "secret://hermes/telegram/bot_token";
const LIMITED: &str = "secret://hermes/limited";
const V1: &str = "first-value-111";
const V2: &str = "second-value-222";
const V3: &str = "third-value-333";

fn rig() -> SecretsRig {
    let rig = secrets();
    create(&rig, PATH, V1, "string");
    rig.access_log.clear();
    rig
}

fn limited(rig: &SecretsRig) {
    let options = CreateOptions {
        max_versions: json!(2),
        ..CreateOptions::default()
    };
    rig.service
        .create(LIMITED, "v1-limited", "string", &options)
        .unwrap();
    rig.service
        .add_version(LIMITED, "v2-limited", &current(rig, LIMITED))
        .unwrap();
}

// py: unit/application/test_secret_rotate.py::test_add_version_makes_the_new_one_active_and_the_old_one_deprecated
#[test]
fn add_version_makes_the_new_one_active_and_the_old_one_deprecated() {
    let rig = rig();
    let out = rig
        .service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    assert!(out.version == 2 && out.fingerprint.len() == 19);
    assert_eq!(json!(out.stream_version), current(&rig, PATH));
    assert_eq!(out.stream_version, 4);
    assert_eq!(states(&rig, PATH), [(1, "deprecated"), (2, "active")]);
    let resolved = resolve_one(&rig, PATH);
    assert_eq!((resolved.value.as_str(), resolved.version), (V2, 2));
}

// py: unit/application/test_secret_rotate.py::test_the_new_version_has_its_own_fingerprint_and_the_same_scope_key
#[test]
fn the_new_version_has_its_own_fingerprint_and_the_same_scope_key() {
    let rig = rig();
    let first = rig.service.get(PATH).unwrap().versions[0]
        .fingerprint
        .clone();
    let out = rig
        .service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    assert_ne!(out.fingerprint, first);
    assert_eq!(rig.store.key_material_count(), 1);
    assert_eq!(kinds(&events(&rig, "key/hermes")), ["ScopeKeyCreated"]);
}

// py: unit/application/test_secret_rotate.py::test_the_previous_version_resolves_explicitly_during_the_grace_and_not_after
#[test]
fn the_previous_version_resolves_explicitly_during_the_grace_and_not_after() {
    let rig = rig();
    rig.service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    let previous = format!("{PATH}@1");
    assert_eq!(resolve_one(&rig, &previous).value.as_str(), V1);
    rig.clock
        .advance(Duration::hours(24) - Duration::seconds(1));
    assert_eq!(resolve_one(&rig, &previous).value.as_str(), V1);
    rig.clock.advance(Duration::seconds(1));
    assert!(!can_resolve(&rig, &previous));
    assert_eq!(resolve_one(&rig, PATH).value.as_str(), V2);
    assert_eq!(states(&rig, PATH), [(1, "revoked"), (2, "active")]);
}

// py: unit/application/test_secret_rotate.py::test_a_stale_expected_version_conflicts_with_the_current_one_and_changes_nothing
#[test]
fn a_stale_expected_version_conflicts_with_the_current_one_and_changes_nothing() {
    let rig = rig();
    let stale = current(&rig, PATH);
    rig.service.add_version(PATH, V2, &stale).unwrap();
    let now = current(&rig, PATH);
    let error = rig.service.add_version(PATH, V3, &stale).unwrap_err();
    assert_eq!(current_version(&error).map(|v| json!(v)), Some(now.clone()));
    assert!(conflict_state(&error).is_none());
    assert_eq!(current(&rig, PATH), now);
    assert_eq!(rig.store.secret_material_count(), 2);
    assert_eq!(resolve_one(&rig, PATH).value.as_str(), V2);
}

// py: unit/application/test_secret_rotate.py::test_a_missing_or_malformed_expected_version_is_invalid
#[test]
fn a_missing_or_malformed_expected_version_is_invalid() {
    for expected in [Value::Null, json!("2"), json!(2.0), json!(true), json!([2])] {
        let rig = rig();
        let before = current(&rig, PATH);
        assert!(
            is_invalid(&rig.service.add_version(PATH, V2, &expected).unwrap_err()),
            "{expected}"
        );
        assert_eq!(current(&rig, PATH), before);
        assert_eq!(rig.store.secret_material_count(), 1);
    }
}

// py: unit/application/test_secret_rotate.py::test_an_unknown_path_is_not_found
#[test]
fn an_unknown_path_is_not_found() {
    let error = rig()
        .service
        .add_version("secret://hermes/missing", V2, &json!(2))
        .unwrap_err();
    assert!(is_not_found(&error));
}

// py: unit/application/test_secret_rotate.py::test_a_malformed_path_is_invalid
#[test]
fn a_malformed_path_is_invalid() {
    assert!(is_invalid(
        &rig()
            .service
            .add_version("nonsense", V2, &json!(2))
            .unwrap_err()
    ));
}

// py: unit/application/test_secret_rotate.py::test_the_value_must_be_valid_for_the_type_of_the_secret
#[test]
fn the_value_must_be_valid_for_the_type_of_the_secret() {
    let rig = rig();
    let doc = "secret://hermes/doc";
    create(&rig, doc, "{\"a\": 1}", "json");
    let before = current(&rig, doc);
    assert!(is_invalid(
        &rig.service
            .add_version(doc, "{broken", &before)
            .unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service
            .add_version(PATH, "", &current(&rig, PATH))
            .unwrap_err()
    ));
    assert_eq!(current(&rig, doc), before);
    rig.service.add_version(doc, "{\"a\": 2}", &before).unwrap();
}

// py: unit/application/test_secret_rotate.py::test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change
#[test]
fn exceeding_the_maximum_destroys_the_oldest_in_the_same_change() {
    let rig = rig();
    limited(&rig);
    assert!(has_material(&rig, LIMITED, 1));
    rig.service
        .add_version(LIMITED, "v3-limited", &current(&rig, LIMITED))
        .unwrap();
    assert!(
        !has_material(&rig, LIMITED, 1),
        "the destroyed version keeps no material"
    );
    assert!(has_material(&rig, LIMITED, 3));
    assert_eq!(
        states(&rig, LIMITED),
        [(1, "destroyed"), (2, "deprecated"), (3, "active")]
    );
    let destroyed: Vec<(Value, Value)> = events(&rig, "secret/hermes/limited")
        .into_iter()
        .filter(|record| record.kind == "VersionDestroyed")
        .map(|record| {
            (
                record.data["version"].clone(),
                record.data["reason"].clone(),
            )
        })
        .collect();
    assert_eq!(destroyed, [(json!(1), json!("max_versions"))]);
    assert!(!can_resolve(&rig, &format!("{LIMITED}@1")));
    let listed = rig.service.list_secrets(&Value::Null).unwrap();
    assert_eq!(
        listed
            .iter()
            .find(|item| item.path == LIMITED)
            .unwrap()
            .version_count,
        2
    );
}

// py: unit/application/test_secret_rotate.py::test_list_reports_the_active_version_and_how_many_are_kept
#[test]
fn list_reports_the_active_version_and_how_many_are_kept() {
    let rig = rig();
    for value in [V2, V3] {
        rig.service
            .add_version(PATH, value, &current(&rig, PATH))
            .unwrap();
    }
    let item = rig.service.list_secrets(&Value::Null).unwrap().remove(0);
    assert_eq!((item.active_version, item.version_count), (Some(3), 3));
}

// py: unit/application/test_secret_rotate.py::test_the_access_log_records_the_admin_the_path_and_the_new_version
#[test]
fn the_access_log_records_the_admin_the_path_and_the_new_version() {
    let rig = rig();
    rig.service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (
            row.operation,
            row.subject.as_deref(),
            row.method,
            row.result
        ),
        (
            Operation::SecretAddVersion,
            Some("admin"),
            Method::Local,
            AccessResult::Success
        )
    );
    assert_eq!((row.path.as_deref(), row.version), (Some(PATH), Some(2)));
    assert!(!format!("{row:?}").contains(V2));
}

// py: unit/application/test_secret_rotate.py::test_refusals_are_logged
#[test]
fn refusals_are_logged() {
    let rig = rig();
    assert!(is_conflict(
        &rig.service.add_version(PATH, V2, &json!(1)).unwrap_err()
    ));
    assert!(is_not_found(
        &rig.service
            .add_version("secret://hermes/missing", V2, &json!(1))
            .unwrap_err()
    ));
    assert!(is_invalid(
        &rig.service.add_version(PATH, V2, &Value::Null).unwrap_err()
    ));
    let results: Vec<AccessResult> = rig.secret_rows().iter().map(|row| row.result).collect();
    assert_eq!(
        results,
        [
            AccessResult::RejectedConflict,
            AccessResult::NotFound,
            AccessResult::InvalidInput
        ]
    );
}

// py: unit/application/test_secret_rotate.py::test_sealed_hecate_refuses
#[test]
fn sealed_hecate_refuses() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    let error = rig.service.add_version(PATH, V2, &json!(2)).unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
}

// py: unit/application/test_secret_rotate.py::test_a_failure_while_saving_leaves_the_secret_as_it_was
#[test]
fn a_failure_while_saving_leaves_the_secret_as_it_was() {
    let rig = rig();
    let before = current(&rig, PATH);
    rig.store.fail_on(Some("save_secret_material"));
    assert_eq!(
        rig.service
            .add_version(PATH, V2, &before)
            .unwrap_err()
            .kind(),
        "DiskFull"
    );
    rig.store.fail_on(None);
    assert_eq!(current(&rig, PATH), before);
    assert_eq!(rig.store.secret_material_count(), 1);
    assert_eq!(resolve_one(&rig, PATH).value.as_str(), V1);
}

// py: unit/application/test_secret_rotate.py::test_a_failure_while_destroying_the_oldest_leaves_everything_as_it_was
#[test]
fn a_failure_while_destroying_the_oldest_leaves_everything_as_it_was() {
    let rig = rig();
    limited(&rig);
    let before = current(&rig, LIMITED);
    rig.store.fail_on(Some("delete_secret_material"));
    assert_eq!(
        rig.service
            .add_version(LIMITED, "v3-limited", &before)
            .unwrap_err()
            .kind(),
        "DiskFull"
    );
    rig.store.fail_on(None);
    assert_eq!(current(&rig, LIMITED), before);
    assert_eq!(resolve_one(&rig, LIMITED).value.as_str(), "v2-limited");
    assert!(has_material(&rig, LIMITED, 1));
}

// py: unit/application/test_secret_rotate.py::test_no_value_is_stored_in_clear
#[test]
fn no_value_is_stored_in_clear() {
    let rig = rig();
    rig.service
        .add_version(PATH, V2, &current(&rig, PATH))
        .unwrap();
    let contents = rig.store.contents();
    assert!(!contains(&contents, V1.as_bytes()) && !contains(&contents, V2.as_bytes()));
    assert!(!format!("{:?}", rig.service.get(PATH).unwrap()).contains(V2));
}
