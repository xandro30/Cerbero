//! T082: `GET /v1/keys`, `GET /v1/keys/{kid}` and `GET /v1/capabilities` with the component of
//! the tests (contracts/api.md): the fields of the contract, no row when served and a row when
//! refused (FR-024), and a year of data answered within a second (plan, Performance Goals).

mod common;

use std::time::{Duration as StdDuration, Instant};

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::CaRig;
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::Value;
use tempfile::TempDir;
use time::Duration;

const JWK_MEMBERS: [&str; 9] = [
    "alg",
    "cerbero_identity",
    "cerbero_not_after",
    "cerbero_status",
    "crv",
    "kid",
    "kty",
    "use",
    "x",
];

async fn enrolled(
    rig: &CaRig,
    dir: &TempDir,
    name: &str,
    addr: std::net::SocketAddr,
) -> TestComponent {
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

fn members(jwk: &Value) -> Vec<String> {
    let mut keys: Vec<String> = jwk.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn rows(rig: &CaRig, operation: Operation) -> Vec<AccessResult> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .map(|entry| entry.result)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_queries_answer_the_fields_of_the_contract() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    enrolled(&rig, &dir, "hades", addr).await;
    let contract = Contract::load();

    let keys = hermes.keys(addr).await.unwrap();
    assert_eq!(keys.status, 200, "{}", keys.body);
    contract.assert_valid("listKeys", 200, &keys.body);
    let listed = keys.body["data"]["keys"].as_array().unwrap();
    assert_eq!(listed.len(), 2);
    for jwk in listed {
        assert_eq!(members(jwk), JWK_MEMBERS);
        assert_eq!(jwk["cerbero_status"], "active");
    }
    let expires =
        hecate_domain::instant::parse_iso(keys.body["data"]["expires_at"].as_str().unwrap())
            .unwrap();
    assert_eq!(expires, rig.clock.now() + Duration::seconds(300));

    let own = rig_key(&hermes);
    let key = hermes.key(addr, &own.kid()).await.unwrap();
    assert_eq!(key.status, 200, "{}", key.body);
    contract.assert_valid("getKey", 200, &key.body);
    assert_eq!(key.body["data"]["key"]["x"], own.x());
    assert_eq!(
        key.body["data"]["key"]["cerbero_identity"],
        "spiffe://cerbero.test/hermes"
    );
    let unknown = hermes
        .key(addr, &CallKey::from_bytes(&[3; 32]).unwrap().kid())
        .await
        .unwrap();
    assert_eq!(unknown.status, 404);
    contract.assert_valid("getKey", 404, &unknown.body);

    let capabilities = hermes.capabilities(addr).await.unwrap();
    assert_eq!(capabilities.status, 200, "{}", capabilities.body);
    contract.assert_valid("getCapabilities", 200, &capabilities.body);
    assert_eq!(capabilities.body["data"]["component"], "hecate");
    supervisor.shutdown().await;
}

fn rig_key(component: &TestComponent) -> CallKey {
    let text = component.call_key().unwrap();
    let der = pem::single(&text, "PUBLIC KEY").unwrap();
    CallKey::from_bytes(&der[der.len() - 32..]).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn served_queries_leave_no_row_and_refusals_are_auth_with_a_row() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    assert_eq!(hermes.keys(addr).await.unwrap().status, 200);
    assert_eq!(hermes.capabilities(addr).await.unwrap().status, 200);
    assert!(rows(&rig, Operation::KeyList).is_empty());
    assert!(rows(&rig, Operation::CapabilitiesGet).is_empty());

    let root = rig.service.status().unwrap().unwrap().root_fingerprint;
    let token = hermes.call_token(None).unwrap();
    let anonymous = Request::builder()
        .uri("/v1/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let refused = hermes.send(addr, &root, None, anonymous).await.unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);
    Contract::load().assert_valid("listKeys", 401, &refused.body);
    let no_token = Request::builder()
        .uri("/v1/keys")
        .body(Body::empty())
        .unwrap();
    let refused = hermes.send_as(addr, None, no_token).await.unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);

    let other = ca_rig();
    let (other_supervisor, _, other_addr) = listening_api(&other).await;
    let foreign = enrolled(&other, &dir, "foreign", other_addr).await;
    let token = foreign.call_token(None).unwrap();
    let request = Request::builder()
        .uri("/v1/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let refused = hermes
        .send(addr, &root, Some(&foreign), request)
        .await
        .unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);

    // Hécate renews its own certificate on time; the one of the component expires.
    rig.clock.advance(Duration::hours(49));
    supervisor.tick().await.unwrap();
    rig.clock.advance(Duration::hours(24));
    let addr = supervisor.local_addr().await.unwrap();
    let expired = hermes.keys(addr).await.unwrap();
    assert_eq!(expired.status, 401, "{}", expired.body);
    assert_eq!(
        rows(&rig, Operation::KeyList),
        vec![AccessResult::Failure; 4]
    );
    other_supervisor.shutdown().await;
    supervisor.shutdown().await;
}

/// T127 (FR-024): a request id that does not follow its rule is `validation` before the use case,
/// with the row of the operation and without a subject: nobody was authenticated.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_request_id_is_validation_with_the_row_of_its_operation() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let token = hermes.call_token(None).unwrap();
    let request = Request::builder()
        .uri("/v1/keys")
        .header("authorization", format!("Bearer {token}"))
        .header("x-request-id", "tab\there")
        .body(Body::empty())
        .unwrap();
    let refused = hermes.send_as(addr, None, request).await.unwrap();
    assert_eq!(
        (refused.status, refused.body["error"]["code"].as_str()),
        (400, Some("validation"))
    );
    Contract::load().assert_valid("listKeys", 400, &refused.body);
    let rows: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::KeyList)
        .map(|entry| (entry.method, entry.result, entry.detail, entry.subject))
        .collect();
    assert_eq!(
        rows,
        vec![(
            Method::Identity,
            AccessResult::InvalidInput,
            "request_id".to_string(),
            None
        )]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_a_year_of_data_the_list_of_keys_answers_within_a_second() {
    let YearOfData {
        caller,
        supervisor,
        addr,
        stack,
        dirs: _dirs,
    } = a_year_of_data(false).await;
    let started = Instant::now();
    let keys = caller.keys(addr).await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(keys.status, 200, "{}", keys.body);
    // The last certificate of each component and the one of the caller are valid now.
    assert_eq!(keys.body["data"]["keys"].as_array().unwrap().len(), 31);
    assert!(elapsed < StdDuration::from_secs(1), "{elapsed:?}");
    supervisor.shutdown().await;
    stack.close();
}

/// T132 (principle II, FR-024): a kid that is not valid UTF-8 once decoded reaches the use case,
/// which authenticates first and answers `validation` with its row, in the common envelope.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kid_that_is_not_utf8_reaches_the_use_case() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let reply = hermes.key(addr, "%FF").await.unwrap();
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (400, Some("validation")),
        "{}",
        reply.body
    );
    Contract::load().assert_valid("getKey", 400, &reply.body);
    // Without a short call token, the authentication answers first.
    let anonymous = Request::builder()
        .uri("/v1/keys/%FF")
        .body(Body::empty())
        .unwrap();
    let refused = hermes.send_as(addr, None, anonymous).await.unwrap();
    assert_eq!(
        (refused.status, refused.body["error"]["code"].as_str()),
        (401, Some("auth"))
    );
    assert_eq!(
        rows(&rig, Operation::KeyGet),
        vec![AccessResult::InvalidInput, AccessResult::Failure]
    );
    supervisor.shutdown().await;
}
