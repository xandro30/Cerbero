//! The skeleton of the API (research R1 and R11): TLS 1.3 only, a client certificate that is
//! optional and that the application validates, the common envelope, standard JSON of up to
//! 64 KiB, the request id and the guard of Hécate sealed, which tell the recorder of refusals
//! what they refuse (T127).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Extension, Path};
use axum::response::Response;
use axum::routing::{get, post};
use hecate_application::identity::service::CallRefusal;
use hecate_infrastructure::api::envelope::{ApiError, RequestContext, wiped_json};
use hecate_infrastructure::api::json::{JsonBody, MAX_BODY};
use hecate_infrastructure::api::server::{
    ApiServer, PeerCertificates, RefusalRecorder, SealedCheck, ServerTls, app, server_config,
};
use hecate_infrastructure::logs::render;
use hecate_infrastructure::testing::Contract;
use hyper::header::HeaderMap;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, SanType,
};
use rustls::client::ResolvesClientCert;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::sign::CertifiedKey;
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme, SupportedProtocolVersion};
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use zeroize::Zeroizing;

// --- A throwaway PKI -------------------------------------------------------------------------

struct Ca {
    params: CertificateParams,
    key: KeyPair,
    der: CertificateDer<'static>,
}

fn ca(name: &str) -> Ca {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.distinguished_name.push(DnType::CommonName, name);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let der = params.self_signed(&key).unwrap().der().clone();
    Ca { params, key, der }
}

fn leaf(
    issuer: &Ca,
    spiffe_id: &str,
    not_after: OffsetDateTime,
) -> (CertificateDer<'static>, KeyPair) {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params
        .distinguished_name
        .push(DnType::CommonName, spiffe_id);
    params.subject_alt_names = vec![SanType::URI(spiffe_id.try_into().unwrap())];
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    params.not_before = not_after - Duration::days(2);
    params.not_after = not_after;
    let issuer = Issuer::from_params(&issuer.params, &issuer.key);
    (params.signed_by(&key, &issuer).unwrap().der().clone(), key)
}

fn pkcs8(key: &KeyPair) -> PrivateKeyDer<'static> {
    PrivatePkcs8KeyDer::from(key.serialize_der()).into()
}

fn tomorrow() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::days(1)
}

// --- The server with test routes -------------------------------------------------------------

async fn peer(context: RequestContext, Extension(peer): Extension<PeerCertificates>) -> Response {
    context
        .completed(json!({"client_certificate": !peer.0.is_empty(), "certificates": peer.0.len()}))
}

async fn echo(context: RequestContext, JsonBody(body): JsonBody) -> Response {
    context.completed(json!({ "received": body }))
}

async fn error(context: RequestContext, Path(code): Path<String>) -> Response {
    let error = match code.as_str() {
        "validation" => ApiError::Validation("a bad request".into()),
        "auth" => ApiError::Auth("not authenticated".into()),
        "not_found" => ApiError::NotFound("no such key".into()),
        "conflict" => ApiError::Conflict("the key was used".into()),
        "rate_limited" => ApiError::RateLimited {
            message: "enrollments are throttled".into(),
            retry_after_s: 60,
        },
        "unavailable" => ApiError::Unavailable("Hécate is sealed".into()),
        _ => ApiError::Internal,
    };
    context.failed(&error)
}

async fn secrets(context: RequestContext) -> Response {
    context.completed_with_secrets(&json!({
        "secrets": [{"reference": "secret://hermes/telegram/bot_token", "value": "s3cr3t"}],
    }))
}

/// The owner of the bytes of an answer as the tests watch it: it notes when it is dropped.
struct Watched {
    bytes: Zeroizing<Vec<u8>>,
    dropped: Arc<AtomicBool>,
}

