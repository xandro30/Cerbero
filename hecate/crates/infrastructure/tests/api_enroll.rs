//! T053 and T068: `POST /v1/identities:enroll` with the component of the tests (contracts/api.md):
//! it checks Hécate with the fingerprint of the token before sending it, gets the fields of the
//! contract, and each error has its status; the task of every minute closes the batches of the
//! throttle.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration as StdDuration, Instant};

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::ports::{CertificateProfile, Clock, PkiProvider};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::identity::names::{IdentityName, RootFingerprint, SpiffeId, TrustDomain};
use hecate_domain::identity::token::EnrollmentToken;
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_infrastructure::pki::RcgenPki;
use hecate_infrastructure::tasks;
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::{Value, json};
use tempfile::TempDir;

fn spiffe(identity: &str) -> SpiffeId {
    SpiffeId::new(
        TrustDomain::parse(TRUST_DOMAIN).unwrap(),
        IdentityName::parse(identity).unwrap(),
    )
}

fn token(rig: &CaRig, identity: &str) -> String {
    rig.identities
        .tokens_create(identity)
        .unwrap()
        .token
        .to_string()
}

fn fingerprint_of(token: &str) -> RootFingerprint {
    *EnrollmentToken::parse(token).unwrap().fingerprint()
}

fn component(rig: &CaRig, dir: &TempDir, name: &str) -> TestComponent {
    TestComponent::open(&dir.path().join(name), rig.clock.clone()).unwrap()
}

fn enroll_rows(rig: &CaRig) -> usize {
    rig.access_log
        .entries()
        .iter()
        .filter(|entry| entry.operation == Operation::IdentityEnroll)
        .count()
}

fn post(body: impl Into<Body>, key: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/identities:enroll")
        .header("content-type", "application/json");
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    builder.body(body.into()).unwrap()
}

