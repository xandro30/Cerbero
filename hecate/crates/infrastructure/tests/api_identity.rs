//! T033 and T046: the API after the creation of the CA (research R1, R8): no listener without a
//! CA; once confirmed it presents the chain up to the root CA with Hécate's SPIFFE ID; sealed, TLS
//! works and every route answers `unavailable` except `GET /v1/status`, which answers sealed
//! (feature 006, `api_status.rs`); without a valid certificate of its own it does not accept
//! connections, and it listens again as soon as there is a new one.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::*;
use hecate_application::ca::service::CaService;
use hecate_application::ports::PkiProvider;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::identity::names::RootFingerprint;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_infrastructure::api::routes::refusals;
use hecate_infrastructure::api::server::SealedCheck;
use hecate_infrastructure::api::supervisor::ApiSupervisor;
use hecate_infrastructure::control::operations::{
    IdentityOperations, request_handler_with_identities,
};
use hecate_infrastructure::control::server::RequestHandler;
use hecate_infrastructure::pki::RcgenPki;
use hecate_infrastructure::tasks;
use hecate_infrastructure::testing::TestComponent;
use hyper_util::rt::TokioIo;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde_json::{Value, json};
use time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};

const LOCAL: &str = "127.0.0.1:0";

/// Accepts any server certificate but checks the handshake signature: the tests look at the
/// chain that Hécate presents.
#[derive(Debug)]
struct AnyServer(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyServer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn client() -> Arc<ClientConfig> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyServer(provider)))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// The chain that the server presents in the handshake.
async fn presented_chain(addr: SocketAddr) -> Result<Vec<Vec<u8>>, String> {
    let tcp = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let tls = TlsConnector::from(client())
        .connect(ServerName::try_from("hecate.test").unwrap(), tcp)
        .await
        .map_err(|e| e.to_string())?;
    Ok(tls
        .get_ref()
        .1
        .peer_certificates()
        .unwrap_or_default()
        .iter()
        .map(|certificate| certificate.to_vec())
        .collect())
}

async fn get(addr: SocketAddr, path: &str) -> (StatusCode, Value) {
    api_request(addr, "GET", path).await
}

