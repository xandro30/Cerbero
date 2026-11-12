//! The TLS server of the API (research R1): TLS 1.3 only, a client certificate that is asked for
//! but not required and whose chain the application validates, HTTP/1.1 and HTTP/2, and a TLS
//! configuration that changes without restarting. What it refuses before an operation runs
//! (Hécate sealed, a malformed request id) reaches a recorder that leaves its row (FR-024).

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Extension, Request, State};
use axum::http::Method;
use axum::middleware::{self, Next};
use axum::response::Response;
use hecate_application::identity::service::CallRefusal;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    DigitallySignedStruct, DistinguishedName, RootCertStore, ServerConfig, SignatureScheme,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use crate::api::envelope::{ApiError, RequestContext, SEALED, request_context};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether a request is refused because Hécate is sealed, given its method and its path: while it
/// is sealed, every route answers `unavailable` except the ones that answer sealed (FR-012).
pub type SealedCheck = Arc<dyn Fn(&Method, &str) -> bool + Send + Sync>;

/// Leaves the row of a request refused before its operation ran, given its method and its
/// path, which say what operation it is (FR-024). It may answer in place of the refusal, as the
/// throttle of the enrollments does (FR-034).
pub type RefusalRecorder =
    Arc<dyn Fn(&Method, &str, CallRefusal) -> Option<ApiError> + Send + Sync>;

/// Hands a refusal to the recorder on a blocking thread, since it writes the access log, and
/// returns what to answer in its place, if anything.
pub(crate) async fn record_refusal(
    recorder: RefusalRecorder,
    method: Method,
    path: String,
    refusal: CallRefusal,
) -> Option<ApiError> {
    match tokio::task::spawn_blocking(move || recorder(&method, &path, refusal)).await {
        Ok(answer) => answer,
        Err(_) => {
            tracing::error!(target: "hecate.api", "cannot leave the row of a refused request");
            None
        }
    }
}

/// What the guard of Hécate sealed needs.
#[derive(Clone)]
struct SealedGuard {
    sealed: SealedCheck,
    refused: RefusalRecorder,
}

/// The TLS configuration that the next connections use.
pub type SharedConfig = Arc<RwLock<Arc<ServerConfig>>>;

/// Hécate's certificate chain and key, and the root CAs that the client certificates name.
pub struct ServerTls {
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
    pub client_roots: Vec<CertificateDer<'static>>,
}

/// The client certificates of the connection, as each request receives them; empty without one.
#[derive(Debug, Clone, Default)]
pub struct PeerCertificates(pub Vec<CertificateDer<'static>>);

/// Asks for a client certificate without requiring it and only checks that the client has its
/// key (the signature of the handshake). The application validates the chain, the validity and
/// the revocation, so a certificate that is not valid gets `auth` and a row (research R1).
#[derive(Debug)]
pub struct PossessionVerifier {
    subjects: Vec<DistinguishedName>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ClientCertVerifier for PossessionVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        false
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.subjects
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// TLS 1.3 only, with aws-lc-rs, HTTP/2 or HTTP/1.1 by ALPN and the verifier of possession.
pub fn server_config(tls: ServerTls) -> Result<ServerConfig, rustls::Error> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    for root in tls.client_roots {
        roots.add(root)?;
    }
    let verifier = Arc::new(PossessionVerifier {
        subjects: roots.subjects(),
        algorithms: provider.signature_verification_algorithms,
    });
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(verifier)
        .with_single_cert(tls.chain, tls.key)?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

async fn sealed_guard(State(guard): State<SealedGuard>, request: Request, next: Next) -> Response {
    match request.extensions().get::<RequestContext>().cloned() {
        Some(context) if (guard.sealed)(request.method(), request.uri().path()) => {
            let (method, path) = (request.method().clone(), request.uri().path().to_string());
            let answer = record_refusal(guard.refused, method, path, CallRefusal::Sealed).await;
            context.failed(&answer.unwrap_or_else(|| ApiError::Unavailable(SEALED.to_string())))
        }
        _ => next.run(request).await,
    }
}

/// What answers a request that is not an operation of the API, a path that does not exist or a
/// method that its route does not admit: `not_found` in the common envelope (principle II),
/// without a row, since it is not an operation.
async fn not_an_operation(context: RequestContext) -> Response {
    context.failed(&ApiError::NotFound("no such operation".into()))
}

/// The routes with what every route shares: the request context, the guard of Hécate sealed,
/// which tell `refused` what they refuse, and the answer to what is not an operation.
pub fn app(routes: Router, sealed: SealedCheck, refused: RefusalRecorder) -> Router {
    let guard = SealedGuard {
        sealed,
        refused: refused.clone(),
    };
    routes
        .fallback(not_an_operation)
        .method_not_allowed_fallback(not_an_operation)
        .layer(middleware::from_fn_with_state(guard, sealed_guard))
        .layer(middleware::from_fn_with_state(refused, request_context))
}

/// The API listening on its address until [`ApiServer::shutdown`].
pub struct ApiServer {
    local_addr: SocketAddr,
    config: SharedConfig,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl ApiServer {
    pub async fn bind(
        addr: SocketAddr,
        config: Arc<ServerConfig>,
        app: Router,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        let config: SharedConfig = Arc::new(RwLock::new(config));
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(accept(listener, config.clone(), app, stopped));
        Ok(Self {
            local_addr,
            config,
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The configuration of the next connections: a new certificate or a new root CA.
    pub async fn replace_config(&self, config: Arc<ServerConfig>) {
        *self.config.write().await = config;
    }

    /// Stops accepting connections and closes the listener; it can be bound again later.
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

async fn accept(
    listener: TcpListener,
    config: SharedConfig,
    app: Router,
    mut stopped: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => match accepted {
                Ok((tcp, peer)) => {
                    let config = config.read().await.clone();
                    tokio::spawn(serve(tcp, peer, config, app.clone()));
                }
                Err(error) => {
                    tracing::warn!(target: "hecate.api", reason = %error, "cannot accept a connection");
                }
            },
        }
    }
}

/// One connection: the TLS handshake, the client certificates in each request and HTTP. A
/// rejected handshake never reaches the application: it leaves a structured log line.
async fn serve(tcp: TcpStream, peer: SocketAddr, config: Arc<ServerConfig>, app: Router) {
    let handshake = tokio::time::timeout(HANDSHAKE_TIMEOUT, TlsAcceptor::from(config).accept(tcp));
    let tls = match handshake.await {
        Ok(Ok(tls)) => tls,
        Ok(Err(error)) => {
            tracing::warn!(target: "hecate.api", peer = %peer, reason = %error, "TLS handshake rejected");
            return;
        }
        Err(_) => {
            tracing::warn!(target: "hecate.api", peer = %peer, reason = "timeout", "TLS handshake rejected");
            return;
        }
    };
    let certificates = tls
        .get_ref()
        .1
        .peer_certificates()
        .map(|chain| {
            chain
                .iter()
                .map(|certificate| certificate.clone().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let service = TowerToHyperService::new(app.layer(Extension(PeerCertificates(certificates))));
    let mut builder = auto::Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT);
    if let Err(error) = builder.serve_connection(TokioIo::new(tls), service).await {
        tracing::debug!(target: "hecate.api", peer = %peer, reason = %error, "connection closed");
    }
}
