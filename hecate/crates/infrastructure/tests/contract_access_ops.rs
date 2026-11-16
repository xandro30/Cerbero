//! T011: the operations `access.*` on the local channel (contracts/control-protocol.md): the exact
//! fields of `data`, the repeated orders, the errors and the row of each one.

mod common;

use common::*;
use hecate_application::ports::{Clock, EventStore};
use hecate_application::testing::CaRig;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};

fn with_rig() -> (CaRig, RequestHandler) {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    rig.access_log.clear();
    (rig, handler)
}

type Row = (Operation, AccessResult, String, Option<String>);

/// The rows of the lists of access: operation, result, detail and app; the admin sends them all.
fn rows(rig: &CaRig) -> Vec<Row> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.operation,
                Operation::AccessGrant
                    | Operation::AccessRevoke
                    | Operation::AccessList
                    | Operation::AccessGet
            )
        })
        .inspect(|entry| assert_eq!(entry.subject.as_deref(), Some("admin")))
        .map(|entry| (entry.operation, entry.result, entry.detail, entry.identity))
        .collect()
}

fn row(operation: Operation, result: AccessResult, detail: &str, app: Option<&str>) -> Row {
    (
        operation,
        result,
        detail.to_string(),
        app.map(str::to_string),
    )
}

fn data(handler: &RequestHandler, op: &str, args: Value) -> Value {
    let reply = call(handler, op, args);
    assert_eq!(reply["status"], "completed", "{op}: {reply}");
    reply["data"].clone()
}

#[test]
fn the_orders_return_the_fields_of_the_contract() {
    let (rig, handler) = with_rig();
    let order = json!({"identity": "hades", "app": "hermes"});
    for (op, expected) in [
        (
            "access.grant",
            json!({"identity": "hades", "app": "hermes", "version": 1, "changed": true}),
        ),
        (
            "access.grant",
            json!({"identity": "hades", "app": "hermes", "version": 1, "changed": false}),
        ),
        (
            "access.revoke",
            json!({"identity": "hades", "app": "hermes", "version": 2, "changed": true}),
        ),
        (
            "access.revoke",
            json!({"identity": "hades", "app": "hermes", "version": 2, "changed": false}),
        ),
    ] {
        assert_eq!(data(&handler, op, order.clone()), expected, "{op}");
    }
    let ok = AccessResult::Success;
    assert_eq!(
        rows(&rig),
        [
            row(Operation::AccessGrant, ok, "caller hades", Some("hermes")),
            row(Operation::AccessGrant, ok, "caller hades", Some("hermes")),
            row(Operation::AccessRevoke, ok, "caller hades", Some("hermes")),
            row(Operation::AccessRevoke, ok, "caller hades", Some("hermes")),
        ]
    );
}

#[test]
fn list_and_get_return_the_fields_of_the_contract() {
    let (rig, handler) = with_rig();
    assert_eq!(
        data(&handler, "access.list", json!({})),
        json!({"apps": []})
    );
    assert_eq!(
        data(&handler, "access.get", json!({"app": "hades"})),
        json!({"app": "hades", "callers": [], "version": 0, "history": []})
    );
    data(
        &handler,
        "access.grant",
        json!({"identity": "hades", "app": "hermes"}),
    );
    data(
        &handler,
        "access.grant",
        json!({"identity": "caller", "app": "hermes"}),
    );
    data(
        &handler,
        "access.grant",
        json!({"identity": "hermes", "app": "estigia"}),
    );
    let at = isoformat(rig.clock.now());
    assert_eq!(
        data(&handler, "access.list", json!({})),
        json!({"apps": [
            {"app": "estigia", "callers": 1, "version": 1},
            {"app": "hermes", "callers": 2, "version": 2},
        ]})
    );
    assert_eq!(
        data(&handler, "access.get", json!({"app": "hermes"})),
        json!({
            "app": "hermes",
            "callers": ["caller", "hades"],
            "version": 2,
            "history": [
                {"version": 1, "change": "caller_granted", "caller": "hades", "at": at, "by": "admin"},
                {"version": 2, "change": "caller_granted", "caller": "caller", "at": at, "by": "admin"},
            ],
        })
    );
    data(
        &handler,
        "access.revoke",
        json!({"identity": "hades", "app": "hermes"}),
    );
    assert_eq!(
        data(&handler, "access.get", json!({"app": "hermes"}))["history"][2],
        json!({"version": 3, "change": "caller_revoked", "caller": "hades", "at": at, "by": "admin"})
    );
    let queries: Vec<Row> = rows(&rig)
        .into_iter()
        .filter(|(operation, ..)| matches!(operation, Operation::AccessList | Operation::AccessGet))
        .collect();
    let ok = AccessResult::Success;
    assert_eq!(
        queries,
        [
            row(Operation::AccessList, ok, "", None),
            row(Operation::AccessGet, ok, "", Some("hades")),
            row(Operation::AccessList, ok, "", None),
            row(Operation::AccessGet, ok, "", Some("hermes")),
            row(Operation::AccessGet, ok, "", Some("hermes")),
        ]
    );
}

#[test]
fn the_errors_follow_the_contract_and_leave_their_rows() {
    let (rig, handler) = with_rig();
    let cases = [
        (
            "access.grant",
            json!({"identity": "Hades", "app": "hermes"}),
        ),
        (
            "access.grant",
            json!({"identity": "admin", "app": "hermes"}),
        ),
        (
            "access.grant",
            json!({"identity": "hades", "app": "hecate"}),
        ),
        (
            "access.grant",
            json!({"identity": "hermes", "app": "hermes"}),
        ),
        (
            "access.grant",
            json!({"identity": "hades/integration/x", "app": "hermes"}),
        ),
        ("access.grant", json!({"identity": "hades"})),
        (
            "access.grant",
            json!({"identity": "hades", "app": "hermes", "extra": 1}),
        ),
        ("access.revoke", json!({"identity": 5, "app": "hermes"})),
        ("access.list", json!({"app": "hermes"})),
        ("access.get", json!({})),
        ("access.get", json!({"app": "admin"})),
    ];
    for (op, args) in &cases {
        let reply = call(&handler, op, args.clone());
        assert_eq!(code(&reply), "validation", "{op} {args}");
    }
    let results: Vec<(Operation, AccessResult)> = rows(&rig)
        .iter()
        .map(|(operation, result, ..)| (*operation, *result))
        .collect();
    assert_eq!(results.len(), cases.len(), "{results:?}");
    assert!(
        results
            .iter()
            .all(|(_, result)| *result == AccessResult::InvalidInput)
    );
    assert!(rig.store.streams("access/").unwrap().is_empty());
    rig.sealing.seal().unwrap();
    for (op, args) in [
        (
            "access.grant",
            json!({"identity": "hades", "app": "hermes"}),
        ),
        ("access.list", json!({})),
    ] {
        assert_eq!(code(&call(&handler, op, args)), "conflict", "{op}");
    }
    let sealed: Vec<Row> = rows(&rig).into_iter().skip(cases.len()).collect();
    let refused = AccessResult::RejectedConflict;
    assert_eq!(
        sealed,
        [
            row(
                Operation::AccessGrant,
                refused,
                "caller hades",
                Some("hermes")
            ),
            row(Operation::AccessList, refused, "", None),
        ]
    );
}
