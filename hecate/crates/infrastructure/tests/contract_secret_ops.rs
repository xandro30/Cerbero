mod common;

use common::*;
use hecate_application::testing::SecretsRig;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Map, Value, json};

const PATH: &str = "secret://hermes/telegram/bot_token";
const VALUE: &str = "contract-secret-value";

fn make(unsealed: bool) -> (SecretsRig, RequestHandler) {
    let rig = secrets_rig(unsealed);
    let handler = secrets_handler(&rig);
    (rig, handler)
}

fn create_with(handler: &RequestHandler, extra: Value) -> Value {
    let mut args: Map<String, Value> =
        json!({"path": PATH, "value": VALUE, "value_type": "string"})
            .as_object()
            .unwrap()
            .clone();
    args.extend(extra.as_object().unwrap().clone());
    call(handler, "secrets.create", Value::Object(args))
}

fn create(handler: &RequestHandler) -> Value {
    create_with(handler, json!({}))
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn get(handler: &RequestHandler) -> Value {
    call(handler, "secrets.get", json!({"path": PATH}))["data"].clone()
}

// py: contract/test_secret_ops.py::test_create_replies_with_the_common_envelope
#[test]
fn create_replies_with_the_common_envelope() {
    let (_rig, handler) = make(true);
    let reply = create(&handler);
    assert_eq!(keys(&reply), ["data", "error", "meta", "status"]);
    assert!(reply["status"] == "completed" && reply["error"].is_null());
    assert_eq!(reply["meta"]["request_id"], "r");
    assert_eq!(
        keys(&reply["data"]),
        ["fingerprint", "path", "stream_version", "version"]
    );
    assert_eq!(
        (
            &reply["data"]["path"],
            &reply["data"]["version"],
            &reply["data"]["stream_version"]
        ),
        (&json!(PATH), &json!(1), &json!(2))
    );
}

// py: contract/test_secret_ops.py::test_create_with_a_maximum_of_one_version_is_validation
#[test]
fn create_with_a_maximum_of_one_version_is_validation() {
    let (_rig, handler) = make(true);
    let reply = create_with(&handler, json!({"max_versions": 1}));
    assert!(reply["status"] == "failed" && code(&reply) == "validation");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("from 2 to 100")
    );
}

// py: contract/test_secret_ops.py::test_create_accepts_optional_metadata
#[test]
fn create_accepts_optional_metadata() {
    let (_rig, handler) = make(true);
    let reply = create_with(
        &handler,
        json!({"max_versions": 4, "rotate_every": "30d", "expires_at": "2027-01-01T00:00:00+00:00"}),
    );
    assert_eq!(reply["status"], "completed");
    let got = get(&handler);
    assert_eq!(
        (
            &got["max_versions"],
            &got["rotate_every"],
            &got["expires_at"]
        ),
        (
            &json!(4),
            &json!("30d"),
            &json!("2027-01-01T00:00:00+00:00")
        )
    );
}

// py: contract/test_secret_ops.py::test_list_returns_the_documented_shape_and_filters
#[test]
fn list_returns_the_documented_shape_and_filters() {
    let (_rig, handler) = make(true);
    create(&handler);
    create_with(&handler, json!({"path": "secret://echidna/key"}));
    let everything = call(&handler, "secrets.list", json!({}))["data"]["secrets"].clone();
    let paths: Vec<&Value> = everything
        .as_array()
        .unwrap()
        .iter()
        .map(|item| &item["path"])
        .collect();
    assert_eq!(paths, [&json!("secret://echidna/key"), &json!(PATH)]);
    for key in [
        "path",
        "value_type",
        "active_version",
        "version_count",
        "expires_at",
    ] {
        assert!(everything[0].get(key).is_some(), "{key}");
    }
    let only =
        call(&handler, "secrets.list", json!({"identity": "hermes"}))["data"]["secrets"].clone();
    assert_eq!(only.as_array().unwrap().len(), 1);
    assert_eq!(only[0]["path"], PATH);
}

// py: contract/test_secret_ops.py::test_get_returns_metadata_stream_version_and_versions_without_the_value
#[test]
fn get_returns_metadata_stream_version_and_versions_without_the_value() {
    let (_rig, handler) = make(true);
    create(&handler);
    let reply = call(&handler, "secrets.get", json!({"path": PATH}));
    assert!(reply["data"]["stream_version"] == 2 && reply["data"]["value_type"] == "string");
    assert_eq!(reply["data"]["versions"][0]["state"], "active");
    assert!(!reply.to_string().contains(VALUE));
}

