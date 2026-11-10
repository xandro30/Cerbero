//! T099: `ca.intermediate.begin`, `ca.intermediate.confirm` and `ca.intermediate.abort` on the
//! local channel (contracts/control-protocol.md): the exact fields of `data`, `validation` and
//! `conflict`, and the rows `ca_intermediate_renew` with the reason of each `abort`.

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::instant::parse_iso;
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use hecate_infrastructure::pki::RootCaKey;
use serde_json::{Value, json};
use time::Duration;

fn keys(data: &Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

struct WithCa {
    rig: CaRig,
    handler: RequestHandler,
    changed: Arc<AtomicUsize>,
    root_key: RootCaKey,
    root_der: Vec<u8>,
}

fn with_ca() -> WithCa {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    let (_, root_key, root_der) = create_ca_keeping_root(&handler, &rig);
    WithCa {
        rig,
        handler,
        changed,
        root_key,
        root_der,
    }
}

fn begin(handler: &RequestHandler) -> Value {
    call(handler, "ca.intermediate.begin", json!({}))
}

fn confirm(handler: &RequestHandler, begun: &Value, certificate: &str) -> Value {
    call(
        handler,
        "ca.intermediate.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "intermediate_certificate": certificate,
        }),
    )
}

fn abort(handler: &RequestHandler, begun: &Value, reason: &str) -> Value {
    call(
        handler,
        "ca.intermediate.abort",
        json!({"ceremony_id": begun["data"]["ceremony_id"], "reason": reason}),
    )
}

fn csr(begun: &Value) -> &str {
    begun["data"]["intermediate_csr"].as_str().unwrap()
}

fn rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaIntermediateRenew)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn row(result: AccessResult, detail: &str) -> (AccessResult, String) {
    (result, detail.to_string())
}

#[test]
fn the_renewal_returns_the_fields_of_the_contract() {
    let env = with_ca();
    let begun = begin(&env.handler);
    assert_eq!(begun["status"], "completed", "{begun}");
    assert_eq!(
        keys(&begun["data"]),
        [
            "ceremony_id",
            "expires_at",
            "intermediate_csr",
            "root_certificate"
        ]
    );
    assert_eq!(
        begun["data"]["root_certificate"],
        pem::encode("CERTIFICATE", &env.root_der)
    );
    assert!(parse_iso(begun["data"]["expires_at"].as_str().unwrap()).is_ok());
    let before = changes(&env.changed);
    let certificate = sign_renewal(&env.rig, &env.root_key, &env.root_der, csr(&begun));
    let confirmed = confirm(&env.handler, &begun, &certificate);
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    assert_eq!(
        keys(&confirmed["data"]),
        ["intermediate_not_after", "intermediate_version"]
    );
    assert_eq!(confirmed["data"]["intermediate_version"], 2);
    assert_eq!(
        parse_iso(
            confirmed["data"]["intermediate_not_after"]
                .as_str()
                .unwrap()
        )
        .unwrap(),
        env.rig.clock.now() + Duration::days(365)
    );
    assert_eq!(
        changes(&env.changed),
        before + 1,
        "the API may present more"
    );
    let again = begin(&env.handler);
    let aborted = abort(&env.handler, &again, "cancelled");
    assert_eq!(aborted["status"], "completed", "{aborted}");
    assert_eq!(aborted["data"], json!({}));
    assert_eq!(
        rows(&env.rig),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "confirm"),
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: cancelled"),
        ]
    );
}

#[test]
fn an_intermediate_that_does_not_follow_the_profile_is_validation() {
    let env = with_ca();
    let begun = begin(&env.handler);
    let (foreign_root, _, foreign_key) = sign_request(&env.rig, csr(&begun));
    let foreign_der = pem::single(&foreign_root, "CERTIFICATE").unwrap();
    let by_another_root = sign_renewal(&env.rig, &foreign_key, &foreign_der, csr(&begun));
    let refused = confirm(&env.handler, &begun, &by_another_root);
    assert_eq!(code(&refused), "validation", "{refused}");
    let not_pem = confirm(&env.handler, &begun, "not a certificate");
    assert_eq!(code(&not_pem), "validation", "{not_pem}");
    assert_eq!(
        rows(&env.rig)[1..],
        [
            row(AccessResult::InvalidInput, "confirm"),
            row(AccessResult::InvalidInput, "confirm"),
        ]
    );
}

#[test]
fn the_renewal_conflicts_when_sealed_without_a_ca_or_with_another_ceremony() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    assert_eq!(code(&begin(&handler)), "conflict");

    let env = with_ca();
    call(&env.handler, "seal", json!({}));
    assert_eq!(code(&begin(&env.handler)), "conflict");
    env.rig
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    let begun = begin(&env.handler);
    assert_eq!(code(&begin(&env.handler)), "conflict");
    let unknown = json!({"data": {"ceremony_id": "unknown"}});
    let certificate = sign_renewal(&env.rig, &env.root_key, &env.root_der, csr(&begun));
    assert_eq!(
        code(&confirm(&env.handler, &unknown, &certificate)),
        "conflict"
    );
    env.rig.clock.advance(Duration::minutes(16));
    assert_eq!(
        code(&confirm(&env.handler, &begun, &certificate)),
        "conflict"
    );
    assert_eq!(code(&abort(&env.handler, &begun, "cancelled")), "conflict");
}

#[test]
fn each_abort_leaves_its_reason_and_malformed_arguments_their_row() {
    let env = with_ca();
    for reason in ["wrong_passphrase", "wrong_root_key"] {
        let begun = begin(&env.handler);
        assert_eq!(abort(&env.handler, &begun, reason)["status"], "completed");
    }
    let begun = begin(&env.handler);
    assert_eq!(
        code(&abort(&env.handler, &begun, "backup_check_failed")),
        "validation"
    );
    let missing = call(
        &env.handler,
        "ca.intermediate.confirm",
        json!({"ceremony_id": "x"}),
    );
    assert_eq!(code(&missing), "validation");
    assert_eq!(
        rows(&env.rig),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: wrong_passphrase"),
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: wrong_root_key"),
            row(AccessResult::Success, "begin"),
            row(AccessResult::InvalidInput, "abort"),
            row(AccessResult::InvalidInput, "confirm"),
        ]
    );
    assert_eq!(
        env.rig.sealing.status().unwrap().consecutive_failures,
        0,
        "no abort counts for the common counter"
    );
}
