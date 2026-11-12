//! T034: `GET /v1/status` with the component of the tests (US3, contracts/api.md): the fields of
//! the contract, unsealed and sealed; without the permission or with a token that does not work,
//! 401 with its row, also sealed; a malformed X-Request-Id, 400 with its row, as in feature 003;
//! and `HEAD` as the same operation.

mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::Request;
use common::*;
use hecate_application::ports::Clock;
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::CaRig;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::testing::{Contract, TestComponent};
use serde_json::json;
use tempfile::TempDir;
use time::Duration;

const STATUS: &str = "/v1/status";

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

/// The rows of the status: result, detail and whether each one names who sent it.
fn rows(rig: &CaRig) -> Vec<(AccessResult, String, bool)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::StatusGet)
        .map(|entry| (entry.result, entry.detail, entry.subject.is_some()))
        .collect()
}

/// A request to the status with headers of its own: what a component would not send.
fn request(method: &str, bearer: Option<&str>, request_id: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(STATUS);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(id) = request_id {
        builder = builder.header("x-request-id", id);
    }
    builder.body(Body::empty()).unwrap()
}

fn create(rig: &CaRig, path: &str, options: &CreateOptions) {
    rig.secrets
        .create(path, "a-value", "string", options)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_component_with_the_permission_reads_the_status_also_sealed() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let in_days = |days| json!(isoformat(rig.clock.now() + Duration::days(days)));
    for (path, options) in [
        (
            "secret://hades/old",
            CreateOptions {
                expires_at: in_days(-1),
                ..CreateOptions::default()
            },
        ),
        (
            "secret://hades/soon/a",
            CreateOptions {
                expires_at: in_days(3),
                ..CreateOptions::default()
            },
        ),
        (
            "secret://hades/soon/b",
            CreateOptions {
                expires_at: in_days(4),
                ..CreateOptions::default()
            },
        ),
    ] {
        create(&rig, path, &options);
    }
    for path in [
        "secret://hades/daily/a",
        "secret://hades/daily/b",
        "secret://hades/daily/c",
    ] {
        let daily = CreateOptions {
            rotate_every: json!("P1D"),
            ..CreateOptions::default()
        };
        create(&rig, path, &daily);
    }
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    rig.policies.grant_status("hermes").unwrap();
    rig.clock.advance(Duration::days(1));
    rig.access_log.clear();
    let contract = Contract::load();
    let reply = hermes.status(addr).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    contract.assert_valid("getStatus", 200, &reply.body);
    let mut expected = json!({
        "sealed": false,
        "secrets_expired": 1,
        "secrets_expiring_soon": 2,
        "secrets_due_rotation": 3,
        "identities_expiring_24h": 0,
        "suspicious_renewals_24h": 0,
        "intermediate_days_left": null,
        "enrollment_throttled": false,
    });
    assert_eq!(reply.body["data"], expected);
    rig.sealing.seal().unwrap();
    let sealed = hermes.status(addr).await.unwrap();
    assert_eq!(sealed.status, 200, "{}", sealed.body);
    contract.assert_valid("getStatus", 200, &sealed.body);
    expected["sealed"] = json!(true);
    assert_eq!(sealed.body["data"], expected);
    for body in [reply.body.to_string(), sealed.body.to_string()] {
        assert!(!body.contains("secret://") && !body.contains("hades") && !body.contains("hermes"));
    }
    assert!(rows(&rig).is_empty());
    supervisor.shutdown().await;
}

