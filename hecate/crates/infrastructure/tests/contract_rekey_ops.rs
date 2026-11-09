mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};

fn unsealed_handler() -> (hecate_application::testing::SealingRig, RequestHandler) {
    let (rig, handler, _) = initialized_handler();
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    (rig, handler)
}

fn proof() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

// py: contract/test_rekey_ops.py::test_begin_confirm_flow
#[test]
fn begin_confirm_flow() {
    let (_rig, handler) = unsealed_handler();
    let begin = call(&handler, "shares.rekey.begin", json!({"proof": proof()}));
    assert_eq!(begin["status"], "completed");
    for key in ["rekey_id", "shares", "expires_at"] {
        assert!(begin["data"].get(key).is_some(), "{key}");
    }
    assert_eq!(begin["data"]["shares"].as_array().unwrap().len(), 3);
    let shares = &begin["data"]["shares"].as_array().unwrap()[..2];
    let confirm = call(
        &handler,
        "shares.rekey.confirm",
        json!({"rekey_id": begin["data"]["rekey_id"], "shares": shares}),
    );
    assert!(confirm["status"] == "completed" && confirm["data"] == json!({"config_version": 2}));
}

// py: contract/test_rekey_ops.py::test_abort_completes_and_discards
#[test]
fn abort_completes_and_discards() {
    let (_rig, handler) = unsealed_handler();
    let begin = call(&handler, "shares.rekey.begin", json!({"proof": proof()}));
    let abort = call(
        &handler,
        "shares.rekey.abort",
        json!({"rekey_id": begin["data"]["rekey_id"]}),
    );
    assert_eq!(abort["data"], json!({}));
    let shares = &begin["data"]["shares"].as_array().unwrap()[..2];
    let again = call(
        &handler,
        "shares.rekey.confirm",
        json!({"rekey_id": begin["data"]["rekey_id"], "shares": shares}),
    );
    assert_eq!(code(&again), "validation");
}

// py: contract/test_rekey_ops.py::test_error_codes
#[test]
fn error_codes() {
    let (_rig, handler) = unsealed_handler();
    assert_eq!(
        code(&call(&handler, "shares.rekey.begin", json!({}))),
        "validation"
    );
    assert_eq!(
        code(&call(&handler, "shares.rekey.begin", json!({"proof": "x"}))),
        "validation"
    );
    let bad = call(
        &handler,
        "shares.rekey.begin",
        json!({"proof": {"passphrase": "nope nope nope"}}),
    );
    assert!(code(&bad) == "auth" && !bad.to_string().contains("nope nope nope"));
    let mistyped = json!({"rekey_id": 1, "shares": []});
    assert_eq!(
        code(&call(&handler, "shares.rekey.confirm", mistyped)),
        "validation"
    );
}

// py: contract/test_rekey_ops.py::test_rekey_ops_while_sealed_are_conflicts
#[test]
fn rekey_ops_while_sealed_are_conflicts() {
    let (_rig, handler) = unsealed_handler();
    call(&handler, "seal", json!({}));
    let ops = [
        ("shares.rekey.begin", json!({"proof": proof()})),
        (
            "shares.rekey.confirm",
            json!({"rekey_id": "x", "shares": ["a", "b"]}),
        ),
        ("shares.rekey.abort", json!({"rekey_id": "x"})),
    ];
    for (op, args) in ops {
        let reply = call(&handler, op, args);
        assert!(
            code(&reply) == "conflict" && reply["data"] == json!({"state": "sealed"}),
            "{op}"
        );
    }
}
