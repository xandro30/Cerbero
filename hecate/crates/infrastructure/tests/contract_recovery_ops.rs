mod common;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, initialize};
use serde_json::json;

const NEW: &str = "a brand new passphrase";

// py: contract/test_recovery_ops.py::test_unseal_with_two_shares
#[test]
fn unseal_with_two_shares() {
    let (_rig, handler, shares) = initialized_handler();
    let reply = call(&handler, "unseal", json!({"shares": &shares[..2]}));
    assert_eq!(reply["status"], "completed");
    assert_eq!(
        reply["data"],
        json!({"state": "unsealed", "unsealed_via": "recovery", "recommend_passphrase_change": true})
    );
}

// py: contract/test_recovery_ops.py::test_unseal_with_bad_shares_codes
#[test]
fn unseal_with_bad_shares_codes() {
    let (_rig, handler, shares) = initialized_handler();
    assert_eq!(
        code(&call(&handler, "unseal", json!({"shares": &shares[..1]}))),
        "validation"
    );
    assert_eq!(
        code(&call(&handler, "unseal", json!({"shares": "x"}))),
        "validation"
    );
    let foreign = initialize(&rig().service, GOOD_PASSPHRASE);
    let reply = call(&handler, "unseal", json!({"shares": &foreign[..2]}));
    assert!(code(&reply) == "auth" && reply["error"]["retryable"] == false);
}

// py: contract/test_recovery_ops.py::test_passphrase_change_completes_with_either_proof
#[test]
fn passphrase_change_completes_with_either_proof() {
    let (_rig, handler, shares) = initialized_handler();
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let reply = call(
        &handler,
        "passphrase.change",
        json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW}),
    );
    assert!(reply["status"] == "completed" && reply["data"] == json!({"config_version": 2}));
    let reply = call(
        &handler,
        "passphrase.change",
        json!({"proof": {"shares": &shares[..2]}, "new_passphrase": GOOD_PASSPHRASE}),
    );
    assert_eq!(reply["data"], json!({"config_version": 3}));
}

// py: contract/test_recovery_ops.py::test_passphrase_change_error_codes
#[test]
fn passphrase_change_error_codes() {
    let (_rig, handler, _) = initialized_handler();
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let ok = json!({"passphrase": GOOD_PASSPHRASE});
    let short = json!({"proof": ok, "new_passphrase": "short"});
    assert_eq!(
        code(&call(&handler, "passphrase.change", short)),
        "validation"
    );
    assert_eq!(
        code(&call(
            &handler,
            "passphrase.change",
            json!({"new_passphrase": NEW})
        )),
        "validation"
    );
    let not_object = json!({"proof": "x", "new_passphrase": NEW});
    assert_eq!(
        code(&call(&handler, "passphrase.change", not_object)),
        "validation"
    );
    let wrong = json!({"proof": {"passphrase": "nope nope nope"}, "new_passphrase": NEW});
    let bad = call(&handler, "passphrase.change", wrong);
    assert_eq!(code(&bad), "auth");
    assert!(!bad.to_string().contains("nope nope nope") && !bad.to_string().contains(NEW));
}

// py: contract/test_recovery_ops.py::test_passphrase_change_while_sealed_is_conflict
#[test]
fn passphrase_change_while_sealed_is_conflict() {
    let (_rig, handler, _) = initialized_handler();
    let args = json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW});
    let reply = call(&handler, "passphrase.change", args);
    assert!(code(&reply) == "conflict" && reply["data"] == json!({"state": "sealed"}));
}
