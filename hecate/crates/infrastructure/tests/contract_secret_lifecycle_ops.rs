mod common;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, SecretsRig};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};
use time::Duration;

const PATH: &str = "secret://hermes/telegram/bot_token";

fn right() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

fn wrong() -> Value {
    json!({"passphrase": "wrong wrong wrong"})
}

/// Version 1 deprecated, version 2 active.
fn env() -> (SecretsRig, RequestHandler) {
    let rig = secrets_rig(true);
    let handler = secrets_handler(&rig);
    call(
        &handler,
        "secrets.create",
        json!({"path": PATH, "value": "value-one", "value_type": "string"}),
    );
    call(
        &handler,
        "secrets.add_version",
        json!({"path": PATH, "value": "value-two", "expected_version": 2}),
    );
    (rig, handler)
}

/// Version 1 revoked, version 2 active.
fn revoked() -> (SecretsRig, RequestHandler) {
    let (rig, handler) = env();
    let reply = call(
        &handler,
        "secrets.revoke",
        json!({"path": PATH, "version": 1, "expected_version": 4}),
    );
    assert_eq!(reply["status"], "completed");
    (rig, handler)
}

fn stream(handler: &RequestHandler) -> u64 {
    call(handler, "secrets.get", json!({"path": PATH}))["data"]["stream_version"]
        .as_u64()
        .unwrap()
}

