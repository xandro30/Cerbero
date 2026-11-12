mod common;

use common::*;
use hecate_application::secrets::service::CreateOptions;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use serde_json::{Value, json};

const PATH: &str = "secret://hermes/telegram/family_bot/bot_token";
const VALUE: &str = "7312:AAF-super-secret-bot-token";

// py: unit/application/test_secret_create.py::test_create_gives_version_one_active_with_a_fingerprint
#[test]
fn create_gives_version_one_active_with_a_fingerprint() {
    let rig = secrets();
    let out = create(&rig, PATH, VALUE, "string");
    assert!(out.path == PATH && out.version == 1);
    assert!(is_fingerprint(&out.fingerprint));
    assert_eq!(out.stream_version, 2);
    assert_eq!(
        rig.service.get(PATH).unwrap().versions[0].state.as_str(),
        "active"
    );
}

// py: unit/application/test_secret_create.py::test_the_first_secret_of_an_identity_creates_its_key_only_once
#[test]
fn the_first_secret_of_an_identity_creates_its_key_only_once() {
    let rig = secrets();
    create(&rig, PATH, VALUE, "string");
    create(&rig, "secret://hermes/other", VALUE, "string");
    assert_eq!(kinds(&events(&rig, "key/hermes")), ["ScopeKeyCreated"]);
    assert!(rig.store.has_key_material("hermes", 1) && rig.store.key_material_count() == 1);
    create(&rig, "secret://echidna/key", VALUE, "string");
    assert_eq!(kinds(&events(&rig, "key/echidna")), ["ScopeKeyCreated"]);
    assert_eq!(rig.store.key_material_count(), 2);
}

// py: unit/application/test_secret_create.py::test_the_same_value_has_different_fingerprints_in_different_identities
#[test]
fn the_same_value_has_different_fingerprints_in_different_identities() {
    let rig = secrets();
    let first = create(&rig, "secret://hermes/a", VALUE, "string").fingerprint;
    let second = create(&rig, "secret://echidna/a", VALUE, "string").fingerprint;
    let same_identity = create(&rig, "secret://hermes/b", VALUE, "string").fingerprint;
    assert!(first != second && first == same_identity);
}

// py: unit/application/test_secret_create.py::test_metadata_given_at_creation_is_kept
#[test]
fn metadata_given_at_creation_is_kept() {
    let rig = secrets();
    let options = CreateOptions {
        max_versions: json!(3),
        rotate_every: json!("P90D"),
        expires_at: json!("2027-01-01T00:00:00+00:00"),
    };
    rig.service.create(PATH, VALUE, "string", &options).unwrap();
    let got = rig.service.get(PATH).unwrap();
    assert_eq!(
        (got.max_versions, got.rotate_every.as_deref()),
        (3, Some("P90D"))
    );
    assert_eq!(
        got.expires_at.map(isoformat).as_deref(),
        Some("2027-01-01T00:00:00+00:00")
    );
}

