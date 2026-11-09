mod common;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, initialize};
use serde_json::json;

// py: contract/test_unseal_ops.py::test_unseal_with_passphrase_completes
#[test]
fn unseal_with_passphrase_completes() {
    let (_rig, handler, _) = initialized_handler();
    let reply = call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    assert_eq!(reply["status"], "completed");
    assert_eq!(
        (
            reply["data"]["state"].as_str(),
            reply["data"]["unsealed_via"].as_str()
        ),
        (Some("unsealed"), Some("passphrase"))
    );
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["state"],
        "unsealed"
    );
}

// py: contract/test_unseal_ops.py::test_wrong_passphrase_is_auth_not_retryable
#[test]
fn wrong_passphrase_is_auth_not_retryable() {
    let (_rig, handler, _) = initialized_handler();
    let reply = call(
        &handler,
        "unseal",
        json!({"passphrase": "wrong wrong wrong"}),
    );
    assert_eq!(reply["status"], "failed");
    assert!(code(&reply) == "auth" && reply["error"]["retryable"] == false);
    assert!(!reply.to_string().contains("wrong wrong wrong"));
}

// py: contract/test_unseal_ops.py::test_rate_limited_carries_retry_after_s
#[test]
fn rate_limited_carries_retry_after_s() {
    let (_rig, handler, _) = initialized_handler();
    for _ in 0..3 {
        call(
            &handler,
            "unseal",
            json!({"passphrase": "wrong wrong wrong"}),
        );
    }
    let reply = call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    assert_eq!(code(&reply), "rate_limited");
    assert_eq!(reply["error"]["retryable"], true);
    assert_eq!(reply["error"]["retry_after_s"], 60);
}

// py: contract/test_unseal_ops.py::test_unseal_when_unsealed_is_conflict_with_state
#[test]
fn unseal_when_unsealed_is_conflict_with_state() {
    let (_rig, handler, _) = initialized_handler();
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let reply = call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    assert_eq!(code(&reply), "conflict");
    assert_eq!(reply["data"], json!({"state": "unsealed"}));
}

// py: contract/test_unseal_ops.py::test_unseal_needs_exactly_one_proof
#[test]
fn unseal_needs_exactly_one_proof() {
    let (_rig, handler, _) = initialized_handler();
    assert_eq!(code(&call(&handler, "unseal", json!({}))), "validation");
    let both = json!({"passphrase": GOOD_PASSPHRASE, "shares": ["x", "y"]});
    assert_eq!(code(&call(&handler, "unseal", both)), "validation");
    assert_eq!(
        code(&call(&handler, "unseal", json!({"passphrase": 5}))),
        "validation"
    );
}

// py: contract/test_unseal_ops.py::test_sealed_rejects_every_other_operation
#[test]
fn sealed_rejects_every_other_operation() {
    let other_ops = [
        ("init.begin", json!({"passphrase": GOOD_PASSPHRASE})),
        (
            "init.confirm",
            json!({"init_id": "x", "shares": ["a", "b"]}),
        ),
        ("init.abort", json!({"init_id": "x"})),
        ("seal", json!({})),
        (
            "passphrase.change",
            json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": "a brand new passphrase"}),
        ),
        (
            "shares.rekey.begin",
            json!({"proof": {"passphrase": GOOD_PASSPHRASE}}),
        ),
        (
            "shares.rekey.confirm",
            json!({"rekey_id": "x", "shares": ["a", "b"]}),
        ),
        ("shares.rekey.abort", json!({"rekey_id": "x"})),
    ];
    for (op, args) in other_ops {
        let (_rig, handler, _) = initialized_handler();
        let reply = call(&handler, op, args);
        assert_eq!(reply["status"], "failed", "{op}");
        assert_eq!(code(&reply), "conflict", "{op}");
        assert_eq!(reply["data"], json!({"state": "sealed"}), "{op}");
    }
}

// py: contract/test_unseal_ops.py::test_malformed_arguments_leave_an_invalid_input_row
#[test]
fn malformed_arguments_leave_an_invalid_input_row() {
    let malformed = [
        ("init.begin", json!({}), "init"),
        ("init.confirm", json!({"init_id": 1, "shares": []}), "init"),
        ("unseal", json!({}), "unseal"),
        ("unseal", json!({"passphrase": 5}), "unseal"),
        ("unseal", json!({"shares": "x"}), "unseal"),
        (
            "passphrase.change",
            json!({"proof": "x", "new_passphrase": "y"}),
            "passphrase_change",
        ),
        ("shares.rekey.begin", json!({}), "shares_rekey"),
        (
            "shares.rekey.confirm",
            json!({"rekey_id": 1}),
            "shares_rekey",
        ),
        ("shares.rekey.abort", json!({}), "shares_rekey"),
    ];
    for (op, args, operation) in malformed {
        let (rig, handler, _) = initialized_handler();
        let before = rig.log.entries().len();
        let reply = call(&handler, op, args);
        assert_eq!(code(&reply), "validation", "{op}");
        let added: Vec<(String, String)> = rig.log.entries()[before..]
            .iter()
            .map(|entry| {
                (
                    entry.operation.as_str().to_string(),
                    entry.result.as_str().to_string(),
                )
            })
            .collect();
        assert_eq!(
            added,
            vec![(operation.to_string(), "invalid_input".to_string())],
            "{op}"
        );
        assert!(
            rig.log.entries()[before..]
                .iter()
                .all(|entry| entry.detail != "x")
        );
    }
}

// py: contract/test_unseal_ops.py::test_unknown_operations_and_status_leave_no_row
#[test]
fn unknown_operations_and_status_leave_no_row() {
    let rig = rig();
    let handler = handler_for(rig.service.clone());
    call(&handler, "status", json!({}));
    call(&handler, "no.such.op", json!({}));
    assert!(rig.log.entries().is_empty());
    let _ = initialize;
}