impl AsRef<[u8]> for Watched {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

/// What the recorder of refusals received: the method, the path and why.
type Refused = Arc<Mutex<Vec<(String, String, CallRefusal)>>>;

/// What the guard of Hécate sealed asked about: the method and the path of each request.
type Asked = Arc<Mutex<Vec<(String, String)>>>;

struct Running {
    server: ApiServer,
    addr: SocketAddr,
    ca: Ca,
    sealed: Arc<AtomicBool>,
    refused: Refused,
    asked: Asked,
    /// Whether the owner of the bytes of `/test/watched` was dropped, and whether it was still
    /// alive when the handler answered.
    dropped: Arc<AtomicBool>,
    alive_when_answered: Arc<AtomicBool>,
}

async fn start() -> Running {
    let ca = ca("Test Root CA");
    let (certificate, key) = leaf(&ca, "spiffe://cerbero.test/hecate", tomorrow());
    let config = server_config(ServerTls {
        chain: vec![certificate, ca.der.clone()],
        key: pkcs8(&key),
        client_roots: vec![ca.der.clone()],
    })
    .unwrap();
    let sealed = Arc::new(AtomicBool::new(false));
    let flag = sealed.clone();
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    let noted_asks = asked.clone();
    // While sealed, every route is refused but `/test/open`, which answers sealed as the status
    // does.
    let check: SealedCheck = Arc::new(move |method: &Method, path: &str| {
        noted_asks
            .lock()
            .unwrap()
            .push((method.to_string(), path.to_string()));
        flag.load(Ordering::SeqCst) && path != "/test/open"
    });
    let refused: Refused = Arc::new(Mutex::new(Vec::new()));
    let noted = refused.clone();
    // Like the throttle of the enrollments, it answers in place of one refusal.
    let recorder: RefusalRecorder =
        Arc::new(move |method: &Method, path: &str, refusal: CallRefusal| {
            noted
                .lock()
                .unwrap()
                .push((method.to_string(), path.to_string(), refusal));
            (*method == Method::POST && path == "/test/echo" && refusal == CallRefusal::RequestId)
                .then(|| ApiError::RateLimited {
                    message: "enrollments are throttled".into(),
                    retry_after_s: 60,
                })
        });
    let dropped = Arc::new(AtomicBool::new(false));
    let alive_when_answered = Arc::new(AtomicBool::new(false));
    let (owner_dropped, owner_alive) = (dropped.clone(), alive_when_answered.clone());
    let watched = move |context: RequestContext| {
        let (dropped, alive) = (owner_dropped.clone(), owner_alive.clone());
        async move {
            let response =
                context.completed_owned(&json!({"value": "s3cr3t-value"}), |bytes| Watched {
                    bytes,
                    dropped: dropped.clone(),
                });
            alive.store(!dropped.load(Ordering::SeqCst), Ordering::SeqCst);
            response
        }
    };
    let routes = Router::new()
        .route("/test/peer", get(peer))
        .route("/test/open", get(peer))
        .route("/test/echo", post(echo))
        .route("/test/error/{code}", get(error))
        .route("/test/secrets", get(secrets))
        .route("/test/watched", get(watched));
    let server = ApiServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        Arc::new(config),
        app(routes, check, recorder),
    )
    .await
    .unwrap();
    let addr = server.local_addr();
    Running {
        server,
        addr,
        ca,
        sealed,
        refused,
        asked,
        dropped,
        alive_when_answered,
    }
}

fn refusal(method: &str, path: &str, why: CallRefusal) -> (String, String, CallRefusal) {
    (method.to_string(), path.to_string(), why)
}

// --- A client --------------------------------------------------------------------------------

/// Accepts any server certificate but checks the handshake signature: these tests look at the
/// server, not at how a component checks Hécate.
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

/// A fixed client certificate and signing key, which may not match: rustls does not check it.
#[derive(Debug)]
struct FixedClient(Arc<CertifiedKey>);

impl ResolvesClientCert for FixedClient {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }

    fn has_certs(&self) -> bool {
        true
    }
}