// py: contract/test_secret_ops.py::test_validation_errors
#[test]
fn validation_errors() {
    let long = "x".repeat(64 * 1024 + 1);
    let base = |extra: Value| -> Value {
        let mut args = json!({"path": PATH, "value": VALUE, "value_type": "string"})
            .as_object()
            .unwrap()
            .clone();
        args.extend(extra.as_object().unwrap().clone());
        Value::Object(args)
    };
    let cases = [
        (
            "secrets.create",
            json!({"path": "nonsense", "value": VALUE, "value_type": "string"}),
        ),
        ("secrets.create", base(json!({"value_type": "binary"}))),
        (
            "secrets.create",
            json!({"path": PATH, "value": "{bad", "value_type": "json"}),
        ),
        ("secrets.create", base(json!({"value": long}))),
        ("secrets.create", json!({"path": PATH, "value": VALUE})),
        ("secrets.create", base(json!({"rotate_every": ""}))),
        (
            "secrets.create",
            base(json!({"rotate_every": "x".repeat(65)})),
        ),
        ("secrets.create", base(json!({"rotate_every": "90\nd"}))),
        ("secrets.create", base(json!({"rotate_every": 90}))),
        ("secrets.create", base(json!({"expires_at": "tomorrow"}))),
        ("secrets.create", base(json!({"expires_at": 20270101}))),
        (
            "secrets.create",
            json!({"path": PATH, "value": 5, "value_type": "string"}),
        ),
        ("secrets.create", json!({})),
        ("secrets.get", json!({"path": 5})),
        ("secrets.get", json!({})),
        ("secrets.get", json!({"path": "nonsense"})),
        ("secrets.list", json!({"identity": "Bad Identity"})),
        ("secrets.list", json!({"identity": 7})),
    ];
    for (op, args) in cases {
        let (rig, handler) = make(true);
        let reply = call(&handler, op, args.clone());
        assert!(
            reply["status"] == "failed" && code(&reply) == "validation",
            "{op} {args}"
        );
        assert!(reply["error"]["retryable"] == false && reply["data"] == json!({}));
        assert!(rig.store.is_empty());
        assert_eq!(
            rig.secret_rows().last().unwrap().result,
            AccessResult::InvalidInput,
            "{op} {args}"
        );
    }
}

// py: contract/test_secret_ops.py::test_malformed_arguments_are_logged_without_their_content
#[test]
fn malformed_arguments_are_logged_without_their_content() {
    let (rig, handler) = make(true);
    call(
        &handler,
        "secrets.create",
        json!({"path": "oops-this-is-the-password", "value": 5}),
    );
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert!(row.operation == Operation::SecretCreate && row.result == AccessResult::InvalidInput);
    assert!(!format!("{row:?}").contains("password"));
}

// py: contract/test_secret_ops.py::test_conflict_when_the_path_already_exists
#[test]
fn conflict_when_the_path_already_exists() {
    let (_rig, handler) = make(true);
    create(&handler);
    let reply = create_with(&handler, json!({"value": "another"}));
    assert!(reply["status"] == "failed" && code(&reply) == "conflict");
    assert_eq!(reply["error"]["retryable"], false);
}

// py: contract/test_secret_ops.py::test_conflict_with_the_state_while_sealed
#[test]
fn conflict_with_the_state_while_sealed() {
    for (op, args) in [
        (
            "secrets.create",
            json!({"path": PATH, "value": VALUE, "value_type": "string"}),
        ),
        ("secrets.list", json!({})),
        ("secrets.get", json!({"path": PATH})),
    ] {
        let (_rig, handler) = make(false);
        let reply = call(&handler, op, args);
        assert!(
            reply["status"] == "failed" && code(&reply) == "conflict",
            "{op}"
        );
        assert_eq!(reply["data"], json!({"state": "sealed"}));
    }
}

// py: contract/test_secret_ops.py::test_status_still_works_while_sealed
#[test]
fn status_still_works_while_sealed() {
    let (_rig, handler) = make(false);
    assert_eq!(call(&handler, "status", json!({}))["status"], "completed");
}

// py: contract/test_secret_ops.py::test_get_of_a_missing_path_is_not_found
#[test]
fn get_of_a_missing_path_is_not_found() {
    let (_rig, handler) = make(true);
    let reply = call(
        &handler,
        "secrets.get",
        json!({"path": "secret://hermes/missing"}),
    );
    assert!(reply["status"] == "failed" && code(&reply) == "not_found");
    assert!(reply["error"]["retryable"] == false && reply["data"] == json!({}));
}

// py: contract/test_secret_ops.py::test_secret_operations_do_not_exist_without_a_secrets_service
#[test]
fn secret_operations_do_not_exist_without_a_secrets_service() {
    let rig = secrets_rig(true);
    let handler = request_handler(rig.sealing.clone(), None);
    assert_eq!(
        code(&call(&handler, "secrets.list", json!({}))),
        "validation"
    );
}

