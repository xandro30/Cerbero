//! T025: the operations `policies.*` on the local channel (contracts/control-protocol.md): the
//! exact fields of `data`, the repeated orders, the errors and the row of each one.

mod common;

use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::CaRig;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};

const SMTP: &str = "secret://hades/smtp/password";
const OWN: &str = "secret://hermes/own/key";

fn with_secrets() -> (CaRig, RequestHandler) {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    store_secret(&rig, SMTP, "smtp-value");
    store_secret(&rig, OWN, "own-value");
    rig.access_log.clear();
    (rig, handler)
}

type Row = (
    Operation,
    AccessResult,
    String,
    Option<String>,
    Option<String>,
);

/// The rows of the Access Policy: operation, result, detail, identity and secret; the admin
/// sends them all.
fn rows(rig: &CaRig) -> Vec<Row> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.operation,
                Operation::PolicyGrant
                    | Operation::PolicyRevoke
                    | Operation::PolicyList
                    | Operation::PolicyGet
            )
        })
        .inspect(|entry| assert_eq!(entry.subject.as_deref(), Some("admin")))
        .map(|entry| {
            (
                entry.operation,
                entry.result,
                entry.detail,
                entry.identity,
                entry.path,
            )
        })
        .collect()
}

fn row(operation: Operation, detail: &str, identity: &str, path: Option<&str>) -> Row {
    (
        operation,
        AccessResult::Success,
        detail.to_string(),
        Some(identity.to_string()),
        path.map(str::to_string),
    )
}

fn data(handler: &RequestHandler, op: &str, args: Value) -> Value {
    let reply = call(handler, op, args);
    assert_eq!(reply["status"], "completed", "{op}: {reply}");
    reply["data"].clone()
}

#[test]
fn the_orders_return_the_fields_of_the_contract() {
    let (rig, handler) = with_secrets();
    let read = json!({"identity": "hermes", "secret": SMTP});
    let status = json!({"identity": "hermes"});
    for (op, args, expected) in [
        (
            "policies.grant_read",
            &read,
            json!({"identity": "hermes", "secret": SMTP, "version": 1, "changed": true}),
        ),
        (
            "policies.grant_read",
            &read,
            json!({"identity": "hermes", "secret": SMTP, "version": 1, "changed": false}),
        ),
        (
            "policies.grant_status",
            &status,
            json!({"identity": "hermes", "version": 2, "changed": true}),
        ),
        (
            "policies.revoke_status",
            &status,
            json!({"identity": "hermes", "version": 3, "changed": true}),
        ),
        (
            "policies.revoke_status",
            &status,
            json!({"identity": "hermes", "version": 3, "changed": false}),
        ),
        (
            "policies.revoke_read",
            &read,
            json!({"identity": "hermes", "secret": SMTP, "version": 4, "changed": true}),
        ),
    ] {
        assert_eq!(data(&handler, op, args.clone()), expected, "{op}");
    }
    assert_eq!(
        rows(&rig),
        [
            row(Operation::PolicyGrant, "read", "hermes", Some(SMTP)),
            row(Operation::PolicyGrant, "read", "hermes", Some(SMTP)),
            row(Operation::PolicyGrant, "status", "hermes", None),
            row(Operation::PolicyRevoke, "status", "hermes", None),
            row(Operation::PolicyRevoke, "status", "hermes", None),
            row(Operation::PolicyRevoke, "read", "hermes", Some(SMTP)),
        ]
    );
}

