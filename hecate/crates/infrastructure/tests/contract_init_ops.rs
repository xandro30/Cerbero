mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use serde_json::{Value, json};

fn begun(handler: &hecate_infrastructure::control::server::RequestHandler) -> Value {
    call(
        handler,
        "init.begin",
        json!({"passphrase": GOOD_PASSPHRASE}),
    )
}

// py: contract/test_init_ops.py::test_init_begin_confirm_flow_in_common_envelope
#[test]
fn init_begin_confirm_flow_in_common_envelope() {
    let handler = handler_for(rig().service);
    let begin = begun(&handler);
    assert!(begin["status"] == "completed" && begin["error"].is_null());
    assert_eq!(begin["meta"]["request_id"], "r");
    for key in ["init_id", "shares", "expires_at", "warning"] {
        assert!(begin["data"].get(key).is_some(), "{key}");
    }
    assert_eq!(begin["data"]["shares"].as_array().unwrap().len(), 3);
    let shares = &begin["data"]["shares"].as_array().unwrap()[1..];
    let confirm = call(
        &handler,
        "init.confirm",
        json!({"init_id": begin["data"]["init_id"], "shares": shares}),
    );
    assert_eq!(confirm["status"], "completed");
    assert_eq!(confirm["data"], json!({"state": "sealed"}));
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["state"],
        "sealed"
    );
}

// py: contract/test_init_ops.py::test_init_abort_discards_the_pending_init
#[test]
fn init_abort_discards_the_pending_init() {
    let handler = handler_for(rig().service);
    let begin = begun(&handler);
    let abort = call(
        &handler,
        "init.abort",
        json!({"init_id": begin["data"]["init_id"]}),
    );
    assert!(abort["status"] == "completed" && abort["data"] == json!({}));
    let shares = &begin["data"]["shares"].as_array().unwrap()[..2];
    let again = call(
        &handler,
        "init.confirm",
        json!({"init_id": begin["data"]["init_id"], "shares": shares}),
    );
    assert_eq!(code(&again), "validation");
}

// py: contract/test_init_ops.py::test_short_passphrase_is_validation_not_retryable
#[test]
fn short_passphrase_is_validation_not_retryable() {
    let reply = call(
        &handler_for(rig().service),
        "init.begin",
        json!({"passphrase": "short"}),
    );
    assert_eq!(reply["status"], "failed");
    assert_eq!(code(&reply), "validation");
    assert_eq!(reply["error"]["retryable"], false);
    assert!(
        !reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("short")
    );
}

// py: contract/test_init_ops.py::test_missing_or_mistyped_args_are_validation
#[test]
fn missing_or_mistyped_args_are_validation() {
    let handler = handler_for(rig().service);
    assert_eq!(code(&call(&handler, "init.begin", json!({}))), "validation");
    assert_eq!(
        code(&call(&handler, "init.begin", json!({"passphrase": 12}))),
        "validation"
    );
    assert_eq!(
        code(&call(
            &handler,
            "init.confirm",
            json!({"init_id": "x", "shares": "not a list"})
        )),
        "validation"
    );
}

// py: unit/domain/test_passphrase.py::test_rejects_non_text
#[test]
fn a_passphrase_that_is_not_text_is_refused_before_the_service() {
    let handler = handler_for(rig().service);
    for passphrase in [Value::Null, json!(12), json!(["a long passphrase"])] {
        let reply = call(&handler, "init.begin", json!({"passphrase": passphrase}));
        assert_eq!(reply["error"]["message"], "'passphrase' must be a string");
    }
}

// py: contract/test_init_ops.py::test_second_initialization_is_conflict_with_current_state
#[test]
fn second_initialization_is_conflict_with_current_state() {
    let handler = handler_for(rig().service);
    let begin = begun(&handler);
    let shares = &begin["data"]["shares"].as_array().unwrap()[..2];
    call(
        &handler,
        "init.confirm",
        json!({"init_id": begin["data"]["init_id"], "shares": shares}),
    );
    let reply = begun(&handler);
    assert_eq!(reply["status"], "failed");
    assert_eq!(code(&reply), "conflict");
    assert_eq!(reply["data"], json!({"state": "sealed"}));
}