async fn api_request(addr: SocketAddr, method: &str, path: &str) -> (StatusCode, Value) {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let tls = TlsConnector::from(client())
        .connect(ServerName::try_from("hecate.test").unwrap(), tcp)
        .await
        .unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls))
        .await
        .unwrap();
    tokio::spawn(connection);
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "hecate.test")
        .body(Body::empty())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(Body::new(response.into_body()), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn sealed_check(rig: &CaRig) -> SealedCheck {
    hecate_infrastructure::api::routes::sealed_check(rig.sealing.clone())
}

fn supervisor(rig: &CaRig) -> Arc<ApiSupervisor> {
    Arc::new(ApiSupervisor::new(
        LOCAL.parse().unwrap(),
        rig.service.clone(),
        Router::new(),
        sealed_check(rig),
        refusals(rig.identities.clone()),
    ))
}

/// The channel as `hecate serve` builds it: after a change, the supervisor runs its tick.
fn handler(rig: &CaRig, supervisor: &Arc<ApiSupervisor>) -> Arc<RequestHandler> {
    let runtime = tokio::runtime::Handle::current();
    let supervisor = supervisor.clone();
    Arc::new(request_handler_with_identities(
        rig.sealing.clone(),
        None,
        IdentityOperations {
            ca: rig.service.clone(),
            identities: rig.identities.clone(),
            policies: rig.policies.clone(),
            changed: Arc::new(move || {
                runtime.block_on(supervisor.tick()).unwrap();
            }),
        },
    ))
}

/// One operation on the channel, from a blocking thread as the control server runs them.
async fn operation(handler: &Arc<RequestHandler>, op: &'static str, args: Value) -> Value {
    let handler = handler.clone();
    tokio::task::spawn_blocking(move || call(&handler, op, args))
        .await
        .unwrap()
}

async fn create_ca(rig: &CaRig, handler: &Arc<RequestHandler>) -> Value {
    let begun = operation(
        handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    )
    .await;
    let (root, intermediate, _) =
        sign_request(rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    operation(
        handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    )
    .await
}

async fn listening(supervisor: &ApiSupervisor) -> SocketAddr {
    supervisor.local_addr().await.expect("the API listens")
}

fn spiffe_ids(der: &[u8]) -> Vec<String> {
    let (_, certificate) = X509Certificate::from_der(der).unwrap();
    certificate
        .subject_alternative_name()
        .unwrap()
        .map(|names| {
            names
                .value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    GeneralName::URI(uri) => Some(uri.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_ca_there_is_no_listener() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    supervisor.refresh().await.unwrap();
    assert_eq!(supervisor.local_addr().await, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn once_confirmed_the_api_presents_the_chain_up_to_the_root_ca() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    let handler = handler(&rig, &supervisor);
    let confirmed = create_ca(&rig, &handler).await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    let chain = presented_chain(listening(&supervisor).await).await.unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(
        RootFingerprint::of_der(&chain[2]).to_string(),
        confirmed["data"]["root_fingerprint"].as_str().unwrap()
    );
    assert_eq!(spiffe_ids(&chain[0]), ["spiffe://cerbero.test/hecate"]);
    let pki = RcgenPki::new();
    assert!(pki.is_signed_by(&chain[0], &chain[1]).unwrap());
    assert!(pki.is_signed_by(&chain[1], &chain[2]).unwrap());
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sealed_the_tls_works_and_every_route_answers_unavailable() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    let handler = handler(&rig, &supervisor);
    create_ca(&rig, &handler).await;
    operation(&handler, "seal", json!({})).await;
    let addr = listening(&supervisor).await;
    for path in ["/v1/keys", "/v1/certificates/renew", "/anything"] {
        let (status, body) = get(addr, path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(body["error"]["code"], "unavailable");
    }
    // T130: a `HEAD` to a `GET` route is that operation.
    let (status, _) = api_request(addr, "HEAD", "/v1/revocations").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    // T127 (FR-024): only `GET /v1/keys` and `HEAD /v1/revocations` are operations, and their
    // refusals leave their rows.
    let refused: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.detail == "sealed")
        .map(|entry| (entry.operation, entry.method, entry.result, entry.subject))
        .collect();
    assert_eq!(
        refused,
        vec![
            (
                Operation::KeyList,
                Method::Identity,
                AccessResult::RejectedConflict,
                None
            ),
            (
                Operation::RevocationList,
                Method::Identity,
                AccessResult::RejectedConflict,
                None
            ),
        ]
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_a_restart_with_a_ca_the_api_listens_sealed() {
    let rig = ca_rig();
    let first = supervisor(&rig);
    create_ca(&rig, &handler(&rig, &first)).await;
    first.shutdown().await;
    let restarted = rig.restart();
    let second = supervisor(&restarted);
    second.refresh().await.unwrap();
    let addr = listening(&second).await;
    assert_eq!(presented_chain(addr).await.unwrap().len(), 3);
    assert_eq!(
        get(addr, "/v1/keys").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    second.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_certificate_closes_the_listener_until_there_is_a_new_one() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    let handler = handler(&rig, &supervisor);
    create_ca(&rig, &handler).await;
    let old_addr = listening(&supervisor).await;
    operation(&handler, "seal", json!({})).await;
    rig.clock.advance(Duration::hours(73));
    supervisor.tick().await.unwrap();
    assert_eq!(supervisor.local_addr().await, None);
    assert!(presented_chain(old_addr).await.is_err());
    let unsealed = operation(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE})).await;
    assert_eq!(unsealed["status"], "completed");
    let addr = listening(&supervisor).await;
    assert_eq!(presented_chain(addr).await.unwrap().len(), 3);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_intermediate_expired_the_api_stops_listening_also_unsealed() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    let handler = handler(&rig, &supervisor);
    // A root CA that ends in two hours caps the intermediate and Hécate's certificate.
    let begun = operation(
        &handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    )
    .await;
    let now = hecate_application::ports::Clock::now(rig.clock.as_ref());
    let root_key = hecate_infrastructure::pki::RootCaKey::generate().unwrap();
    let root = root_key
        .root_certificate(
            &hecate_domain::identity::names::TrustDomain::parse(TRUST_DOMAIN).unwrap(),
            &ca_serial(0x9f),
            now - Duration::days(3650) + Duration::hours(2),
        )
        .unwrap();
    let intermediate = root_key
        .sign_intermediate(
            &root,
            begun["data"]["intermediate_csr"].as_str().unwrap(),
            &ca_serial(0x9e),
            now,
        )
        .unwrap();
    let confirmed = operation(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": hecate_domain::pem::encode("CERTIFICATE", &root),
            "intermediate_certificate": hecate_domain::pem::encode("CERTIFICATE", &intermediate),
        }),
    )
    .await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    listening(&supervisor).await;
    rig.clock.advance(Duration::hours(2));
    supervisor.tick().await.unwrap();
    assert_eq!(supervisor.local_addr().await, None);
    supervisor.shutdown().await;
}

async fn wait_until(supervisor: &ApiSupervisor, listening: bool) {
    for _ in 0..200 {
        if supervisor.local_addr().await.is_some() == listening {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    panic!("the API did not reach listening = {listening}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_task_of_every_minute_renews_and_closes_or_reopens_the_listener() {
    let rig = ca_rig();
    let supervisor = supervisor(&rig);
    create_ca(&rig, &handler(&rig, &supervisor)).await;
    let periodic = tasks::spawn_periodic(
        StdDuration::from_millis(20),
        rig.service.clone() as Arc<CaService>,
        rig.identities.clone(),
        supervisor.clone(),
    );
    rig.sealing.seal().unwrap();
    rig.clock.advance(Duration::hours(73));
    wait_until(&supervisor, false).await;
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    wait_until(&supervisor, true).await;
    let status = rig.service.status().unwrap().unwrap();
    let own = status.hecate_certificate.unwrap();
    assert!(own.not_after > hecate_application::ports::Clock::now(rig.clock.as_ref()));
    periodic.stop().await;
    supervisor.shutdown().await;
}

// --- After renewing the intermediate (T100) -----------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_renewing_the_intermediate_the_tls_presents_the_one_that_signed_hecates_certificate()
{
    let rig = ca_rig();
    let (supervisor, handler, addr, root) = listening_api_keeping_root(&rig).await;
    let before = presented_chain(addr).await.unwrap();
    let new_intermediate = renew_intermediate_through(&rig, &handler, &root).await;
    // Hécate's certificate is still the one that the previous intermediate signed.
    let chain = presented_chain(listening(&supervisor).await).await.unwrap();
    assert_eq!(chain[1], before[1]);
    assert_ne!(chain[1], new_intermediate);
    // At two thirds of its life Hécate renews it with the new one, which the TLS presents then.
    rig.clock.advance(Duration::hours(49));
    supervisor.tick().await.unwrap();
    let chain = presented_chain(listening(&supervisor).await).await.unwrap();
    assert_eq!(chain[1], new_intermediate);
    assert!(RcgenPki::new().is_signed_by(&chain[0], &chain[1]).unwrap());
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_certificate_of_the_previous_intermediate_renews_into_one_of_the_new_one() {
    let rig = ca_rig();
    let (supervisor, handler, addr, root) = listening_api_keeping_root(&rig).await;
    let dir = tempfile::tempdir().unwrap();
    let hermes = TestComponent::open(dir.path(), rig.clock.clone()).unwrap();
    let token = rig
        .identities
        .tokens_create("hermes")
        .unwrap()
        .token
        .to_string();
    assert_eq!(hermes.enroll(addr, &token).await.unwrap().status, 200);
    let old_intermediate = hermes.certificate_chain().unwrap()[1].clone();
    let new_intermediate = renew_intermediate_through(&rig, &handler, &root).await;
    assert_ne!(old_intermediate, new_intermediate);
    let reply = hermes.renew(addr, None).await.unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(hermes.certificate_chain().unwrap()[1], new_intermediate);
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_task_of_every_minute_destroys_the_previous_key_once_its_certificates_expire() {
    let rig = ca_rig();
    let (supervisor, handler, _, root) = listening_api_keeping_root(&rig).await;
    renew_intermediate_through(&rig, &handler, &root).await;
    let periodic = tasks::spawn_periodic(
        StdDuration::from_millis(20),
        rig.service.clone() as Arc<CaService>,
        rig.identities.clone(),
        supervisor.clone(),
    );
    assert_eq!(rig.store.ca_material_count(), 2);
    rig.sealing.seal().unwrap();
    rig.clock.advance(Duration::hours(73));
    for _ in 0..200 {
        if rig.store.ca_material_count() == 1 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    assert_eq!(rig.store.ca_material_count(), 1, "the previous key is gone");
    let ca = rig.service.authority().unwrap().unwrap();
    assert_eq!(
        ca.intermediate(1, 1).unwrap().state,
        hecate_domain::ca::authority::IntermediateState::Retired
    );
    periodic.stop().await;
    supervisor.shutdown().await;
}

/// T124 (FR-005): the task destroys the previous key at the instant its last certificate expires,
/// not at its next run, which here comes a minute later.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_previous_key_goes_at_the_instant_its_last_certificate_expires() {
    let rig = ca_rig();
    let (supervisor, handler, _, root) = listening_api_keeping_root(&rig).await;
    renew_intermediate_through(&rig, &handler, &root).await;
    // Hécate's certificate, the last one of the previous intermediate, ends in half a second.
    rig.clock
        .advance(Duration::hours(72) - Duration::milliseconds(500));
    let periodic = tasks::spawn_periodic(
        StdDuration::from_secs(60),
        rig.service.clone() as Arc<CaService>,
        rig.identities.clone(),
        supervisor.clone(),
    );
    // The task plans the instant first; then the clock reaches it.
    tokio::time::sleep(StdDuration::from_millis(100)).await;
    rig.clock.advance(Duration::milliseconds(500));
    for _ in 0..300 {
        if rig.store.ca_material_count() == 1 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    assert_eq!(rig.store.ca_material_count(), 1, "the previous key is gone");
    periodic.stop().await;
    supervisor.shutdown().await;
}

/// T126: a retirement that fails is tried again at the next run, not at once: a store that keeps
/// failing does not make the task spin.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retirement_that_fails_waits_for_the_next_run() {
    let rig = ca_rig();
    let (supervisor, handler, _, root) = listening_api_keeping_root(&rig).await;
    renew_intermediate_through(&rig, &handler, &root).await;
    rig.clock.advance(Duration::hours(73));
    rig.store.fail_commits(true);
    let before = rig.store.transactions();
    let periodic = tasks::spawn_periodic(
        StdDuration::from_secs(60),
        rig.service.clone() as Arc<CaService>,
        rig.identities.clone(),
        supervisor.clone(),
    );
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    let attempts = rig.store.transactions() - before;
    periodic.stop().await;
    rig.store.fail_commits(false);
    assert_eq!(
        attempts, 1,
        "one attempt at the instant, the next one at the next run"
    );
    assert_eq!(rig.store.ca_material_count(), 2);
    supervisor.shutdown().await;
}