/// T048: the counts of the identities, the days left to the intermediate and the throttle of the
/// enrollments reach the reply with values other than `0`, `null` and `false`, as the local
/// `status` computes them (US3-AC1, FR-011).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_identities_the_intermediate_and_the_throttle_reach_the_reply() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    // Fewer than 30 days left to the intermediate; Hécate's own certificate, which lasts 3 days,
    // is renewed before the components enroll.
    rig.clock.advance(Duration::days(336));
    supervisor.tick().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    let _zeus = enrolled(&rig, &dir, "zeus", addr).await;
    let hades = enrolled(&rig, &dir, "hades", addr).await;
    rig.policies.grant_status("hermes").unwrap();
    // 49 hours later, the certificates of hermes and zeus, which last 72 hours, expire within 24.
    rig.clock.advance(Duration::hours(49));
    // hades renews and tries again from its previous certificate: a suspicious renewal, which
    // revokes its certificates, so it does not count among the ones that expire.
    let first = hades.serial().unwrap();
    assert_eq!(hades.renew(addr, None).await.unwrap().status, 200);
    let again = hades.renew(addr, Some(&first)).await.unwrap();
    assert_eq!(again.status, 401, "{}", again.body);
    // Ten failed enrollments in a row throttle the enrollments.
    for _ in 0..10 {
        let refused = rig.identities.enroll(
            Some("k1"),
            &json!({"token": "x", "csr": "x", "call_key": "x"}),
        );
        assert!(refused.is_err());
    }
    let days_left = rig
        .service
        .status()
        .unwrap()
        .unwrap()
        .intermediate_days_left;
    assert!(days_left.is_some_and(|days| days < 30), "{days_left:?}");
    let reply = hermes.status(addr).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    Contract::load().assert_valid("getStatus", 200, &reply.body);
    assert_eq!(
        reply.body["data"],
        json!({
            "sealed": false,
            "secrets_expired": 0,
            "secrets_expiring_soon": 0,
            "secrets_due_rotation": 0,
            "identities_expiring_24h": 2,
            "suspicious_renewals_24h": 1,
            "intermediate_days_left": days_left,
            "enrollment_throttled": true,
        })
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_permission_or_with_a_token_that_does_not_work_it_is_401_also_sealed() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    rig.access_log.clear();
    let contract = Contract::load();
    let mut replies = Vec::new();
    for sealed in [false, true] {
        if sealed {
            rig.sealing.seal().unwrap();
        }
        replies.push(hermes.status(addr).await.unwrap());
        let forged = request("GET", Some("not.a.token"), None);
        replies.push(hermes.send_as(addr, None, forged).await.unwrap());
    }
    for reply in &replies {
        assert_eq!(reply.status, 401, "{}", reply.body);
        contract.assert_valid("getStatus", 401, &reply.body);
        assert_eq!(reply.body["error"]["code"], "auth");
    }
    assert_eq!(
        rows(&rig),
        vec![(AccessResult::Failure, "get".to_string(), true); 4]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_request_id_leaves_its_row_and_head_is_the_same_operation() {
    let rig = ca_rig();
    let (supervisor, _, addr) = listening_api(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = enrolled(&rig, &dir, "hermes", addr).await;
    rig.policies.grant_status("hermes").unwrap();
    rig.access_log.clear();
    let token = hermes.call_token(None).unwrap();
    let malformed = request("GET", Some(&token), Some("tab\there"));
    let reply = hermes.send_as(addr, None, malformed).await.unwrap();
    assert_eq!(reply.status, 400, "{}", reply.body);
    Contract::load().assert_valid("getStatus", 400, &reply.body);
    let mut heads = Vec::new();
    for sealed in [false, true] {
        if sealed {
            rig.sealing.seal().unwrap();
        }
        let token = hermes.call_token(None).unwrap();
        let head = request("HEAD", Some(&token), None);
        heads.push(hermes.send_as(addr, None, head).await.unwrap().status);
    }
    let refused = request("HEAD", Some("not.a.token"), None);
    heads.push(hermes.send_as(addr, None, refused).await.unwrap().status);
    assert_eq!(heads, [200, 200, 401]);
    assert_eq!(
        rows(&rig),
        [
            (AccessResult::InvalidInput, "request_id".to_string(), false),
            (AccessResult::Failure, "get".to_string(), true),
        ]
    );
    supervisor.shutdown().await;
}