fn client(
    versions: &[&'static SupportedProtocolVersion],
    identity: Option<(Vec<CertificateDer<'static>>, &KeyPair)>,
) -> ClientConfig {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(versions)
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyServer(provider.clone())));
    let mut config = match identity {
        None => builder.with_no_client_auth(),
        Some((chain, key)) => {
            let signing_key = provider.key_provider.load_private_key(pkcs8(key)).unwrap();
            let certified = Arc::new(CertifiedKey::new(chain, signing_key));
            builder.with_client_cert_resolver(Arc::new(FixedClient(certified)))
        }
    };
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

static TLS13_ONLY: [&SupportedProtocolVersion; 1] = [&rustls::version::TLS13];

fn tls13() -> &'static [&'static SupportedProtocolVersion] {
    &TLS13_ONLY
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

async fn send(
    addr: SocketAddr,
    config: ClientConfig,
    request: Request<Body>,
) -> Result<Reply, String> {
    let tcp = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let tls = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("hecate.test").unwrap(), tcp)
        .await
        .map_err(|e| e.to_string())?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls))
        .await
        .map_err(|e| e.to_string())?;
    tokio::spawn(connection);
    let response = sender
        .send_request(request)
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(Body::new(response.into_body()), usize::MAX)
        .await
        .map_err(|e| e.to_string())?;
    let body = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn get_request(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("host", "hecate.test")
        .body(Body::empty())
        .unwrap()
}

fn post_request(path: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "hecate.test")
        .header("content-type", "application/json")
        .body(body.into())
        .unwrap()
}

// --- Logs ------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        self.0.lock().unwrap().push(render(event));
    }
}

impl Capture {
    async fn wait_for(&self, text: &str) -> String {
        for _ in 0..100 {
            if let Some(line) = self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|line| line.contains(text))
            {
                return line.clone();
            }
            tokio::time::sleep(StdDuration::from_millis(20)).await;
        }
        panic!("no log line with {text:?}: {:?}", self.0.lock().unwrap());
    }
}

fn capture() -> (Capture, tracing::subscriber::DefaultGuard) {
    let capture = Capture::default();
    let guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(capture.clone()));
    (capture, guard)
}

fn failure_schema(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => "ValidationFailure",
        401 => "AuthFailure",
        404 => "NotFoundFailure",
        409 => "ConflictFailure",
        429 => "RateLimitedFailure",
        503 => "UnavailableFailure",
        _ => "InternalFailure",
    }
}

/// Checks an answer against the contract: the envelope of a success, or the failure of its
/// status.
fn assert_contract(reply: &Reply) {
    let contract = Contract::load();
    let uri = if reply.status == StatusCode::OK {
        contract.common_schema("Completed")
    } else {
        contract.hecate_schema(failure_schema(reply.status))
    };
    if let Err(errors) = contract.check_schema(&uri, &reply.body) {
        panic!("{}: {errors}\n{}", reply.status, reply.body);
    }
}

// --- Tests -----------------------------------------------------------------------------------

#[tokio::test]
async fn only_tls_1_3_connects() {
    let running = start().await;
    let reply = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/peer"),
    )
    .await
    .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    let tls12: &[&'static SupportedProtocolVersion] = &[&rustls::version::TLS12];
    assert!(
        send(running.addr, client(tls12, None), get_request("/test/peer"))
            .await
            .is_err()
    );
    running.server.shutdown().await;
}

#[tokio::test]
async fn the_client_certificate_is_optional_and_any_reaches_the_request() {
    let running = start().await;
    let reply = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/peer"),
    )
    .await
    .unwrap();
    assert_eq!(
        reply.body["data"],
        json!({"client_certificate": false, "certificates": 0})
    );
    assert_contract(&reply);
    let other = ca("Another Root CA");
    let valid = leaf(&running.ca, "spiffe://cerbero.test/hermes", tomorrow());
    let expired = leaf(
        &running.ca,
        "spiffe://cerbero.test/hermes",
        OffsetDateTime::now_utc() - Duration::hours(1),
    );
    let foreign = leaf(&other, "spiffe://cerbero.test/hermes", tomorrow());
    for (certificate, key) in [&valid, &expired, &foreign] {
        let identity = Some((vec![certificate.clone()], key));
        let reply = send(
            running.addr,
            client(tls13(), identity),
            get_request("/test/peer"),
        )
        .await
        .unwrap();
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(
            reply.body["data"],
            json!({"client_certificate": true, "certificates": 1})
        );
    }
    running.server.shutdown().await;
}