fn state(handler: &RequestHandler, version: u64) -> String {
    let data = call(handler, "secrets.get", json!({"path": PATH}))["data"].clone();
    data["versions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["version"] == version)
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

fn destroy(handler: &RequestHandler, proof: Value, version: u64, expected: u64) -> Value {
    call(
        handler,
        "secrets.destroy",
        json!({"path": PATH, "version": version, "expected_version": expected, "proof": proof}),
    )
}

const OPS: [&str; 3] = ["secrets.revoke", "secrets.restore", "secrets.promote"];

// py: contract/test_secret_lifecycle_ops.py::test_revoke_restore_and_promote_reply_with_the_stream_version
#[test]
fn revoke_restore_and_promote_reply_with_the_stream_version() {
    let (_rig, handler) = env();
    for (op, expected_state) in [
        ("secrets.revoke", "revoked"),
        ("secrets.restore", "deprecated"),
        ("secrets.promote", "active"),
    ] {
        let before = stream(&handler);
        let reply = call(
            &handler,
            op,
            json!({"path": PATH, "version": 1, "expected_version": before}),
        );
        assert!(
            reply["status"] == "completed" && reply["error"].is_null(),
            "{op} {reply}"
        );
        let data = reply["data"].as_object().unwrap();
        assert!(data.len() == 1 && data["stream_version"].as_u64().unwrap() > before);
        assert_eq!(state(&handler, 1), expected_state);
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_lifecycle_validation
#[test]
fn lifecycle_validation() {
    for op in OPS {
        for args in [
            json!({"version": 1, "expected_version": 4}),
            json!({"path": PATH, "expected_version": 4}),
            json!({"path": PATH, "version": "1", "expected_version": 4}),
            json!({"path": PATH, "version": 0, "expected_version": 4}),
            json!({"path": PATH, "version": 1}),
            json!({"path": PATH, "version": 1, "expected_version": "4"}),
            json!({"path": 5, "version": 1, "expected_version": 4}),
            json!({"path": "nonsense", "version": 1, "expected_version": 4}),
        ] {
            let (_rig, handler) = env();
            let reply = call(&handler, op, args.clone());
            assert!(
                reply["status"] == "failed" && code(&reply) == "validation",
                "{op} {args}"
            );
            assert_eq!(stream(&handler), 4);
        }
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_lifecycle_not_found
#[test]
fn lifecycle_not_found() {
    for op in OPS {
        let (_rig, handler) = env();
        assert_eq!(
            code(&call(
                &handler,
                op,
                json!({"path": PATH, "version": 9, "expected_version": 4})
            )),
            "not_found"
        );
        let missing =
            json!({"path": "secret://hermes/missing", "version": 1, "expected_version": 1});
        assert_eq!(code(&call(&handler, op, missing)), "not_found");
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_lifecycle_transition_not_allowed_is_a_conflict
#[test]
fn lifecycle_transition_not_allowed_is_a_conflict() {
    for (op, version) in [
        ("secrets.restore", 1),
        ("secrets.restore", 2),
        ("secrets.promote", 2),
    ] {
        let (_rig, handler) = env();
        let reply = call(
            &handler,
            op,
            json!({"path": PATH, "version": version, "expected_version": 4}),
        );
        assert!(
            reply["status"] == "failed" && code(&reply) == "conflict",
            "{op} {version}"
        );
        assert!(reply["error"]["retryable"] == false && reply["data"] == json!({}));
        assert_eq!(stream(&handler), 4);
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_lifecycle_stale_version_is_a_conflict_with_the_current_one
#[test]
fn lifecycle_stale_version_is_a_conflict_with_the_current_one() {
    for op in OPS {
        let (_rig, handler) = env();
        let reply = call(
            &handler,
            op,
            json!({"path": PATH, "version": 1, "expected_version": 3}),
        );
        assert!(
            code(&reply) == "conflict" && reply["data"] == json!({"current_version": 4}),
            "{op}"
        );
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_lifecycle_while_sealed_conflicts_with_the_state
#[test]
fn lifecycle_while_sealed_conflicts_with_the_state() {
    for op in [
        "secrets.revoke",
        "secrets.restore",
        "secrets.promote",
        "secrets.destroy",
    ] {
        let (rig, handler) = env();
        rig.sealing.seal().unwrap();
        let reply = call(
            &handler,
            op,
            json!({"path": PATH, "version": 1, "expected_version": 4, "proof": right()}),
        );
        assert!(
            code(&reply) == "conflict" && reply["data"] == json!({"state": "sealed"}),
            "{op}"
        );
    }
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_with_the_passphrase
#[test]
fn destroy_with_the_passphrase() {
    let (_rig, handler) = revoked();
    let reply = destroy(&handler, right(), 1, 5);
    assert!(reply["status"] == "completed" && reply["data"] == json!({"stream_version": 6}));
    assert_eq!(state(&handler, 1), "destroyed");
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_with_two_shares
#[test]
fn destroy_with_two_shares() {
    let (rig, handler) = revoked();
    assert_eq!(
        destroy(&handler, json!({"shares": &rig.shares[..2]}), 1, 5)["status"],
        "completed"
    );
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_with_a_wrong_proof_is_auth
#[test]
fn destroy_with_a_wrong_proof_is_auth() {
    let (_rig, handler) = revoked();
    let reply = destroy(&handler, wrong(), 1, 5);
    assert!(reply["status"] == "failed" && code(&reply) == "auth");
    assert!(reply["error"]["retryable"] == false && reply["data"] == json!({}));
    assert_eq!(state(&handler, 1), "revoked");
    assert!(!reply.to_string().contains("wrong wrong"));
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_during_a_wait_is_rate_limited
#[test]
fn destroy_during_a_wait_is_rate_limited() {
    let (rig, handler) = revoked();
    for _ in 0..3 {
        destroy(&handler, wrong(), 1, 5);
    }
    let reply = destroy(&handler, right(), 1, 5);
    assert!(code(&reply) == "rate_limited" && reply["error"]["retryable"] == true);
    assert_eq!(reply["error"]["retry_after_s"], 60);
    rig.clock.advance(Duration::seconds(61));
    assert_eq!(destroy(&handler, right(), 1, 5)["status"], "completed");
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_of_a_version_that_is_not_revoked_is_a_conflict
#[test]
fn destroy_of_a_version_that_is_not_revoked_is_a_conflict() {
    let (_rig, handler) = revoked();
    assert_eq!(code(&destroy(&handler, right(), 2, 5)), "conflict");
    assert_eq!(state(&handler, 2), "active");
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_of_a_destroyed_version_is_a_conflict
#[test]
fn destroy_of_a_destroyed_version_is_a_conflict() {
    let (_rig, handler) = revoked();
    destroy(&handler, right(), 1, 5);
    assert_eq!(code(&destroy(&handler, right(), 1, 6)), "conflict");
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_stale_version_is_a_conflict_with_the_current_one
#[test]
fn destroy_stale_version_is_a_conflict_with_the_current_one() {
    let (_rig, handler) = revoked();
    let reply = destroy(&handler, right(), 1, 4);
    assert!(code(&reply) == "conflict" && reply["data"] == json!({"current_version": 5}));
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_not_found
#[test]
fn destroy_not_found() {
    let (_rig, handler) = revoked();
    assert_eq!(code(&destroy(&handler, right(), 9, 5)), "not_found");
    let missing = json!({"path": "secret://hermes/missing", "version": 1, "expected_version": 1, "proof": right()});
    assert_eq!(
        code(&call(&handler, "secrets.destroy", missing)),
        "not_found"
    );
}

// py: contract/test_secret_lifecycle_ops.py::test_destroy_validation_does_not_spend_an_attempt
#[test]
fn destroy_validation_does_not_spend_an_attempt() {
    for args in [
        json!({"version": 1, "expected_version": 5, "proof": right()}),
        json!({"path": PATH, "expected_version": 5, "proof": right()}),
        json!({"path": PATH, "version": 1, "proof": right()}),
        json!({"path": PATH, "version": 1, "expected_version": 5}),
        json!({"path": PATH, "version": 1, "expected_version": 5, "proof": "secret"}),
        json!({"path": PATH, "version": 1, "expected_version": 5, "proof": {}}),
        json!({"path": PATH, "version": 1, "expected_version": 5, "proof": {"passphrase": ""}}),
        json!({"path": PATH, "version": 1, "expected_version": 5, "proof": {"shares": ["only-one"]}}),
    ] {
        let (rig, handler) = revoked();
        let reply = call(&handler, "secrets.destroy", args.clone());
        assert!(
            reply["status"] == "failed" && code(&reply) == "validation",
            "{args}"
        );
        assert_eq!(rig.sealing.status().unwrap().consecutive_failures, 0);
        assert_eq!(state(&handler, 1), "revoked");
    }
}
