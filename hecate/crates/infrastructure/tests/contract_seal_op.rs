mod common;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use serde_json::json;

// py: contract/test_seal_op.py::test_seal_unsealed_completes
#[test]
fn seal_unsealed_completes() {
    let (_rig, handler, _) = initialized_handler();
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let reply = call(&handler, "seal", json!({}));
    assert!(reply["status"] == "completed" && reply["data"] == json!({"state": "sealed"}));
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["state"],
        "sealed"
    );
}

// py: contract/test_seal_op.py::test_seal_when_sealed_is_conflict_with_state
#[test]
fn seal_when_sealed_is_conflict_with_state() {
    let (_rig, handler, _) = initialized_handler();
    let reply = call(&handler, "seal", json!({}));
    assert_eq!(code(&reply), "conflict");
    assert_eq!(reply["error"]["retryable"], false);
    assert_eq!(reply["data"], json!({"state": "sealed"}));
}

// py: contract/test_seal_op.py::test_seal_when_uninitialized_is_conflict
#[test]
fn seal_when_uninitialized_is_conflict() {
    let reply = call(&handler_for(rig().service), "seal", json!({}));
    assert!(code(&reply) == "conflict" && reply["data"] == json!({"state": "uninitialized"}));
}