// py: unit/application/test_secret_create.py::test_invalid_input_changes_nothing
#[test]
fn invalid_input_changes_nothing() {
    let long_value = "x".repeat(64 * 1024 + 1);
    let options = |max_versions: Value, rotate_every: Value, expires_at: Value| CreateOptions {
        max_versions,
        rotate_every,
        expires_at,
    };
    let none = || options(Value::Null, Value::Null, Value::Null);
    let cases: Vec<(&str, &str, &str, CreateOptions)> = vec![
        ("hermes/token", VALUE, "string", none()),
        ("secret://Hermes/token", VALUE, "string", none()),
        (PATH, VALUE, "binary", none()),
        (PATH, "{nope", "json", none()),
        (PATH, "not a pem", "pem", none()),
        (PATH, "", "string", none()),
        (PATH, &long_value, "string", none()),
        (
            PATH,
            VALUE,
            "string",
            options(json!(0), Value::Null, Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(json!(1), Value::Null, Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(json!(1000), Value::Null, Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(json!("ten"), Value::Null, Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(Value::Null, json!(""), Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(Value::Null, json!("x".repeat(100)), Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(Value::Null, json!(5), Value::Null),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(Value::Null, Value::Null, json!("tomorrow")),
        ),
        (
            PATH,
            VALUE,
            "string",
            options(Value::Null, Value::Null, json!(12345)),
        ),
    ];
    for (path, value, value_type, options) in cases {
        let rig = secrets();
        let error = rig
            .service
            .create(path, value, value_type, &options)
            .unwrap_err();
        assert!(
            is_invalid(&error),
            "{path} {value_type} {options:?}: {error:?}"
        );
        assert!(rig.store.is_empty());
    }
}

// py: unit/application/test_secret_create.py::test_a_value_of_exactly_64_kib_is_accepted
#[test]
fn a_value_of_exactly_64_kib_is_accepted() {
    let rig = secrets();
    assert_eq!(
        create(&rig, PATH, &"x".repeat(64 * 1024), "string").version,
        1
    );
}

// py: unit/application/test_secret_create.py::test_creating_over_an_existing_path_conflicts_and_keeps_the_original
#[test]
fn creating_over_an_existing_path_conflicts_and_keeps_the_original() {
    let rig = secrets();
    create(&rig, PATH, VALUE, "string");
    let before = events(&rig, &stream_of(PATH)).len();
    let error = rig
        .service
        .create(PATH, "another value", "string", &CreateOptions::default())
        .unwrap_err();
    assert!(is_conflict(&error) && conflict_state(&error).is_none());
    assert_eq!(events(&rig, &stream_of(PATH)).len(), before);
    assert_eq!(rig.store.secret_material_count(), 1);
}

// py: unit/application/test_secret_create.py::test_creating_while_sealed_conflicts
#[test]
fn creating_while_sealed_conflicts() {
    let rig = sealed_secrets();
    let error = rig
        .service
        .create(PATH, VALUE, "string", &CreateOptions::default())
        .unwrap_err();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    assert!(rig.store.is_empty());
}

// py: unit/application/test_secret_create.py::test_a_failure_while_saving_leaves_no_trace
#[test]
fn a_failure_while_saving_leaves_no_trace() {
    let rig = secrets();
    rig.store.fail_on(Some("save_secret_material"));
    let error = rig
        .service
        .create(PATH, VALUE, "string", &CreateOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), "DiskFull");
    assert!(rig.store.is_empty());
    rig.store.fail_on(None);
    assert_eq!(create(&rig, PATH, VALUE, "string").version, 1);
}

// py: unit/application/test_secret_create.py::test_the_access_log_records_who_where_and_which_version_but_no_value
#[test]
fn the_access_log_records_who_where_and_which_version_but_no_value() {
    let rig = secrets();
    let out = create(&rig, PATH, VALUE, "string");
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
            Operation::SecretCreate,
            Some("admin"),
            Method::Local,
            AccessResult::Success
        )
    );
    assert_eq!((row.path.as_deref(), row.version), (Some(PATH), Some(1)));
    let shown = format!("{row:?}");
    assert!(!shown.contains(VALUE) && !shown.contains(&out.fingerprint));
}

// py: unit/application/test_secret_create.py::test_refusals_are_logged_too
#[test]
fn refusals_are_logged_too() {
    let rig = secrets();
    create(&rig, PATH, VALUE, "string");
    for (value_type, result) in [
        ("binary", AccessResult::InvalidInput),
        ("string", AccessResult::RejectedConflict),
    ] {
        rig.service
            .create(PATH, VALUE, value_type, &CreateOptions::default())
            .unwrap_err();
        assert_eq!(rig.secret_rows().last().unwrap().result, result);
    }
    let sealed = sealed_secrets();
    sealed
        .service
        .create(PATH, VALUE, "string", &CreateOptions::default())
        .unwrap_err();
    assert_eq!(
        sealed.secret_rows().last().unwrap().result,
        AccessResult::RejectedConflict
    );
}

// py: unit/application/test_secret_create.py::test_a_malformed_path_is_never_written_to_the_log
#[test]
fn a_malformed_path_is_never_written_to_the_log() {
    let rig = secrets();
    let error = rig
        .service
        .create(
            "my-actual-password-by-mistake",
            VALUE,
            "string",
            &CreateOptions::default(),
        )
        .unwrap_err();
    assert!(is_invalid(&error));
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert!(row.path.is_none() && !format!("{row:?}").contains("password"));
}

// py: unit/application/test_secret_create.py::test_the_value_appears_nowhere_in_the_store
#[test]
fn the_value_appears_nowhere_in_the_store() {
    let rig = secrets();
    create(&rig, PATH, VALUE, "string");
    assert!(!contains(&rig.store.contents(), VALUE.as_bytes()));
    assert!(!format!("{:?}", rig.access_log.entries()).contains(VALUE));
}

// py: unit/domain/test_secret_aggregate.py::test_a_maximum_outside_2_to_100_is_rejected
#[test]
fn a_maximum_that_is_not_an_integer_is_rejected_like_one_out_of_range() {
    for max_versions in [json!(true), json!("2"), json!(2.0)] {
        let rig = secrets();
        let options = CreateOptions {
            max_versions: max_versions.clone(),
            ..CreateOptions::default()
        };
        let error = rig
            .service
            .create(PATH, VALUE, "string", &options)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "max_versions must be a number from 2 to 100",
            "{max_versions}"
        );
        assert!(rig.store.is_empty());
    }
}

// --- Feature 006: `rotate_every` as a duration (T032, FR-023) -------------------------------------

#[test]
fn rotate_every_is_a_duration_in_days_or_weeks() {
    let rig = secrets();
    for (n, rotate_every) in ["P90D", "P12W", "P3650D", "P520W"].iter().enumerate() {
        let options = CreateOptions {
            rotate_every: json!(rotate_every),
            ..CreateOptions::default()
        };
        rig.service
            .create(
                &format!("secret://hermes/kept_{n}"),
                VALUE,
                "string",
                &options,
            )
            .unwrap();
    }
    for rotate_every in [
        json!("90d"),
        json!("P0D"),
        json!("P3M"),
        json!("P1W2D"),
        json!(""),
        json!(90),
    ] {
        let options = CreateOptions {
            rotate_every: rotate_every.clone(),
            ..CreateOptions::default()
        };
        let error = rig
            .service
            .create("secret://hermes/refused", VALUE, "string", &options)
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{rotate_every}");
        assert_eq!(
            error.to_string(),
            "rotate_every must be an ISO 8601 duration in days or weeks, such as P90D or P12W",
            "{rotate_every}"
        );
    }
    assert!(rig.service.get("secret://hermes/refused").is_err());
}
