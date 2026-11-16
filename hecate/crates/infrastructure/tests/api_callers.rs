//! T016: `GET /v1/callers` with the component of the tests (US2, contracts/api.md): an app
//! receives its own list, also empty; the request has no parameters, so nothing asks for the list
//! of another app; a certificate or a token that does not work is 401 with its row, sealed is 503
//! with its row, a malformed X-Request-Id is 400 with its row, and a served one leaves none.

mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::testing::CaRig;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::json;
use tempfile::TempDir;

const CALLERS: &str = "/v1/callers";

async fn enrolled(rig: &CaRig, dir: &TempDir, name: &str, addr: SocketAddr) -> TestComponent {
    let component = TestComponent::open(&dir.path().join(name), rig.clock.clone()).unwrap();
    let token = rig
        .identities
        .tokens_create(name)
        .unwrap()
        .token
        .to_string();
    let reply = component.enroll(addr, &token).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    component
}

type Row = (AccessResult, String, bool, Option<String>);

/// The rows of the list: result, detail, whether each one names who sent it, and the identity.
fn rows(rig: &CaRig) -> Vec<Row> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CallersList)
        .map(|entry| {
            (
                entry.result,
                entry.detail,
                entry.subject.is_some(),
                entry.identity,
            )
        })
        .collect()
}

fn row(result: AccessResult, detail: &str, subject: bool, identity: Option<&str>) -> Row {
    (
        result,
        detail.to_string(),
        subject,
        identity.map(str::to_string),
    )
}

/// A request with headers of its own: what a component would not send.
fn request(
    method: &str,
    path: &str,
    bearer: Option<&str>,
    request_id: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(id) = request_id {
        builder = builder.header("x-request-id", id);
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_receives_its_own_list_by_name_and_its_version() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let contract = Contract::load();
    rig.access_log.clear();
    let empty = hermes.callers(addr).await.unwrap();
    assert_eq!(empty.status, 200, "{}", empty.body);
    contract.assert_valid("listCallers", 200, &empty.body);
    assert_eq!(
        empty.body["data"],
        json!({"app": "hermes", "callers": [], "version": 0})
    );
    rig.access.grant("hades", "hermes").unwrap();
    rig.access.grant("caller", "hermes").unwrap();
    rig.access.grant("hermes", "estigia").unwrap();
    let listed = hermes.callers(addr).await.unwrap();
    assert_eq!(listed.status, 200, "{}", listed.body);
    contract.assert_valid("listCallers", 200, &listed.body);
    assert_eq!(
        listed.body["data"],
        json!({"app": "hermes", "callers": ["caller", "hades"], "version": 2})
    );
    assert!(rows(&rig).is_empty());
    let capabilities = hermes.capabilities(addr).await.unwrap();
    contract.assert_valid("getCapabilities", 200, &capabilities.body);
    let operations = capabilities.body["data"]["operations"].as_array().unwrap();
    assert!(operations.contains(&json!("callers")), "{operations:?}");
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_asks_for_the_list_of_another_app() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    rig.access.grant("hermes", "estigia").unwrap();
    rig.access_log.clear();
    let contract = Contract::load();
    let not_found = contract.hecate_schema("NotFoundFailure");
    for (method, path) in [
        ("GET", "/v1/callers/estigia"),
        ("GET", "/v1/callers/"),
        ("POST", CALLERS),
    ] {
        let token = hermes.call_token(None).unwrap();
        let reply = hermes
            .send_as(addr, None, request(method, path, Some(&token), None))
            .await
            .unwrap();
        assert_eq!(reply.status, 404, "{method} {path}: {}", reply.body);
        assert_eq!(reply.body["error"]["code"], "not_found", "{method} {path}");
        contract
            .check_schema(&not_found, &reply.body)
            .unwrap_or_else(|errors| panic!("{method} {path}: {errors}"));
    }
    let token = hermes.call_token(None).unwrap();
    let asked = request("GET", "/v1/callers?app=estigia", Some(&token), None);
    let reply = hermes.send_as(addr, None, asked).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body["data"],
        json!({"app": "hermes", "callers": [], "version": 0})
    );
    assert!(rows(&rig).is_empty());
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusals_answer_their_code_and_leave_their_row() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let estigia = enrolled(&rig, &dir, "estigia", addr).await;
    rig.access_log.clear();
    let forged = request("GET", CALLERS, Some("not.a.token"), None);
    let forged = hermes.send_as(addr, None, forged).await.unwrap();
    let token = hermes.call_token(None).unwrap();
    let malformed = request("GET", CALLERS, Some(&token), Some("tab\there"));
    let malformed = hermes.send_as(addr, None, malformed).await.unwrap();
    rig.identities.identities_disable("estigia", false).unwrap();
    let disabled = estigia.callers(addr).await.unwrap();
    let token = hermes.call_token(None).unwrap();
    let head = request("HEAD", CALLERS, Some(&token), None);
    let head = hermes.send_as(addr, None, head).await.unwrap();
    rig.sealing.seal().unwrap();
    let sealed = hermes.callers(addr).await.unwrap();
    let contract = Contract::load();
    for (reply, status, code) in [
        (&forged, 401, "auth"),
        (&malformed, 400, "validation"),
        (&disabled, 401, "auth"),
        (&sealed, 503, "unavailable"),
    ] {
        assert_eq!(reply.status, status, "{}", reply.body);
        contract.assert_valid("listCallers", status, &reply.body);
        assert_eq!(reply.body["error"]["code"], code);
    }
    assert_eq!(head.status, 200);
    assert_eq!(
        rows(&rig),
        [
            row(AccessResult::Failure, "list", true, Some("hermes")),
            row(AccessResult::InvalidInput, "request_id", false, None),
            row(AccessResult::Failure, "list", true, Some("estigia")),
            row(AccessResult::RejectedConflict, "sealed", false, None),
        ]
    );
    supervisor.shutdown().await;
}
