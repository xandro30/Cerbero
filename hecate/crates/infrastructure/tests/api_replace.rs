//! T106: the API after the replacement of the CA (FR-006, SC-006, SC-010): the TLS presents the
//! chain of the new CA, a certificate of the previous generation is `auth` with its row, and the
//! component of the tests enrolls again with a new token.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::json;

/// The replacement through the channel, as `hecate ca replace` does it; returns the new
/// fingerprint.
async fn replace_through(rig: &CaRig, handler: &Arc<RequestHandler>) -> RootFingerprint {
    let begun = operation(
        handler,
        "ca.replace.begin",
        json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "ca_passphrase": "harbor violet engine 77"}),
    )
    .await;
    assert_eq!(begun["status"], "completed", "{begun}");
    let (root, intermediate, _) =
        sign_request(rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let confirmed = operation(
        handler,
        "ca.replace.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    )
    .await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    RootFingerprint::of_der(&hecate_domain::pem::single(&root, "CERTIFICATE").unwrap())
}

async fn enrolled(
    rig: &CaRig,
    dir: &tempfile::TempDir,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_the_replacement_the_tls_presents_the_new_chain_and_a_new_token_enrolls() {
    let rig = ca_rig();
    let (supervisor, handler, _) = listening_api(&rig).await;
    let fingerprint = replace_through(&rig, &handler).await;
    let addr = supervisor.local_addr().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let material = rig.service.tls_material().unwrap().unwrap();
    assert_eq!(hermes.hecate_certificate(), Some(material.chain[0].clone()));
    assert_eq!(RootFingerprint::of_der(&material.chain[2]), fingerprint);
    assert_eq!(
        RootFingerprint::of_der(hermes.certificate_chain_of(None).unwrap().last().unwrap()),
        fingerprint
    );
    assert_eq!(hermes.keys(addr).await.unwrap().status, 200);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_certificate_of_the_previous_generation_is_auth_and_leaves_a_row() {
    let rig = ca_rig();
    let (supervisor, handler, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let fingerprint = replace_through(&rig, &handler).await;
    let addr = supervisor.local_addr().await.unwrap();
    let token = hermes.call_token(None).unwrap();
    let request = Request::builder()
        .uri("/v1/keys")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    // It trusts the new root CA, and shows the certificate that the previous one issued.
    let refused = hermes
        .send(addr, &fingerprint, Some(&hermes), request)
        .await
        .unwrap();
    assert_eq!(refused.status, 401, "{}", refused.body);
    Contract::load().assert_valid("listKeys", 401, &refused.body);
    let rows: Vec<(AccessResult, Option<String>)> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::KeyList)
        .map(|entry| (entry.result, entry.identity))
        .collect();
    assert_eq!(
        rows,
        vec![(AccessResult::Failure, Some("hermes".to_string()))]
    );
    let renewed = hermes.renew(addr, None).await;
    assert!(
        renewed.is_err(),
        "it still pins the previous root CA, which Hécate no longer presents"
    );
    supervisor.shutdown().await;
}