fn body(token: &str, component: &TestComponent, asks_for: Option<&str>) -> String {
    let csr = component
        .new_request(asks_for.map(spiffe).as_ref())
        .unwrap();
    json!({"token": token, "csr": csr, "call_key": component.call_key().unwrap()}).to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_component_checks_hecate_with_the_fingerprint_before_sending_the_token() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    let token = token(&rig, "hermes");
    let (front, _) = token.rsplit_once('.').unwrap();
    let elsewhere = format!("{front}.{}", "0".repeat(64));
    assert!(hermes.enroll(addr, &elsewhere).await.is_err());
    assert_eq!(enroll_rows(&rig), 0, "the token never reached Hécate");
    assert_eq!(rig.identities.tokens_list().unwrap().len(), 1);
    let reply = hermes.enroll(addr, &token).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_enrollment_answers_the_fields_of_the_contract_within_a_second() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    let token = token(&rig, "hermes");
    let started = Instant::now();
    let reply = hermes.enroll(addr, &token).await.unwrap();
    assert!(
        started.elapsed() < StdDuration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    Contract::load().assert_valid("enrollIdentity", 200, &reply.body);
    let data = &reply.body["data"];
    assert_eq!(data["identity"], "spiffe://cerbero.test/hermes");
    let pki = RcgenPki::new();
    let certificate = pem::single(data["certificate"].as_str().unwrap(), "CERTIFICATE").unwrap();
    let chain: Vec<Vec<u8>> = data["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| pem::single(block.as_str().unwrap(), "CERTIFICATE").unwrap())
        .collect();
    let profile = CertificateProfile::Leaf {
        spiffe_id: spiffe("hermes"),
    };
    assert!(pki.follows_profile(&certificate, &profile).unwrap());
    assert!(pki.is_signed_by(&certificate, &chain[0]).unwrap());
    assert!(pki.is_signed_by(&chain[0], &chain[1]).unwrap());
    let info = pki.read_certificate(&certificate).unwrap();
    assert_eq!(info.not_after, rig.clock.now() + time::Duration::hours(72));
    let call_key = pki.read_call_key(&hermes.call_key().unwrap()).unwrap();
    assert_eq!(data["call_key_id"], call_key.kid());

    // No private key of the component reaches Hécate.
    for name in ["tls-key.pem", "call-key.pem"] {
        let path = hermes.path(name);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let key = pem::single(&std::fs::read_to_string(&path).unwrap(), "PRIVATE KEY").unwrap();
        assert!(!contains(&rig.store.contents(), &key));
        for row in rig.access_log.entries() {
            assert!(!format!("{row:?}").contains(&data_encoding::HEXLOWER.encode(&key)));
        }
    }
    let state = dir.path().join("hermes");
    assert_eq!(
        std::fs::metadata(state).unwrap().permissions().mode() & 0o777,
        0o700
    );

    // The exact repetition gets the same certificate; another request with the token, `auth`.
    let again = hermes.repeat_last(addr, &token).await.unwrap();
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.body["data"]["serial"], data["serial"]);
    let other = hermes.enroll(addr, &token).await.unwrap();
    assert_eq!(
        (other.status, other.body["error"]["code"].as_str()),
        (401, Some("auth"))
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_certificate_that_arrives_with_the_enrollment_is_not_used() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    assert_eq!(
        hermes
            .enroll(addr, &token(&rig, "hermes"))
            .await
            .unwrap()
            .status,
        200
    );
    let hades = component(&rig, &dir, "hades");
    let reply = hades
        .enroll_presenting(addr, &token(&rig, "hades"), Some(&hermes))
        .await
        .unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body["data"]["identity"],
        "spiffe://cerbero.test/hades"
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_refusal_has_its_status_follows_the_contract_and_leaves_a_row() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    let token = token(&rig, "hermes");
    let fingerprint = fingerprint_of(&token);
    let contract = Contract::load();
    let send = |request| hermes.send(addr, &fingerprint, None, request);

    let not_json = send(post("{not json", Some("k1"))).await.unwrap();
    assert_eq!(not_json.status, 400);
    assert_eq!(not_json.body["error"]["code"], "validation");
    contract.assert_valid("enrollIdentity", 400, &not_json.body);
    assert_eq!(
        enroll_rows(&rig),
        0,
        "a body that is not JSON leaves no row"
    );

    let call_key = hermes.call_key().unwrap();
    let malformed = json!({"token": token, "csr": "not pem", "call_key": call_key}).to_string();
    for (request, status) in [
        (post(malformed, Some("k1")), 400),
        (post(body(&token, &hermes, Some("hades")), Some("k1")), 400),
        (post(body(&token, &hermes, None), None), 400),
    ] {
        let reply = send(request).await.unwrap();
        assert_eq!(reply.status, status, "{}", reply.body);
        contract.assert_valid("enrollIdentity", status, &reply.body);
    }
    let (front, _) = token.split_at(token.len() - 64 - 44);
    let forged = format!("{front}{}.{}", "A".repeat(43), &token[token.len() - 64..]);
    let auth = send(post(body(&forged, &hermes, None), Some("k1")))
        .await
        .unwrap();
    assert_eq!(
        (auth.status, auth.body["error"]["code"].as_str()),
        (401, Some("auth"))
    );
    contract.assert_valid("enrollIdentity", 401, &auth.body);

    let issued = send(post(body(&token, &hermes, None), Some("k1")))
        .await
        .unwrap();
    assert_eq!(issued.status, 200, "{}", issued.body);
    let other = rig
        .identities
        .tokens_create("hermes")
        .unwrap()
        .token
        .to_string();
    let reused = send(post(body(&other, &hermes, None), Some("k1")))
        .await
        .unwrap();
    assert_eq!(
        (reused.status, reused.body["error"]["code"].as_str()),
        (409, Some("conflict"))
    );
    contract.assert_valid("enrollIdentity", 409, &reused.body);
    assert_eq!(enroll_rows(&rig), 6);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn throttled_it_answers_rate_limited_with_retry_after_and_sealed_unavailable() {
    let rig = ca_rig();
    let (supervisor, handler, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    let token = token(&rig, "hermes");
    let fingerprint = fingerprint_of(&token);
    let contract = Contract::load();
    let bad = json!({"token": "x", "csr": "x", "call_key": "x"}).to_string();
    for _ in 0..10 {
        let reply = hermes
            .send(addr, &fingerprint, None, post(bad.clone(), Some("k1")))
            .await
            .unwrap();
        assert_eq!(reply.status, 400);
    }
    let throttled = hermes.enroll(addr, &token).await.unwrap();
    assert_eq!(throttled.status, 429, "{}", throttled.body);
    assert_eq!(throttled.headers["retry-after"], "60");
    assert_eq!(throttled.body["error"]["retry_after_s"], 60);
    contract.assert_valid("enrollIdentity", 429, &throttled.body);

    operation(&handler, "seal", json!({})).await;
    let sealed = hermes.enroll(addr, &token).await.unwrap();
    assert_eq!(
        (sealed.status, sealed.body["error"]["code"].as_str()),
        (503, Some("unavailable"))
    );
    contract.assert_valid("enrollIdentity", 503, &sealed.body);
    // T127 (FR-024): the sealed refusal leaves its row, without a subject.
    let refused: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::IdentityEnroll && entry.detail == "sealed")
        .map(|entry| (entry.method, entry.result, entry.subject, entry.identity))
        .collect();
    assert_eq!(
        refused,
        vec![(
            Method::EnrollmentToken,
            AccessResult::RejectedConflict,
            None,
            None
        )]
    );
    operation(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE})).await;
    supervisor.shutdown().await;
}

/// T129 (FR-034, SC-017): an enrollment refused for a malformed request id goes through the
/// throttle, like a malformed Idempotency-Key: it counts as a failure and, while the enrollments
/// are throttled, it answers `rate_limited` and counts in the batch, without a row of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_enrollment_with_a_malformed_request_id_goes_through_the_throttle() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = component(&rig, &dir, "hermes");
    let token = token(&rig, "hermes");
    let fingerprint = fingerprint_of(&token);
    let malformed = || {
        let mut request = post(body(&token, &hermes, None), Some("k1"));
        request
            .headers_mut()
            .insert("x-request-id", "tab\there".parse().unwrap());
        request
    };
    let own_rows = |rig: &CaRig| {
        rig.access_log
            .entries()
            .iter()
            .filter(|entry| {
                entry.operation == Operation::IdentityEnroll && entry.detail == "request_id"
            })
            .count()
    };
    for _ in 0..10 {
        let reply = hermes
            .send(addr, &fingerprint, None, malformed())
            .await
            .unwrap();
        assert_eq!(
            (reply.status, reply.body["error"]["code"].as_str()),
            (400, Some("validation"))
        );
    }
    assert_eq!(own_rows(&rig), 10);
    // Ten failures: the next enrollment is throttled, also with a valid token.
    let throttled = hermes.enroll(addr, &token).await.unwrap();
    assert_eq!(throttled.status, 429, "{}", throttled.body);
    // Throttled, a malformed request id answers `rate_limited` and leaves no row of its own.
    let reply = hermes
        .send(addr, &fingerprint, None, malformed())
        .await
        .unwrap();
    assert_eq!(
        (reply.status, reply.body["error"]["code"].as_str()),
        (429, Some("rate_limited"))
    );
    assert_eq!(reply.headers["retry-after"], "60");
    Contract::load().assert_valid("enrollIdentity", 429, &reply.body);
    assert_eq!(own_rows(&rig), 10);
    // The batch counts both refusals of the wait.
    rig.clock.advance(time::Duration::seconds(61));
    rig.identities.close_throttle_batch().unwrap();
    let batches: Vec<String> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::IdentityEnrollThrottled)
        .map(|entry| entry.detail)
        .collect();
    assert_eq!(batches, vec!["failures 10, rejected 2".to_string()]);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_task_of_every_minute_closes_the_batches_of_the_throttle() {
    let rig = ca_rig();
    let (supervisor, _, _) = listening_api(&rig).await;
    for _ in 0..10 {
        let refused = rig.identities.enroll(
            Some("k1"),
            &json!({"token": "x", "csr": "x", "call_key": "x"}),
        );
        assert!(refused.is_err());
    }
    let periodic = tasks::spawn_periodic(
        StdDuration::from_millis(20),
        rig.service.clone(),
        rig.identities.clone(),
        supervisor.clone(),
    );
    rig.clock.advance(time::Duration::minutes(1));
    let mut closed = Vec::<Value>::new();
    for _ in 0..200 {
        closed = rig
            .access_log
            .entries()
            .iter()
            .filter(|entry| entry.operation == Operation::IdentityEnrollThrottled)
            .map(|entry| json!(entry.detail))
            .collect();
        if !closed.is_empty() {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    assert_eq!(closed, vec![json!("failures 10, rejected 0")]);
    periodic.stop().await;
    supervisor.shutdown().await;
}