// py: contract/test_secret_ops.py::test_add_version_replies_with_the_new_version_and_its_fingerprint
#[test]
fn add_version_replies_with_the_new_version_and_its_fingerprint() {
    let (_rig, handler) = make(true);
    create(&handler);
    let reply = call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "next-value", "expected_version": 2}),
    );
    assert_eq!(reply["status"], "completed");
    assert_eq!(
        keys(&reply["data"]),
        ["fingerprint", "stream_version", "version"]
    );
    assert_eq!(
        (&reply["data"]["version"], &reply["data"]["stream_version"]),
        (&json!(2), &json!(4))
    );
    assert!(!reply.to_string().contains("next-value"));
    let got = get(&handler);
    let states: Vec<&Value> = got["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| &v["state"])
        .collect();
    assert_eq!(states, [&json!("deprecated"), &json!("active")]);
}

// py: contract/test_secret_ops.py::test_add_version_validation
#[test]
fn add_version_validation() {
    for args in [
        json!({"path": PATH, "value": "x"}),
        json!({"path": PATH, "value": "x", "expected_version": "2"}),
        json!({"path": PATH, "value": "x", "expected_version": null}),
        json!({"path": PATH, "expected_version": 2}),
        json!({"path": PATH, "value": 5, "expected_version": 2}),
        json!({"value": "x", "expected_version": 2}),
        json!({"path": "nonsense", "value": "x", "expected_version": 2}),
    ] {
        let (_rig, handler) = make(true);
        create(&handler);
        let reply = call(&handler, "secrets.add_version", args.clone());
        assert!(
            reply["status"] == "failed" && code(&reply) == "validation",
            "{args}"
        );
        assert_eq!(get(&handler)["stream_version"], 2);
    }
}

// py: contract/test_secret_ops.py::test_add_version_with_a_value_invalid_for_the_type_is_validation
#[test]
fn add_version_with_a_value_invalid_for_the_type_is_validation() {
    let (_rig, handler) = make(true);
    create_with(&handler, json!({"value": "{}", "value_type": "json"}));
    let reply = call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "{bad", "expected_version": 2}),
    );
    assert_eq!(code(&reply), "validation");
}

// py: contract/test_secret_ops.py::test_add_version_conflict_carries_the_current_version
#[test]
fn add_version_conflict_carries_the_current_version() {
    let (_rig, handler) = make(true);
    create(&handler);
    call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "v2", "expected_version": 2}),
    );
    let reply = call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "v3", "expected_version": 2}),
    );
    assert!(reply["status"] == "failed" && code(&reply) == "conflict");
    assert_eq!(reply["error"]["retryable"], false);
    assert_eq!(reply["data"], json!({"current_version": 4}));
}

// py: contract/test_secret_ops.py::test_add_version_of_a_missing_path_is_not_found
#[test]
fn add_version_of_a_missing_path_is_not_found() {
    let (_rig, handler) = make(true);
    let args = json!({"path": "secret://hermes/missing", "value": "x", "expected_version": 1});
    assert_eq!(
        code(&call(&handler, "secrets.add_version", args)),
        "not_found"
    );
}

// py: contract/test_secret_ops.py::test_add_version_while_sealed_conflicts_with_the_state
#[test]
fn add_version_while_sealed_conflicts_with_the_state() {
    let (_rig, handler) = make(false);
    let reply = call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "x", "expected_version": 2}),
    );
    assert!(code(&reply) == "conflict" && reply["data"] == json!({"state": "sealed"}));
}

// py: unit/domain/test_secret_path.py::test_rejects_non_text
// py: unit/domain/test_value_type.py::test_parse_known_and_unknown_types
// py: unit/domain/test_value_type.py::test_string_rejects_empty_and_non_text
// py: unit/application/test_secret_create.py::test_invalid_input_changes_nothing
#[test]
fn arguments_that_are_not_text_are_refused_before_the_service() {
    for (args, message) in [
        (
            json!({"path": null, "value": VALUE, "value_type": "string"}),
            "'path' must be a string",
        ),
        (
            json!({"path": PATH, "value": VALUE, "value_type": null}),
            "'value_type' must be a string",
        ),
        (
            json!({"path": PATH, "value": VALUE, "value_type": 3}),
            "'value_type' must be a string",
        ),
        (
            json!({"path": PATH, "value": 123, "value_type": "string"}),
            "'value' must be a string",
        ),
        (
            json!({"path": PATH, "value": [98, 121, 116, 101, 115], "value_type": "string"}),
            "'value' must be a string",
        ),
    ] {
        let (rig, handler) = make(true);
        let reply = call(&handler, "secrets.create", args.clone());
        assert_eq!(reply["error"]["message"], message, "{args}");
        assert!(rig.store.is_empty());
    }
    let (_rig, handler) = make(true);
    let lone = br#"{"request_id": "r", "op": "secrets.create", "args": {"path": "secret://hermes/x", "value": "lone surrogate \ud800", "value_type": "string"}}"#;
    let reply: Value = serde_json::from_slice(
        &hecate_infrastructure::control::server::LineHandler::handle(&handler, lone),
    )
    .unwrap();
    assert_eq!(reply["error"]["message"], "request is not valid JSON");
}
