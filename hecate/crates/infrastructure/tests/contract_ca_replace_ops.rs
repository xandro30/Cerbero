//! T108: `ca.replace.begin`, `ca.replace.confirm` and `ca.replace.abort` on the local channel
//! (contracts/control-protocol.md): the exact fields of `data`, `auth` and `rate_limited` with the
//! common counter, `validation`, `conflict` and the rows `ca_replace`.

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use common::*;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};
use time::Duration;

const NEW_CA_PASSPHRASE: &str = "harbor violet engine 77";

fn keys(data: &Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn with_ca() -> (CaRig, RequestHandler, Arc<AtomicUsize>) {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    (rig, handler, changed)
}

fn begin_with(handler: &RequestHandler, passphrase: &str, ca_passphrase: &str) -> Value {
    call(
        handler,
        "ca.replace.begin",
        json!({"proof": {"passphrase": passphrase}, "ca_passphrase": ca_passphrase}),
    )
}

fn begin(handler: &RequestHandler) -> Value {
    begin_with(handler, GOOD_PASSPHRASE, NEW_CA_PASSPHRASE)
}

fn confirm(handler: &RequestHandler, begun: &Value, root: &str, intermediate: &str) -> Value {
    call(
        handler,
        "ca.replace.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    )
}

fn csr(begun: &Value) -> &str {
    begun["data"]["intermediate_csr"].as_str().unwrap()
}

fn rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaReplace)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn row(result: AccessResult, detail: &str) -> (AccessResult, String) {
    (result, detail.to_string())
}

#[test]
fn the_replacement_returns_the_fields_of_the_contract() {
    let (rig, handler, changed) = with_ca();
    let begun = begin(&handler);
    assert_eq!(begun["status"], "completed", "{begun}");
    assert_eq!(
        keys(&begun["data"]),
        ["ceremony_id", "expires_at", "intermediate_csr"]
    );
    let before = changes(&changed);
    let (root, intermediate, _) = sign_request(&rig, csr(&begun));
    let confirmed = confirm(&handler, &begun, &root, &intermediate);
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    assert_eq!(
        keys(&confirmed["data"]),
        ["generation", "root_fingerprint", "trust_domain"]
    );
    assert_eq!(confirmed["data"]["generation"], 2);
    assert_eq!(confirmed["data"]["trust_domain"], TRUST_DOMAIN);
    let status = call(&handler, "status", json!({}));
    assert_eq!(
        confirmed["data"]["root_fingerprint"],
        status["data"]["ca"]["root_fingerprint"]
    );
    assert_eq!(changes(&changed), before + 1, "the API presents the new CA");
    let again = begin(&handler);
    let aborted = call(
        &handler,
        "ca.replace.abort",
        json!({"ceremony_id": again["data"]["ceremony_id"], "reason": "backup_check_failed"}),
    );
    assert_eq!(aborted["data"], json!({}), "{aborted}");
    assert_eq!(
        rows(&rig),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "confirm"),
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: backup_check_failed"),
        ]
    );
}

#[test]
fn a_wrong_proof_is_auth_counts_and_then_waits() {
    let (rig, handler, _) = with_ca();
    for _ in 0..3 {
        let refused = begin_with(&handler, "not the passphrase", NEW_CA_PASSPHRASE);
        assert_eq!(code(&refused), "auth", "{refused}");
    }
    let status = call(&handler, "status", json!({}));
    assert_eq!(status["data"]["consecutive_failures"], 3);
    let waiting = begin(&handler);
    assert_eq!(code(&waiting), "rate_limited", "{waiting}");
    assert_eq!(waiting["error"]["retry_after_s"], 60);
    assert_eq!(
        rows(&rig),
        vec![
            row(AccessResult::Failure, "begin"),
            row(AccessResult::Failure, "begin"),
            row(AccessResult::Failure, "begin"),
            row(AccessResult::RejectedRateLimited, "begin"),
        ]
    );
}

#[test]
fn the_ca_passphrase_and_the_certificates_are_validated() {
    let (rig, handler, _) = with_ca();
    assert_eq!(
        code(&begin_with(&handler, GOOD_PASSPHRASE, "too short")),
        "validation"
    );
    assert_eq!(
        code(&begin_with(&handler, GOOD_PASSPHRASE, GOOD_PASSPHRASE)),
        "validation"
    );
    let missing = call(
        &handler,
        "ca.replace.begin",
        json!({"ca_passphrase": NEW_CA_PASSPHRASE}),
    );
    assert_eq!(code(&missing), "validation");
    let begun = begin(&handler);
    let (root, intermediate, _) = sign_request(&rig, csr(&begun));
    let swapped = confirm(&handler, &begun, &intermediate, &root);
    assert_eq!(code(&swapped), "validation", "{swapped}");
    assert_eq!(
        rows(&rig),
        vec![
            row(AccessResult::InvalidInput, "begin"),
            row(AccessResult::InvalidInput, "begin"),
            row(AccessResult::InvalidInput, "begin"),
            row(AccessResult::Success, "begin"),
            row(AccessResult::InvalidInput, "confirm"),
        ]
    );
}

#[test]
fn the_replacement_conflicts_sealed_without_a_ca_or_with_another_ceremony() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    assert_eq!(code(&begin(&handler)), "conflict");

    let (rig, handler, _) = with_ca();
    call(&handler, "seal", json!({}));
    assert_eq!(code(&begin(&handler)), "conflict");
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let begun = begin(&handler);
    assert_eq!(code(&begin(&handler)), "conflict");
    let (root, intermediate, _) = sign_request(&rig, csr(&begun));
    let unknown = json!({"data": {"ceremony_id": "unknown"}});
    assert_eq!(
        code(&confirm(&handler, &unknown, &root, &intermediate)),
        "conflict"
    );
    rig.clock.advance(Duration::minutes(16));
    assert_eq!(
        code(&confirm(&handler, &begun, &root, &intermediate)),
        "conflict"
    );
}