#[tokio::test]
async fn a_handshake_signature_that_does_not_match_the_certificate_is_rejected_and_logged() {
    let (capture, _guard) = capture();
    let running = start().await;
    let (certificate, _key) = leaf(&running.ca, "spiffe://cerbero.test/hermes", tomorrow());
    let wrong_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let identity = Some((vec![certificate], &wrong_key));
    assert!(
        send(
            running.addr,
            client(tls13(), identity),
            get_request("/test/peer")
        )
        .await
        .is_err()
    );
    let line = capture.wait_for("TLS handshake rejected").await;
    assert!(line.contains("\"reason\""), "{line}");
    assert!(line.contains("\"peer\""), "{line}");
    let tls12: &[&'static SupportedProtocolVersion] = &[&rustls::version::TLS12];
    let before = capture.0.lock().unwrap().len();
    assert!(
        send(running.addr, client(tls12, None), get_request("/test/peer"))
            .await
            .is_err()
    );
    for _ in 0..100 {
        if capture.0.lock().unwrap().len() > before {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    assert!(
        capture.0.lock().unwrap()[before..]
            .iter()
            .any(|line| line.contains("TLS handshake rejected"))
    );
    running.server.shutdown().await;
}

#[tokio::test]
async fn bodies_are_standard_json_of_up_to_64_kib() {
    let (capture, _guard) = capture();
    let running = start().await;
    let reply = send(
        running.addr,
        client(tls13(), None),
        post_request("/test/echo", r#"{"a": [1, "dos"]}"#),
    )
    .await
    .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["data"]["received"], json!({"a": [1, "dos"]}));
    let padding = MAX_BODY - r#"{"p":""}"#.len();
    let largest = format!(r#"{{"p":"{}"}}"#, "x".repeat(padding));
    assert_eq!(largest.len(), MAX_BODY);
    let reply = send(
        running.addr,
        client(tls13(), None),
        post_request("/test/echo", largest),
    )
    .await
    .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    let too_large = format!(r#"{{"p":"{}"}}"#, "x".repeat(padding + 1));
    let reply = send(
        running.addr,
        client(tls13(), None),
        post_request("/test/echo", too_large),
    )
    .await
    .unwrap();
    assert_eq!(
        (reply.status, &reply.body["error"]["code"]),
        (StatusCode::BAD_REQUEST, &json!("validation"))
    );
    assert_contract(&reply);
    for not_json in ["{not json, secret-word", "", "{\"a\": NaN}"] {
        let reply = send(
            running.addr,
            client(tls13(), None),
            post_request("/test/echo", not_json),
        )
        .await
        .unwrap();
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{not_json:?}");
        assert_eq!(reply.body["error"]["code"], "validation");
        assert_contract(&reply);
    }
    let line = capture.wait_for("not standard JSON").await;
    assert!(!line.contains("secret-word"), "{line}");
    running.server.shutdown().await;
}

#[tokio::test]
async fn the_request_id_is_repeated_or_generated_and_follows_its_rule() {
    let running = start().await;
    let mut request = get_request("/test/peer");
    request
        .headers_mut()
        .insert("x-request-id", "req-123".parse().unwrap());
    let reply = send(running.addr, client(tls13(), None), request)
        .await
        .unwrap();
    assert_eq!(reply.body["meta"]["request_id"], "req-123");
    let reply = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/peer"),
    )
    .await
    .unwrap();
    let generated = reply.body["meta"]["request_id"].as_str().unwrap();
    assert_eq!(generated.len(), 32, "{generated}");
    assert!(generated.bytes().all(|b| b.is_ascii_hexdigit()));
    for bad in ["a".repeat(129), "tab\there".to_string()] {
        let mut request = get_request("/test/peer");
        request
            .headers_mut()
            .insert("x-request-id", bad.parse().unwrap());
        let reply = send(running.addr, client(tls13(), None), request)
            .await
            .unwrap();
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{bad:?}");
        assert_eq!(reply.body["error"]["code"], "validation");
        assert_contract(&reply);
    }
    // T127: only the malformed ones reach the recorder of refusals.
    assert_eq!(
        *running.refused.lock().unwrap(),
        vec![refusal("GET", "/test/peer", CallRefusal::RequestId); 2]
    );
    running.server.shutdown().await;
}

/// T129: what the recorder answers replaces the refusal, as the throttle of the enrollments does.
#[tokio::test]
async fn the_answer_of_the_recorder_replaces_the_refusal() {
    let running = start().await;
    let mut request = post_request("/test/echo", "{}");
    request
        .headers_mut()
        .insert("x-request-id", "tab\there".parse().unwrap());
    let reply = send(running.addr, client(tls13(), None), request)
        .await
        .unwrap();
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(reply.body["error"]["code"], "rate_limited");
    assert_eq!(reply.body["error"]["retry_after_s"], 60);
    assert_eq!(reply.headers["retry-after"], "60");
    assert_contract(&reply);
    assert_eq!(
        *running.refused.lock().unwrap(),
        vec![refusal("POST", "/test/echo", CallRefusal::RequestId)]
    );
    running.server.shutdown().await;
}

/// T131 (principle II): what is not an operation also answers the common envelope: a path that
/// does not exist and a method that the route does not admit are `not_found`, and they reach no
/// recorder of refusals, since they are not operations.
#[tokio::test]
async fn what_is_not_an_operation_answers_not_found_with_the_envelope() {
    let running = start().await;
    for request in [
        get_request("/nope"),
        post_request("/test/peer", "{}"),
        get_request("/test/echo"),
    ] {
        let target = format!("{} {}", request.method(), request.uri());
        let reply = send(running.addr, client(tls13(), None), request)
            .await
            .unwrap();
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{target}");
        assert_eq!(reply.body["error"]["code"], "not_found", "{target}");
        assert_eq!(reply.body["error"]["retryable"], false, "{target}");
        assert!(reply.body["meta"]["request_id"].is_string(), "{target}");
        assert_contract(&reply);
    }
    assert!(running.refused.lock().unwrap().is_empty());
    running.server.shutdown().await;
}

#[tokio::test]
async fn while_sealed_every_route_answers_unavailable() {
    let running = start().await;
    running.sealed.store(true, Ordering::SeqCst);
    for request in [get_request("/test/peer"), post_request("/test/echo", "{}")] {
        let reply = send(running.addr, client(tls13(), None), request)
            .await
            .unwrap();
        assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(reply.body["error"]["code"], "unavailable");
        assert_eq!(reply.body["error"]["retryable"], true);
        assert_contract(&reply);
    }
    // A malformed request id is refused first, also while sealed.
    let mut request = get_request("/test/peer");
    request
        .headers_mut()
        .insert("x-request-id", "tab\there".parse().unwrap());
    let reply = send(running.addr, client(tls13(), None), request)
        .await
        .unwrap();
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    // T127: each refusal reaches the recorder with its method, its path and why.
    assert_eq!(
        *running.refused.lock().unwrap(),
        vec![
            refusal("GET", "/test/peer", CallRefusal::Sealed),
            refusal("POST", "/test/echo", CallRefusal::Sealed),
            refusal("GET", "/test/peer", CallRefusal::RequestId),
        ]
    );
    running.sealed.store(false, Ordering::SeqCst);
    let reply = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/peer"),
    )
    .await
    .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(running.refused.lock().unwrap().len(), 3);
    running.server.shutdown().await;
}

#[tokio::test]
async fn the_guard_asks_per_request_and_lets_through_the_routes_that_answer_sealed() {
    let running = start().await;
    running.sealed.store(true, Ordering::SeqCst);
    let open = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/open"),
    )
    .await
    .unwrap();
    assert_eq!(open.status, StatusCode::OK);
    let closed = send(
        running.addr,
        client(tls13(), None),
        post_request("/test/echo", "{}"),
    )
    .await
    .unwrap();
    assert_eq!(closed.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        *running.asked.lock().unwrap(),
        vec![
            ("GET".to_string(), "/test/open".to_string()),
            ("POST".to_string(), "/test/echo".to_string()),
        ]
    );
    assert_eq!(
        *running.refused.lock().unwrap(),
        vec![refusal("POST", "/test/echo", CallRefusal::Sealed)]
    );
    running.server.shutdown().await;
}

#[test]
fn an_answer_with_secrets_is_written_in_a_buffer_of_exactly_its_size() {
    let value = json!({"secrets": [{"value": "a".repeat(1000)}], "count": 1});
    let bytes = wiped_json(&value).unwrap();
    assert_eq!(bytes.as_slice(), serde_json::to_vec(&value).unwrap());
    assert_eq!(bytes.capacity(), bytes.len());
}

#[tokio::test]
async fn the_buffer_of_an_answer_with_secrets_lives_until_hyper_drops_the_body() {
    let running = start().await;
    assert!(!running.dropped.load(Ordering::SeqCst));
    let reply = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/watched"),
    )
    .await
    .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["data"]["value"], "s3cr3t-value");
    // The body owns the buffer: it was still alive when the handler answered, so it is no copy.
    assert!(running.alive_when_answered.load(Ordering::SeqCst));
    // Hyper drops the body once it is written, and the buffer with it.
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(5);
    while !running.dropped.load(Ordering::SeqCst) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the body was never dropped"
        );
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    running.server.shutdown().await;
}