#[test]
fn list_and_get_return_the_fields_of_the_contract() {
    let (rig, handler) = with_secrets();
    assert_eq!(
        data(&handler, "policies.list", json!({})),
        json!({"policies": []})
    );
    assert_eq!(
        data(&handler, "policies.get", json!({"identity": "hades"})),
        json!({"identity": "hades", "reads": [], "status": false, "version": 0, "history": []})
    );
    data(
        &handler,
        "policies.grant_read",
        json!({"identity": "hermes", "secret": SMTP}),
    );
    data(
        &handler,
        "policies.grant_status",
        json!({"identity": "zeus"}),
    );
    let at = isoformat(rig.clock.now());
    assert_eq!(
        data(&handler, "policies.list", json!({})),
        json!({"policies": [
            {"identity": "hermes", "reads": [SMTP], "status": false, "version": 1},
            {"identity": "zeus", "reads": [], "status": true, "version": 1},
        ]})
    );
    assert_eq!(
        data(&handler, "policies.get", json!({"identity": "hermes"})),
        json!({
            "identity": "hermes",
            "reads": [SMTP],
            "status": false,
            "version": 1,
            "history": [
                {"version": 1, "change": "read_granted", "secret": SMTP, "at": at, "by": "admin"},
            ],
        })
    );
    assert_eq!(
        data(&handler, "policies.get", json!({"identity": "zeus"}))["history"],
        json!([{"version": 1, "change": "status_granted", "secret": null, "at": at, "by": "admin"}])
    );
    let queries: Vec<Row> = rows(&rig)
        .into_iter()
        .filter(|(operation, ..)| matches!(operation, Operation::PolicyList | Operation::PolicyGet))
        .collect();
    assert_eq!(queries.len(), 5);
    assert_eq!(queries[1], row(Operation::PolicyGet, "get", "hades", None));
}

#[test]
fn the_errors_follow_the_contract_and_leave_their_rows() {
    let (rig, handler) = with_secrets();
    let cases = [
        (
            "policies.grant_read",
            json!({"identity": "Hermes", "secret": SMTP}),
            "validation",
        ),
        (
            "policies.grant_read",
            json!({"identity": "admin", "secret": SMTP}),
            "validation",
        ),
        (
            "policies.grant_read",
            json!({"identity": "hermes", "secret": format!("{SMTP}@1")}),
            "validation",
        ),
        (
            "policies.grant_read",
            json!({"identity": "hermes", "secret": OWN}),
            "validation",
        ),
        (
            "policies.grant_read",
            json!({"identity": "hermes"}),
            "validation",
        ),
        (
            "policies.grant_read",
            json!({"identity": "hermes", "secret": SMTP, "extra": 1}),
            "validation",
        ),
        (
            "policies.grant_status",
            json!({"identity": "hecate"}),
            "validation",
        ),
        (
            "policies.revoke_status",
            json!({"identity": 5}),
            "validation",
        ),
        ("policies.list", json!({"identity": "hermes"}), "validation"),
        ("policies.get", json!({}), "validation"),
        (
            "policies.grant_read",
            json!({"identity": "hermes", "secret": "secret://hades/nothing"}),
            "not_found",
        ),
    ];
    for (op, args, expected) in &cases {
        let reply = call(&handler, op, args.clone());
        assert_eq!(code(&reply), *expected, "{op} {args}");
    }
    let results: Vec<(Operation, AccessResult)> = rows(&rig)
        .iter()
        .map(|(operation, result, ..)| (*operation, *result))
        .collect();
    assert_eq!(results.len(), cases.len(), "{results:?}");
    assert!(
        results[..cases.len() - 1]
            .iter()
            .all(|(_, result)| *result == AccessResult::InvalidInput)
    );
    assert_eq!(
        results.last(),
        Some(&(Operation::PolicyGrant, AccessResult::NotFound))
    );
    rig.sealing.seal().unwrap();
    for (op, args) in [
        (
            "policies.grant_read",
            json!({"identity": "hermes", "secret": SMTP}),
        ),
        ("policies.list", json!({})),
    ] {
        assert_eq!(code(&call(&handler, op, args)), "conflict", "{op}");
    }
    let sealed: Vec<Row> = rows(&rig).into_iter().skip(cases.len()).collect();
    assert_eq!(
        sealed,
        [
            (
                Operation::PolicyGrant,
                AccessResult::RejectedConflict,
                "read".to_string(),
                Some("hermes".to_string()),
                Some(SMTP.to_string()),
            ),
            (
                Operation::PolicyList,
                AccessResult::RejectedConflict,
                "list".to_string(),
                None,
                None,
            ),
        ]
    );
}
