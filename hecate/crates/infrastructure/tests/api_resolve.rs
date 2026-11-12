//! T015: `POST /v1/secrets:resolve` with the component of the tests (US1, contracts/api.md): the
//! fields of the contract, the same refusal for its three cases and never sooner than the floor,
//! the rows of the refusals before the resolution, and ten secrets of 64 KiB within a second
//! (SC-001).

mod common;

use std::net::SocketAddr;
use std::time::{Duration as StdDuration, Instant};

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::identity::settings::REFUSAL_FLOOR;
use hecate_application::testing::CaRig;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::{Value, json};
use tempfile::TempDir;

const TOKEN: &str = "secret://hermes/telegram/bot_token";
const PROXY: &str = "secret://hermes/certs/proxy";
const HADES: &str = "secret://hades/smtp/password";
const MISSING: &str = "secret://hermes/does/not/exist";
const UNRESOLVABLE: &str = "secret://hermes/old/key";
const RESOLVE: &str = "/v1/secrets:resolve";

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

/// The secrets of the tests: two of `hermes`, one of `hades`, and one of `hermes` whose only
/// version is revoked.
fn stored(rig: &CaRig) {
    store_secret(rig, TOKEN, "token-value");
    store_secret(rig, PROXY, "proxy-value");
    store_secret(rig, HADES, "hades-value");
    store_secret(rig, UNRESOLVABLE, "old-value");
    revoke_secret(rig, UNRESOLVABLE, 1);
}

/// The results of the rows of the resolution, and whether each one names who sent it.
fn rows(rig: &CaRig) -> Vec<(AccessResult, bool)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::SecretResolve)
        .map(|entry| (entry.result, entry.subject.is_some()))
        .collect()
}

/// A resolution with a body and headers of its own: what a component would not send.
fn raw(
    component: &TestComponent,
    body: &str,
    bearer: bool,
    request_id: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(RESOLVE)
        .header("content-type", "application/json");
    if bearer {
        let token = component.call_token(None).unwrap();
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(id) = request_id {
        builder = builder.header("x-request-id", id);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn secret(reference: &str, value: &str) -> Value {
    json!({
        "reference": reference,
        "version": 1,
        "value": value,
        "cache_ttl_soft_s": 300,
        "cache_ttl_hard_s": 3600,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_component_resolves_its_secrets_with_the_fields_of_the_contract() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    stored(&rig);
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let concrete = format!("{PROXY}@1");
    let reply = hermes.resolve(addr, &[TOKEN, &concrete]).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    Contract::load().assert_valid("resolveSecrets", 200, &reply.body);
    assert_eq!(
        reply.body["data"]["secrets"],
        json!([
            secret(TOKEN, "token-value"),
            secret(&concrete, "proxy-value")
        ])
    );
    assert_eq!(
        rows(&rig),
        [(AccessResult::Success, true), (AccessResult::Success, true)]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_three_refusals_answer_the_same_body_and_never_sooner_than_the_floor() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    stored(&rig);
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let contract = Contract::load();
    let mut errors = Vec::new();
    for references in [[HADES, TOKEN], [TOKEN, MISSING], [UNRESOLVABLE, PROXY]] {
        let started = Instant::now();
        let reply = hermes.resolve(addr, &references).await.unwrap();
        let elapsed = started.elapsed();
        assert_eq!(reply.status, 404, "{}", reply.body);
        contract.assert_valid("resolveSecrets", 404, &reply.body);
        assert!(elapsed >= REFUSAL_FLOOR, "{references:?}: {elapsed:?}");
        let duration = reply.body["meta"]["duration_ms"].as_u64().unwrap();
        assert!(duration >= 250, "{references:?}: {duration} ms");
        errors.push(reply.body["error"].clone());
    }
    assert!(
        errors.windows(2).all(|pair| pair[0] == pair[1]),
        "{errors:?}"
    );
    assert_eq!(errors[0]["message"], "secret not found");
    assert_eq!(rows(&rig), [(AccessResult::NotFound, true); 3]);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refusals_before_the_resolution_leave_their_rows() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    stored(&rig);
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let contract = Contract::load();
    let valid = json!({"references": [TOKEN]}).to_string();
    let mut statuses = Vec::new();
    // A body that is not standard JSON: `validation`, without a row (FR-015).
    let not_json = raw(&hermes, "{\"references\": [", true, None);
    statuses.push(hermes.send_as(addr, None, not_json).await.unwrap());
    assert!(rows(&rig).is_empty());
    // A malformed reference, a malformed X-Request-Id and no short call token.
    statuses.push(hermes.resolve(addr, &["not a reference"]).await.unwrap());
    let request_id = raw(&hermes, &valid, true, Some("tab\there"));
    statuses.push(hermes.send_as(addr, None, request_id).await.unwrap());
    let without_token = raw(&hermes, &valid, false, None);
    statuses.push(hermes.send_as(addr, None, without_token).await.unwrap());
    // Sealed: `unavailable`.
    rig.sealing.seal().unwrap();
    statuses.push(hermes.resolve(addr, &[TOKEN]).await.unwrap());
    for reply in &statuses {
        contract.assert_valid("resolveSecrets", reply.status, &reply.body);
    }
    assert_eq!(
        statuses
            .iter()
            .map(|reply| reply.status)
            .collect::<Vec<_>>(),
        [400, 400, 400, 401, 503]
    );
    assert_eq!(
        rows(&rig),
        [
            (AccessResult::InvalidInput, true),
            (AccessResult::InvalidInput, false),
            (AccessResult::Failure, true),
            (AccessResult::RejectedConflict, false),
        ]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ten_secrets_of_64_kib_resolve_within_a_second() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let value = "v".repeat(64 * 1024);
    let paths: Vec<String> = (0..10)
        .map(|n| format!("secret://hermes/large/key_{n}"))
        .collect();
    for path in &paths {
        store_secret(&rig, path, &value);
    }
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let references: Vec<&str> = paths.iter().map(String::as_str).collect();
    let started = Instant::now();
    let reply = hermes.resolve(addr, &references).await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(reply.status, 200, "{}", reply.body);
    let secrets = reply.body["data"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 10);
    assert!(
        secrets
            .iter()
            .all(|secret| secret["value"] == value.as_str())
    );
    assert!(elapsed < StdDuration::from_secs(1), "{elapsed:?}");
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shared_secret_resolves_while_its_exception_lasts() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    stored(&rig);
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    assert_eq!(hermes.resolve(addr, &[HADES]).await.unwrap().status, 404);
    rig.policies.grant_read("hermes", HADES).unwrap();
    let reply = hermes.resolve(addr, &[HADES]).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["data"]["secrets"][0]["value"], "hades-value");
    rig.policies.revoke_read("hermes", HADES).unwrap();
    assert_eq!(hermes.resolve(addr, &[HADES]).await.unwrap().status, 404);
    supervisor.shutdown().await;
}
