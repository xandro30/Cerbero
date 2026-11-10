//! T035: `ca.create.begin`, `ca.create.confirm` and `ca.create.abort` on the local channel
//! (contracts/control-protocol.md): the exact fields of `data`, `validation` and `conflict`, and
//! the rows `ca_create`.

mod common;

use common::*;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::instant::parse_iso;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};
use time::Duration;

fn keys(data: &Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn begin(handler: &RequestHandler) -> Value {
    call(
        handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    )
}

fn rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaCreate)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

/// An instant of the channel: `isoformat` in UTC, as in the change 005.
fn is_instant(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|text| parse_iso(text).is_ok() && text.ends_with("+00:00"))
}

#[test]
fn the_ceremony_returns_the_fields_of_the_contract() {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    let begun = begin(&handler);
    assert!(
        begun["status"] == "completed" && begun["error"].is_null(),
        "{begun}"
    );
    assert_eq!(
        keys(&begun["data"]),
        ["ceremony_id", "expires_at", "intermediate_csr"]
    );
    assert!(is_instant(&begun["data"]["expires_at"]), "{begun}");
    assert!(
        begun["data"]["intermediate_csr"]
            .as_str()
            .unwrap()
            .starts_with("-----BEGIN CERTIFICATE REQUEST-----\n")
    );
    let (root, intermediate, _) =
        sign_request(&rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let confirmed = call(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    );
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    assert_eq!(
        keys(&confirmed["data"]),
        ["intermediate_not_after", "root_fingerprint", "trust_domain"]
    );
    assert_eq!(confirmed["data"]["trust_domain"], TRUST_DOMAIN);
    let fingerprint = confirmed["data"]["root_fingerprint"].as_str().unwrap();
    assert!(
        fingerprint.starts_with("sha256:") && fingerprint.len() == 71,
        "{fingerprint}"
    );
    assert!(is_instant(&confirmed["data"]["intermediate_not_after"]));
    assert_eq!(changes(&changed), 1);
}

#[test]
fn abort_returns_no_data_and_records_its_reason() {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    let begun = begin(&handler);
    let aborted = call(
        &handler,
        "ca.create.abort",
        json!({"ceremony_id": begun["data"]["ceremony_id"], "reason": "backup_check_failed"}),
    );
    assert!(
        aborted["status"] == "completed" && aborted["data"] == json!({}),
        "{aborted}"
    );
    let cancelled = begin(&handler);
    call(
        &handler,
        "ca.create.abort",
        json!({"ceremony_id": cancelled["data"]["ceremony_id"], "reason": "cancelled"}),
    );
    assert_eq!(
        rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (
                AccessResult::Success,
                "abort: backup_check_failed".to_string()
            ),
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "abort: cancelled".to_string()),
        ]
    );
    assert_eq!(changes(&changed), 0);
}

#[test]
fn malformed_input_is_validation_and_leaves_a_row() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    for (op, args) in [
        (
            "ca.create.begin",
            json!({"trust_domain": "Bad Domain", "ca_passphrase": CA_PASSPHRASE}),
        ),
        (
            "ca.create.begin",
            json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": "short"}),
        ),
        (
            "ca.create.begin",
            json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": GOOD_PASSPHRASE}),
        ),
        ("ca.create.begin", json!({"trust_domain": TRUST_DOMAIN})),
        (
            "ca.create.begin",
            json!({"trust_domain": 7, "ca_passphrase": CA_PASSPHRASE}),
        ),
        (
            "ca.create.confirm",
            json!({"ceremony_id": "x", "root_certificate": "x"}),
        ),
        ("ca.create.abort", json!({"ceremony_id": "x"})),
    ] {
        let reply = call(&handler, op, args.clone());
        assert_eq!(code(&reply), "validation", "{op} {args}: {reply}");
        assert_eq!(reply["error"]["retryable"], false);
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(!message.contains(CA_PASSPHRASE) && !message.contains(GOOD_PASSPHRASE));
    }
    let begun = begin(&handler);
    let bad_reason = call(
        &handler,
        "ca.create.abort",
        json!({"ceremony_id": begun["data"]["ceremony_id"], "reason": "bored"}),
    );
    assert_eq!(code(&bad_reason), "validation");
    let not_the_profile = call(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n",
            "intermediate_certificate": "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n",
        }),
    );
    assert_eq!(code(&not_the_profile), "validation");
    let invalid_rows = rows(&rig)
        .into_iter()
        .filter(|(result, _)| *result == AccessResult::InvalidInput)
        .count();
    assert_eq!(invalid_rows, 9);
}

#[test]
fn the_state_conflicts_are_conflict() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    call(&handler, "seal", json!({}));
    assert_eq!(code(&begin(&handler)), "conflict");
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let begun = begin(&handler);
    assert_eq!(code(&begin(&handler)), "conflict");
    let (root, intermediate, _) =
        sign_request(&rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let unknown = call(
        &handler,
        "ca.create.confirm",
        json!({"ceremony_id": "unknown", "root_certificate": root, "intermediate_certificate": intermediate}),
    );
    assert_eq!(code(&unknown), "conflict");
    rig.clock.advance(Duration::minutes(16));
    let expired = call(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    );
    assert_eq!(code(&expired), "conflict");
    assert_eq!(create_ca_through(&handler, &rig)["status"], "completed");
    assert_eq!(code(&begin(&handler)), "conflict");
}

#[test]
fn unsealing_also_says_that_the_api_may_change_and_sealing_forgets_the_ceremony() {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    let begun = begin(&handler);
    call(&handler, "seal", json!({}));
    assert_eq!(changes(&changed), 0);
    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    assert_eq!(changes(&changed), 1);
    let (root, intermediate, _) =
        sign_request(&rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let lost = call(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    );
    assert_eq!(code(&lost), "conflict");
    assert_eq!(changes(&changed), 1);
}
