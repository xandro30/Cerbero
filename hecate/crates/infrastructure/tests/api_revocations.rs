//! T088: `GET /v1/revocations` with the component of the tests (contracts/api.md): the CRLs in
//! PEM and `expires_at`, no row when served and a row when refused (FR-024), a component that
//! stops accepting a certificate revoked urgently once it refreshes (SC-005), the new chain of
//! Hécate after its certificate is replaced (SC-015), and a year of data within a second.

mod common;

use std::net::SocketAddr;
use std::time::{Duration as StdDuration, Instant};

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::CaRig;
use hecate_domain::identity::names::Serial;
use hecate_domain::instant::parse_iso;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent, read_crl};
use serde_json::json;
use tempfile::TempDir;
use time::Duration;

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

fn rows(rig: &CaRig, operation: Operation) -> Vec<AccessResult> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .map(|entry| entry.result)
        .collect()
}

/// The serials of the CRLs that the component keeps.
fn kept_serials(component: &TestComponent) -> Vec<Serial> {
    component
        .crls()
        .unwrap()
        .iter()
        .flat_map(|der| read_crl(der).unwrap().revoked)
        .collect()
}

/// Hécate's certificate and its intermediate, as the API presents them now.
fn own_chain(rig: &CaRig) -> Vec<Vec<u8>> {
    let material = rig.service.tls_material().unwrap().unwrap();
    material.chain[..2].to_vec()
}

async fn revoke_urgently(
    handler: &std::sync::Arc<hecate_infrastructure::control::server::RequestHandler>,
    serial: &Serial,
) {
    let revoked = operation(
        handler,
        "certificates.revoke",
        json!({"serial": serial.as_str(), "urgent": true}),
    )
    .await;
    assert_eq!(revoked["status"], "completed", "{revoked}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_crls_answer_the_fields_of_the_contract() {
    let rig = ca_rig();
    let (supervisor, handler, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let hades = enrolled(&rig, &dir, "hades", addr).await;
    revoke_urgently(&handler, &hades.serial().unwrap()).await;
    let reply = hermes.revocations(addr).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    Contract::load().assert_valid("listRevocations", 200, &reply.body);
    assert_eq!(reply.body["data"]["crls"].as_array().unwrap().len(), 1);
    let expires = parse_iso(reply.body["data"]["expires_at"].as_str().unwrap()).unwrap();
    let whole = rig.clock.now().replace_nanosecond(0).unwrap();
    assert_eq!(expires, whole + Duration::seconds(300));
    assert_eq!(kept_serials(&hermes), vec![hades.serial().unwrap()]);
    let capabilities = hermes.capabilities(addr).await.unwrap();
    assert_eq!(
        capabilities.body["data"]["operations"],
        json!([
            "identities:enroll",
            "certificates:renew",
            "revocations",
            "keys",
            "keys/{kid}",
            "capabilities"
        ])
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_served_crl_leaves_no_row_and_a_refused_one_does() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    assert_eq!(hermes.revocations(addr).await.unwrap().status, 200);
    assert!(rows(&rig, Operation::RevocationList).is_empty());
    let no_token = Request::builder()
        .uri("/v1/revocations")
        .body(Body::empty())
        .unwrap();
    let refused = hermes.send_as(addr, None, no_token).await.unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);
    Contract::load().assert_valid("listRevocations", 401, &refused.body);
    assert_eq!(
        rows(&rig, Operation::RevocationList),
        vec![AccessResult::Failure]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_component_stops_accepting_a_certificate_revoked_urgently_once_it_refreshes() {
    let rig = ca_rig();
    let (supervisor, handler, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let hades = enrolled(&rig, &dir, "hades", addr).await;
    assert_eq!(hades.revocations(addr).await.unwrap().status, 200);
    let chain = hermes.certificate_chain().unwrap();
    assert!(hades.accepts(&chain).unwrap());
    revoke_urgently(&handler, &hermes.serial().unwrap()).await;
    assert!(
        hades.accepts(&chain).unwrap(),
        "until it refreshes, it goes on with its copy"
    );
    assert_eq!(hades.revocations(addr).await.unwrap().status, 200);
    assert!(!hades.accepts(&chain).unwrap());
    let keys = hades.keys(addr).await.unwrap();
    let identities: Vec<&str> = keys.body["data"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key["cerbero_identity"].as_str().unwrap())
        .collect();
    assert_eq!(identities, vec!["spiffe://cerbero.test/hades"]);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_hecates_certificate_is_replaced_a_component_sees_the_new_chain_and_the_old_serial() {
    let rig = ca_rig();
    let (supervisor, handler, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let old_chain = own_chain(&rig);
    assert_eq!(hermes.hecate_certificate(), Some(old_chain[0].clone()));
    let old = rig
        .service
        .status()
        .unwrap()
        .unwrap()
        .hecate_certificate
        .unwrap()
        .serial;
    revoke_urgently(&handler, &old).await;
    let new_chain = own_chain(&rig);
    assert_ne!(new_chain[0], old_chain[0]);
    let addr = supervisor.local_addr().await.unwrap();
    let reply = hermes.revocations(addr).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(hermes.hecate_certificate(), Some(new_chain[0].clone()));
    assert_eq!(kept_serials(&hermes), vec![old]);
    assert!(hermes.accepts(&new_chain).unwrap());
    assert!(!hermes.accepts(&old_chain).unwrap());
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_a_year_of_data_the_crls_answer_within_a_second() {
    let YearOfData {
        caller,
        supervisor,
        addr,
        stack,
        dirs: _dirs,
    } = a_year_of_data(true).await;
    let started = Instant::now();
    let reply = caller.revocations(addr).await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(reply.status, 200, "{}", reply.body);
    // The last certificate of each component, revoked urgently, has not expired.
    assert_eq!(kept_serials(&caller).len(), 30);
    assert!(elapsed < StdDuration::from_secs(1), "{elapsed:?}");
    supervisor.shutdown().await;
    stack.close();
}
