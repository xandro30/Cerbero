//! T072: `POST /v1/certificates:renew` with the component of the tests (contracts/api.md): the
//! certificate of the connection and a short call token; a certificate that is not valid, of
//! another CA or missing is `auth` with its row (SC-006); renewing at two thirds keeps the
//! identity (SC-008).

mod common;

use std::time::{Duration as StdDuration, Instant};

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::ca::settings::CaSettings;
use hecate_application::testing::CaRig;
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::Value;
use tempfile::TempDir;
use time::Duration;

fn component(rig: &CaRig, dir: &TempDir, name: &str) -> TestComponent {
    TestComponent::open(&dir.path().join(name), rig.clock.clone()).unwrap()
}

async fn enrolled(
    rig: &CaRig,
    dir: &TempDir,
    name: &str,
    addr: std::net::SocketAddr,
) -> TestComponent {
    let component = component(rig, dir, name);
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

/// The rows of the renewals of components; Hécate's own renewals are `internal`.
fn renew_rows(rig: &CaRig) -> Vec<AccessResult> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CertificateRenew)
        .filter(|entry| entry.method == Method::Identity)
        .map(|entry| entry.result)
        .collect()
}

fn renew_request(body: &Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/certificates:renew")
        .header("content-type", "application/json")
        .header("idempotency-key", "r1");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn root_of(rig: &CaRig) -> RootFingerprint {
    rig.service.status().unwrap().unwrap().root_fingerprint
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_component_renews_with_its_certificate_and_a_short_call_token() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let old = hermes.serial().unwrap();
    let started = Instant::now();
    let reply = hermes.renew(addr, None).await.unwrap();
    assert!(
        started.elapsed() < StdDuration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    Contract::load().assert_valid("renewCertificate", 200, &reply.body);
    assert_eq!(
        reply.body["data"]["identity"],
        "spiffe://cerbero.test/hermes"
    );
    assert_ne!(hermes.serial().unwrap(), old);
    assert_eq!(renew_rows(&rig), vec![AccessResult::Success]);
    let again = hermes.renew(addr, None).await.unwrap();
    assert_eq!(again.status, 200, "{}", again.body);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_certificate_another_ca_or_an_expired_one_is_auth_and_leaves_a_row() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let contract = Contract::load();
    let (body, _) = hermes.throwaway_renewal(None).unwrap();

    let token = hermes.call_token(None).unwrap();
    let anonymous = hermes
        .send(
            addr,
            &root_of(&rig),
            None,
            renew_request(&body, Some(&token)),
        )
        .await
        .unwrap();
    assert_eq!(
        (anonymous.status, anonymous.body["error"]["code"].as_str()),
        (401, Some("auth"))
    );
    contract.assert_valid("renewCertificate", 401, &anonymous.body);

    // A component of another CA shows its certificate to this Hécate.
    let other = ca_rig();
    let (other_supervisor, _, other_addr) = listening_api(&other).await;
    let foreign = enrolled(&other, &dir, "foreign", other_addr).await;
    let token = foreign.call_token(None).unwrap();
    let refused = hermes
        .send(
            addr,
            &root_of(&rig),
            Some(&foreign),
            renew_request(&body, Some(&token)),
        )
        .await
        .unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);

    // Hécate renews its own certificate on time; the one of the component expires.
    rig.clock.advance(Duration::hours(49));
    supervisor.tick().await.unwrap();
    rig.clock.advance(Duration::hours(24));
    let addr = supervisor.local_addr().await.unwrap();
    let expired = hermes.renew(addr, None).await.unwrap();
    assert_eq!(expired.status, 401, "{}", expired.body);
    assert_eq!(renew_rows(&rig), vec![AccessResult::Failure; 3]);
    other_supervisor.shutdown().await;
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renewing_at_two_thirds_of_one_hour_keeps_the_identity() {
    let rig = ca_rig_configured(CaSettings::new(1));
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    for _ in 0..6 {
        rig.clock.advance(Duration::minutes(40));
        // The task of every minute renews Hécate's certificate, which lives the same hour.
        supervisor.tick().await.unwrap();
        let addr = supervisor.local_addr().await.expect("the API listens");
        let reply = hermes.renew(addr, None).await.unwrap();
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(
            reply.body["data"]["identity"],
            "spiffe://cerbero.test/hermes"
        );
    }
    assert!(
        renew_rows(&rig)
            .iter()
            .all(|result| *result == AccessResult::Success)
    );
    supervisor.shutdown().await;
}