#[tokio::test]
async fn an_answer_with_secrets_carries_the_same_envelope_as_the_others() {
    let running = start().await;
    let mut request = get_request("/test/secrets");
    request
        .headers_mut()
        .insert("x-request-id", "req-secrets".parse().unwrap());
    let reply = send(running.addr, client(tls13(), None), request)
        .await
        .unwrap();
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.headers["content-type"], "application/json");
    assert_eq!(reply.body["status"], "completed");
    assert_eq!(
        reply.body["data"],
        json!({"secrets": [{"reference": "secret://hermes/telegram/bot_token", "value": "s3cr3t"}]})
    );
    assert_eq!(reply.body["error"], Value::Null);
    assert_eq!(reply.body["meta"]["request_id"], "req-secrets");
    assert!(reply.body["meta"]["duration_ms"].is_u64());
    let plain = send(
        running.addr,
        client(tls13(), None),
        get_request("/test/peer"),
    )
    .await
    .unwrap();
    let members = |body: &Value| {
        body.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(members(&reply.body), members(&plain.body));
    assert_eq!(members(&reply.body["meta"]), members(&plain.body["meta"]));
    assert_contract(&reply);
    running.server.shutdown().await;
}

#[tokio::test]
async fn each_error_code_has_its_http_status() {
    let running = start().await;
    for (code, status) in [
        ("validation", StatusCode::BAD_REQUEST),
        ("auth", StatusCode::UNAUTHORIZED),
        ("not_found", StatusCode::NOT_FOUND),
        ("conflict", StatusCode::CONFLICT),
        ("rate_limited", StatusCode::TOO_MANY_REQUESTS),
        ("unavailable", StatusCode::SERVICE_UNAVAILABLE),
        ("internal", StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let reply = send(
            running.addr,
            client(tls13(), None),
            get_request(&format!("/test/error/{code}")),
        )
        .await
        .unwrap();
        assert_eq!(
            (reply.status, reply.body["error"]["code"].as_str()),
            (status, Some(code))
        );
        assert_contract(&reply);
        let retry_after = reply
            .headers
            .get("retry-after")
            .map(|value| value.to_str().unwrap());
        if code == "rate_limited" {
            assert_eq!(retry_after, Some("60"));
            assert_eq!(reply.body["error"]["retry_after_s"], 60);
        } else {
            assert_eq!(retry_after, None, "{code}");
        }
    }
    running.server.shutdown().await;
}

#[tokio::test]
async fn closing_the_server_frees_its_port() {
    let running = start().await;
    let addr = running.addr;
    running.server.shutdown().await;
    assert!(TcpStream::connect(addr).await.is_err());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    drop(listener);
}
