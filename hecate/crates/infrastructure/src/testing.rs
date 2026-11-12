//! Integration helpers for the tests (feature `test-support`), as in `fakes.py`.

use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::oneshot;

use crate::control::server::{ControlServer, LineHandler};

/// `ControlServer::bind` narrows the process umask while it binds, as the Python version does.
/// A directory that another test thread creates meanwhile would lose its search bit, so the
/// binding in `ServerThread` and the directories of `short_dir` take turns.
static UMASK_TURN: Mutex<()> = Mutex::new(());

fn umask_turn() -> MutexGuard<'static, ()> {
    UMASK_TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A short temporary directory under /tmp: socket paths have a small length limit.
pub fn short_dir() -> tempfile::TempDir {
    let _turn = umask_turn();
    tempfile::Builder::new()
        .prefix("hc")
        .tempdir_in("/tmp")
        .expect("cannot create a temporary directory")
}

/// The control server running in a background thread until dropped.
pub struct ServerThread {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl ServerThread {
    pub fn start(handler: Arc<dyn LineHandler>, socket_path: &Path) -> Self {
        let turn = umask_turn();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let path = socket_path.to_path_buf();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cannot build the server runtime");
            runtime.block_on(async move {
                let server = ControlServer::bind(handler, &path).expect("cannot bind the socket");
                let _ = ready_tx.send(());
                server
                    .serve_until(async {
                        let _ = stop_rx.await;
                    })
                    .await;
            });
        });
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("server did not start");
        drop(turn);
        Self {
            stop: Some(stop_tx),
            thread: Some(thread),
        }
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

use hecate_application::ports::{Clock, SecretStore};
use hecate_application::sealing::service::SealingService;
use hecate_application::sealing::settings::SealingSettings;
use hecate_application::secrets::service::SecretsService;
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::MINIMAL_KDF;

use hecate_application::access_policy::service::AccessPolicyService;
use hecate_application::ca::service::CaService;
use hecate_application::ca::settings::CaSettings;
use hecate_application::identity::service::IdentityService;
use hecate_application::identity::settings::IdentitySettings;

use crate::crypto::SoftwareCryptoProvider;
use crate::pki::RcgenPki;
use crate::store::ca::SqliteIdentityStore;
use crate::store::identity::SqliteEnrollAttempts;
use crate::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use crate::store::secrets::SqliteSecretStore;

/// The real adapters over one database file, as `hecate serve` builds them, with the cheapest
/// key derivation.
pub struct SqliteStack {
    pub clock: Arc<dyn Clock>,
    pub repo: Arc<SqliteSealRepository>,
    pub access_log: Arc<SqliteAccessLog>,
    pub store: Arc<SqliteSecretStore>,
    pub sealing: Arc<SealingService>,
    pub service: Arc<SecretsService>,
    pub identity_store: Arc<SqliteIdentityStore>,
    pub enroll_attempts: Arc<SqliteEnrollAttempts>,
    pub ca: Arc<CaService>,
    pub identities: Arc<IdentityService>,
    pub policies: Arc<AccessPolicyService>,
}

impl SqliteStack {
    pub fn open(db_path: &Path, clock: Arc<dyn Clock>) -> Self {
        Self::open_with(db_path, clock, |store| store)
    }

    /// The same, with the secrets service on a wrapper of the store (to make a write fail).
    pub fn open_with(
        db_path: &Path,
        clock: Arc<dyn Clock>,
        wrap: impl FnOnce(Arc<SqliteSecretStore>) -> Arc<dyn SecretStore>,
    ) -> Self {
        let open = "cannot open the database";
        let repo = Arc::new(SqliteSealRepository::open(db_path).expect(open));
        let access_log = Arc::new(SqliteAccessLog::open(db_path).expect(open));
        let store = Arc::new(SqliteSecretStore::open(db_path).expect(open));
        let crypto = Arc::new(SoftwareCryptoProvider::new());
        let sealing = Arc::new(SealingService::new(
            crypto.clone(),
            repo.clone(),
            access_log.clone(),
            clock.clone(),
            SealingSettings::new(MINIMAL_KDF),
        ));
        let service = Arc::new(SecretsService::new(
            sealing.clone(),
            wrap(store.clone()),
            crypto.clone(),
            access_log.clone(),
            clock.clone(),
            SecretsSettings::default(),
        ));
        let identity_store = Arc::new(SqliteIdentityStore::open(db_path).expect(open));
        let enroll_attempts = Arc::new(SqliteEnrollAttempts::open(db_path).expect(open));
        let pki = Arc::new(RcgenPki::new());
        let ca = Arc::new(CaService::new(
            sealing.clone(),
            identity_store.clone(),
            pki.clone(),
            crypto.clone(),
            access_log.clone(),
            clock.clone(),
            CaSettings::default(),
        ));
        let policies = Arc::new(AccessPolicyService::new(
            sealing.clone(),
            identity_store.clone(),
            store.clone(),
            access_log.clone(),
            clock.clone(),
        ));
        let identities = Arc::new(IdentityService::new(
            sealing.clone(),
            ca.clone(),
            policies.clone(),
            service.clone(),
            identity_store.clone(),
            enroll_attempts.clone(),
            pki,
            crypto,
            access_log.clone(),
            clock.clone(),
            Arc::new(crate::call_token::JwtCallTokenVerifier::new()),
            IdentitySettings::default(),
        ));
        Self {
            clock,
            repo,
            access_log,
            store,
            sealing,
            service,
            identity_store,
            enroll_attempts,
            ca,
            identities,
            policies,
        }
    }

    /// Stops as the process does: the root is released and the connections are closed.
    pub fn close(self) {
        self.sealing.shutdown();
    }
}

use std::path::PathBuf;

use serde_json::Value;

/// The folder of the OpenAPI contracts, inside the package `cerbero-contracts` (research R11).
pub fn contracts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../shared/contracts/src/cerbero_contracts/openapi")
}

/// The OpenAPI contracts of the API, to check each response against its operation and status.
/// The two files are read from disk, so `$ref` between them resolve as published.
pub struct Contract {
    hecate_uri: String,
    common_uri: String,
    hecate: Value,
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A JSON pointer token: `~` and `/` escaped (RFC 6901).
fn pointer_token(text: &str) -> String {
    text.replace('~', "~0").replace('/', "~1")
}

impl Contract {
    pub fn load() -> Self {
        let dir = contracts_dir()
            .canonicalize()
            .expect("the contracts folder exists");
        let hecate_path = dir.join("hecate.v1.json");
        let text = std::fs::read_to_string(&hecate_path).expect("hecate.v1.json is readable");
        Self {
            hecate_uri: file_uri(&hecate_path),
            common_uri: file_uri(&dir.join("common.v1.json")),
            hecate: serde_json::from_str(&text).expect("hecate.v1.json is JSON"),
        }
    }

    /// The URI of a schema of the common envelope (`Failed`, `Completed`, `Error`...).
    pub fn common_schema(&self, name: &str) -> String {
        format!("{}#/components/schemas/{name}", self.common_uri)
    }

    /// The URI of a schema of the API (`ValidationFailure`, `Jwk`...).
    pub fn hecate_schema(&self, name: &str) -> String {
        format!("{}#/components/schemas/{name}", self.hecate_uri)
    }

    fn response_schema(&self, operation_id: &str, status: u16) -> Result<String, String> {
        let paths = self.hecate["paths"]
            .as_object()
            .ok_or("the contract has no paths")?;
        for (path, item) in paths {
            for (method, operation) in item.as_object().into_iter().flatten() {
                if operation["operationId"] != operation_id {
                    continue;
                }
                let response = operation["responses"]
                    .get(status.to_string())
                    .ok_or_else(|| {
                        format!("status {status} is not in the contract of {operation_id}")
                    })?;
                let location = match response.get("$ref").and_then(Value::as_str) {
                    Some(reference) => reference.trim_start_matches('#').to_string(),
                    None => format!("/paths/{}/{method}/responses/{status}", pointer_token(path)),
                };
                return Ok(format!(
                    "{}#{location}/content/application~1json/schema",
                    self.hecate_uri
                ));
            }
        }
        Err(format!("the contract has no operation {operation_id}"))
    }

    /// Checks a response of an operation: its status must be in the contract and its body must
    /// follow the schema of that status.
    pub fn check(&self, operation_id: &str, status: u16, body: &Value) -> Result<(), String> {
        let uri = self.response_schema(operation_id, status)?;
        self.check_schema(&uri, body)
    }

    /// Checks a value against a schema given by its URI: a file and a JSON pointer.
    pub fn check_schema(&self, uri: &str, value: &Value) -> Result<(), String> {
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&serde_json::json!({ "$ref": uri }))
            .map_err(|error| format!("{uri}: {error}"))?;
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|error| format!("{error} at '{}'", error.instance_path()))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub fn assert_valid(&self, operation_id: &str, status: u16, body: &Value) {
        if let Err(errors) = self.check(operation_id, status, body) {
            panic!("{operation_id} {status} does not follow the contract: {errors}\n{body}");
        }
    }
}

// --- The component of the tests (T066) -----------------------------------------------------------

use std::fs::{DirBuilder, OpenOptions};
use std::io;
use std::io::Write;
use std::net::SocketAddr;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use axum::body::Body;
use axum::http::{HeaderMap, Request};
use data_encoding::HEXLOWER;
use hecate_domain::identity::names::{IdentityName, RootFingerprint, Serial, SpiffeId};
use hecate_domain::identity::token::EnrollmentToken;
use hecate_domain::pem;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rcgen::{
    CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256, PKCS_ED25519, PublicKeyData, SanType,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::verify_server_cert_signed_by_trust_anchor;
use rustls::pki_types::{
    CertificateDer, CertificateRevocationListDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName,
    UnixTime,
};
use rustls::server::{ParsedCertificate, VerifierBuilderError, WebPkiClientVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use time::OffsetDateTime;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::revocation_list::CertificateRevocationList;

const TLS_KEY: &str = "tls-key.pem";
const CALL_KEY: &str = "call-key.pem";
const PENDING_REQUEST: &str = "request.pem";
const PENDING_KEY: &str = "idempotency-key";
const LAST_REQUEST: &str = "last-request.pem";
const LAST_KEY: &str = "last-idempotency-key";
const OWN_CERTIFICATE: &str = "certificate.pem";
const OWN_CHAIN: &str = "chain.pem";
const CRLS: &str = "crls.pem";
const ENROLL: &str = "/v1/identities:enroll";

/// The URIs of the SAN of a certificate.
fn uris(der: &[u8]) -> Vec<String> {
    let Ok((_, certificate)) = X509Certificate::from_der(der) else {
        return Vec::new();
    };
    let Ok(Some(names)) = certificate.subject_alternative_name() else {
        return Vec::new();
    };
    names
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::URI(uri) => Some(uri.to_string()),
            _ => None,
        })
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How a component checks Hécate before sending its token (contracts/api.md): the chain ends in
/// the root CA whose fingerprint the token carries, it is valid at the time of the clock that the
/// component receives, and the certificate names `spiffe://<td>/hecate`.
pub struct PinnedHecate {
    fingerprint: RootFingerprint,
    clock: Arc<dyn Clock>,
    provider: Arc<rustls::crypto::CryptoProvider>,
    /// Where it notes the certificate with which Hécate identified itself.
    seen: Option<Arc<Mutex<Option<Vec<u8>>>>>,
}

impl std::fmt::Debug for PinnedHecate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedHecate")
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl PinnedHecate {
    pub fn new(fingerprint: RootFingerprint, clock: Arc<dyn Clock>) -> Self {
        Self {
            fingerprint,
            clock,
            provider: Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
            seen: None,
        }
    }

    /// The same check, noting in `seen` the certificate that Hécate presents.
    pub fn noting(self, seen: Arc<Mutex<Option<Vec<u8>>>>) -> Self {
        Self {
            seen: Some(seen),
            ..self
        }
    }
}

impl ServerCertVerifier for PinnedHecate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let refused = |reason: &str| rustls::Error::General(reason.to_string());
        let root = intermediates
            .iter()
            .find(|certificate| RootFingerprint::of_der(certificate) == self.fingerprint)
            .ok_or_else(|| refused("no root CA with the fingerprint of the token"))?;
        let mut roots = RootCertStore::empty();
        roots.add(root.clone().into_owned())?;
        let seconds = u64::try_from(self.clock.now().unix_timestamp())
            .map_err(|_| refused("the clock is before 1970"))?;
        let now = UnixTime::since_unix_epoch(std::time::Duration::from_secs(seconds));
        verify_server_cert_signed_by_trust_anchor(
            &ParsedCertificate::try_from(end_entity)?,
            &roots,
            intermediates,
            now,
            self.provider.signature_verification_algorithms.all,
        )?;
        let names = uris(end_entity);
        let is_hecate = names.len() == 1
            && SpiffeId::parse(&names[0]).is_ok_and(|id| id.identity().as_str() == "hecate");
        if !is_hecate {
            return Err(refused("the server is not Hécate"));
        }
        if let Some(seen) = &self.seen {
            *lock(seen) = Some(end_entity.to_vec());
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not used".into()))
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
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Why the component got no answer from Hécate.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Key(#[from] rcgen::Error),
    #[error("{0}")]
    Token(#[from] hecate_domain::sealing::errors::HecateError),
    #[error("{0}")]
    Tls(#[from] rustls::Error),
    #[error("cannot reach Hécate: {0}")]
    Unreachable(String),
    #[error("not a CRL: {0}")]
    Crl(String),
    #[error("{0}")]
    Verifier(#[from] VerifierBuilderError),
}

/// What a component reads in a CRL: its number, its times and the serials it revokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrlContents {
    pub number: u64,
    pub this_update: OffsetDateTime,
    pub next_update: Option<OffsetDateTime>,
    pub revoked: Vec<Serial>,
}

/// Reads a CRL in DER; its signature is checked by webpki when a component uses it.
pub fn read_crl(der: &[u8]) -> Result<CrlContents, ComponentError> {
    let (_, crl) = CertificateRevocationList::from_der(der)
        .map_err(|error| ComponentError::Crl(error.to_string()))?;
    let number = crl
        .crl_number()
        .and_then(|number| number.to_string().parse().ok())
        .ok_or_else(|| ComponentError::Crl("without a cRLNumber".to_string()))?;
    let revoked = crl
        .iter_revoked_certificates()
        .map(|entry| Serial::from_be_bytes(entry.raw_serial()))
        .collect::<Result<_, _>>()?;
    Ok(CrlContents {
        number,
        this_update: crl.last_update().to_datetime(),
        next_update: crl.next_update().map(|instant| instant.to_datetime()),
        revoked,
    })
}

/// What Hécate answered: the status, the headers and the envelope.
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Value,
}

/// The component of the tests (T066, T080, contracts/api.md «Lo que hace cada componente»): a
/// P-256 key for its certificate and an Ed25519 key to sign its calls, created on its own machine
/// and kept with its certificates in a directory only its user reads.
///
/// The current keys and certificate live at the top of the directory; a pending renewal keeps
/// its new keys and request in `renewal/` until it is issued, and each certificate replaced by a
/// renewal stays in `history/<serial>/` with its keys.
pub struct TestComponent {
    dir: PathBuf,
    clock: Arc<dyn Clock>,
    /// The certificate with which Hécate identified itself in the last connection.
    hecate: Arc<Mutex<Option<Vec<u8>>>>,
}

/// A copy of the private files of a component, also to be removed.
pub use tempfile::TempDir;

const RENEWAL: &str = "renewal";
const HISTORY: &str = "history";
const RENEW: &str = "/v1/certificates:renew";
const RESOLVE: &str = "/v1/secrets:resolve";

fn random_hex() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("no random bytes"))?;
    Ok(HEXLOWER.encode(&bytes))
}

fn write_private(dir: &Path, name: &str, text: &str) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join(name))?;
    file.write_all(text.as_bytes())
}

fn new_keys(dir: &Path) -> Result<(), ComponentError> {
    write_private(
        dir,
        TLS_KEY,
        &KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?.serialize_pem(),
    )?;
    write_private(
        dir,
        CALL_KEY,
        &KeyPair::generate_for(&PKCS_ED25519)?.serialize_pem(),
    )?;
    Ok(())
}

fn request_text(key: &KeyPair, asks_for: Option<&SpiffeId>) -> Result<String, ComponentError> {
    let mut params = CertificateParams::default();
    if let Some(id) = asks_for {
        params.subject_alt_names = vec![SanType::URI(id.to_string().try_into()?)];
    }
    Ok(pem::encode(
        "CERTIFICATE REQUEST",
        params.serialize_request(key)?.der(),
    ))
}

fn public_key_text(key: &KeyPair) -> String {
    pem::encode("PUBLIC KEY", &key.subject_public_key_info())
}

impl TestComponent {
    /// The component whose state lives in `dir`; its keys are created the first time.
    pub fn open(dir: &Path, clock: Arc<dyn Clock>) -> Result<Self, ComponentError> {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let component = Self {
            dir: dir.to_path_buf(),
            clock,
            hecate: Arc::new(Mutex::new(None)),
        };
        if !component.path(TLS_KEY).exists() {
            new_keys(dir)?;
        }
        Ok(component)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn write(&self, name: &str, text: &str) -> io::Result<()> {
        write_private(&self.dir, name, text)
    }

    fn read(&self, name: &str) -> io::Result<String> {
        std::fs::read_to_string(self.path(name))
    }

    /// The folder of the keys and certificate it presents: the current ones, or those of an
    /// older certificate of its history.
    fn set(&self, from: Option<&Serial>) -> PathBuf {
        match from {
            None => self.dir.clone(),
            Some(serial) => self.dir.join(HISTORY).join(serial.as_str()),
        }
    }

    fn key_in(dir: &Path, name: &str) -> Result<KeyPair, ComponentError> {
        Ok(KeyPair::from_pem(&std::fs::read_to_string(
            dir.join(name),
        )?)?)
    }

    /// The current call key, in PEM `PUBLIC KEY` (RFC 8410).
    pub fn call_key(&self) -> Result<String, ComponentError> {
        Ok(public_key_text(&Self::key_in(&self.dir, CALL_KEY)?))
    }

    /// A new request for the current key, which asks for `asks_for` or for no name; it replaces
    /// the pending enrollment.
    pub fn new_request(&self, asks_for: Option<&SpiffeId>) -> Result<String, ComponentError> {
        let text = request_text(&Self::key_in(&self.dir, TLS_KEY)?, asks_for)?;
        self.write(PENDING_REQUEST, &text)?;
        Ok(text)
    }

    /// The certificate and the intermediate that signed it, in DER: the current ones or those
    /// of an older certificate.
    pub fn certificate_chain_of(
        &self,
        from: Option<&Serial>,
    ) -> Result<Vec<Vec<u8>>, ComponentError> {
        let set = self.set(from);
        let mut chain = vec![pem::single(
            &std::fs::read_to_string(set.join(OWN_CERTIFICATE))?,
            "CERTIFICATE",
        )?];
        let blocks = pem::blocks(&std::fs::read_to_string(set.join(OWN_CHAIN))?)
            .map_err(|_| io::Error::other("the saved chain is not PEM"))?;
        chain.extend(blocks.into_iter().map(|block| block.der));
        Ok(chain)
    }

    /// The certificate and the intermediate that signed it, in DER, once enrolled.
    pub fn certificate_chain(&self) -> Result<Vec<Vec<u8>>, ComponentError> {
        Ok(self
            .certificate_chain_of(None)?
            .into_iter()
            .take(2)
            .collect())
    }

    /// The serial of the current certificate.
    pub fn serial(&self) -> Result<Serial, ComponentError> {
        let chain = self.certificate_chain_of(None)?;
        let (_, certificate) = X509Certificate::from_der(&chain[0])
            .map_err(|_| io::Error::other("the saved certificate is not X.509"))?;
        Ok(Serial::from_be_bytes(certificate.raw_serial())?)
    }

    /// Its SPIFFE ID, from a certificate it keeps.
    pub fn spiffe_id_of(&self, from: Option<&Serial>) -> Result<SpiffeId, ComponentError> {
        let chain = self.certificate_chain_of(from)?;
        let names = uris(&chain[0]);
        Ok(SpiffeId::parse(names.first().map_or("", String::as_str))?)
    }

    /// The root CA of its chain, which it trusts.
    fn root_fingerprint(&self) -> Result<RootFingerprint, ComponentError> {
        let chain = self.certificate_chain_of(None)?;
        let root = chain
            .last()
            .ok_or_else(|| io::Error::other("the saved chain is empty"))?;
        Ok(RootFingerprint::of_der(root))
    }

    /// Keeps a certificate and its chain (the intermediate and the root CA) as the current one.
    pub fn keep(&self, certificate: &[u8], chain: &[Vec<u8>]) -> io::Result<()> {
        self.write(OWN_CERTIFICATE, &pem::encode("CERTIFICATE", certificate))?;
        let text: String = chain
            .iter()
            .map(|der| pem::encode("CERTIFICATE", der))
            .collect();
        self.write(OWN_CHAIN, &text)
    }

    fn keep_answer(&self, data: &Value) -> Result<(), ComponentError> {
        let certificate = pem::single(
            data["certificate"].as_str().unwrap_or_default(),
            "CERTIFICATE",
        )?;
        let chain = data["chain"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|block| pem::single(block.as_str().unwrap_or_default(), "CERTIFICATE"))
            .collect::<Result<Vec<_>, _>>()?;
        self.keep(&certificate, &chain)?;
        Ok(())
    }

    /// A short call token for Hécate with these claims, signed with the call key of the
    /// certificate it presents.
    pub fn call_token_with(
        &self,
        from: Option<&Serial>,
        claims: &Value,
    ) -> Result<String, ComponentError> {
        let key = Self::key_in(&self.set(from), CALL_KEY)?;
        let header = jsonwebtoken::Header {
            typ: Some("JWT".into()),
            ..jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA)
        };
        jsonwebtoken::encode(
            &header,
            claims,
            &jsonwebtoken::EncodingKey::from_ed_der(&key.serialize_der()),
        )
        .map_err(|error| ComponentError::Unreachable(error.to_string()))
    }

    /// The claims of a short call token for Hécate, from now and for 300 s.
    pub fn call_claims(&self, from: Option<&Serial>) -> Result<Value, ComponentError> {
        let id = self.spiffe_id_of(from)?;
        let hecate = SpiffeId::new(id.trust_domain().clone(), IdentityName::hecate());
        let now = self.clock.now().unix_timestamp();
        Ok(serde_json::json!({
            "iss": id.to_string(),
            "aud": hecate.to_string(),
            "iat": now,
            "exp": now + 300,
            "jti": random_hex()?,
        }))
    }

    /// A short call token for Hécate, signed with the call key of the certificate it presents.
    pub fn call_token(&self, from: Option<&Serial>) -> Result<String, ComponentError> {
        self.call_token_with(from, &self.call_claims(from)?)
    }

    /// The body of the pending renewal, with new keys, and its Idempotency-Key: they stay until
    /// the renewal is issued, so an answer that never arrived is asked for again with the same
    /// request instead of a second attempt.
    pub fn renewal(&self) -> Result<(Value, String), ComponentError> {
        let pending = self.dir.join(RENEWAL);
        if !pending.join(PENDING_REQUEST).exists() {
            new_keys(&pending)?;
            let request = request_text(&Self::key_in(&pending, TLS_KEY)?, None)?;
            write_private(&pending, PENDING_REQUEST, &request)?;
            write_private(&pending, PENDING_KEY, &random_hex()?)?;
        }
        let body = serde_json::json!({
            "csr": std::fs::read_to_string(pending.join(PENDING_REQUEST))?,
            "call_key": public_key_text(&Self::key_in(&pending, CALL_KEY)?),
        });
        Ok((body, std::fs::read_to_string(pending.join(PENDING_KEY))?))
    }

    /// The body of a renewal with new keys that are not kept, which asks for `asks_for` or for
    /// no name, and a new Idempotency-Key.
    pub fn throwaway_renewal(
        &self,
        asks_for: Option<&SpiffeId>,
    ) -> Result<(Value, String), ComponentError> {
        let tls = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
        let call = KeyPair::generate_for(&PKCS_ED25519)?;
        let body = serde_json::json!({
            "csr": request_text(&tls, asks_for)?,
            "call_key": public_key_text(&call),
        });
        Ok((body, random_hex()?))
    }

    /// A request for the current key of its certificate, as a renewal that repeats it would send.
    pub fn request_for_current_key(&self) -> Result<String, ComponentError> {
        request_text(&Self::key_in(&self.dir, TLS_KEY)?, None)
    }

    /// After an issued renewal: the current certificate and its keys go to the history, and the
    /// new keys and certificate become the current ones.
    pub fn accept_renewal(
        &self,
        certificate: &[u8],
        chain: &[Vec<u8>],
    ) -> Result<(), ComponentError> {
        let old = self.dir.join(HISTORY).join(self.serial()?.as_str());
        DirBuilder::new().recursive(true).mode(0o700).create(&old)?;
        for name in [TLS_KEY, CALL_KEY, OWN_CERTIFICATE, OWN_CHAIN] {
            std::fs::rename(self.path(name), old.join(name))?;
        }
        let pending = self.dir.join(RENEWAL);
        for name in [TLS_KEY, CALL_KEY] {
            std::fs::rename(pending.join(name), self.path(name))?;
        }
        std::fs::remove_dir_all(&pending)?;
        self.keep(certificate, chain)?;
        Ok(())
    }

    /// The certificate chain and the key that it shows in mTLS.
    fn identity_of(
        &self,
        from: Option<&Serial>,
    ) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), ComponentError> {
        let chain = self
            .certificate_chain_of(from)?
            .into_iter()
            .take(2)
            .map(CertificateDer::from)
            .collect();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            Self::key_in(&self.set(from), TLS_KEY)?.serialize_der(),
        ));
        Ok((chain, key))
    }

    fn client_config(
        &self,
        fingerprint: &RootFingerprint,
        identity: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
    ) -> Result<ClientConfig, ComponentError> {
        let verifier = Arc::new(
            PinnedHecate::new(*fingerprint, self.clock.clone()).noting(self.hecate.clone()),
        );
        let provider = verifier.provider.clone();
        let builder = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(verifier);
        // hyper-rustls sets the ALPN protocols that the connector enables.
        Ok(match identity {
            None => builder.with_no_client_auth(),
            Some((chain, key)) => builder.with_client_auth_cert(chain, key)?,
        })
    }

    async fn send_with(
        &self,
        addr: SocketAddr,
        fingerprint: &RootFingerprint,
        identity: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
        request: Request<Body>,
    ) -> Result<Reply, ComponentError> {
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(self.client_config(fingerprint, identity)?)
            .https_only()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build::<_, Body>(connector);
        let (mut parts, body) = request.into_parts();
        let path = parts
            .uri
            .path_and_query()
            .map_or("/", |path| path.as_str())
            .to_string();
        parts.uri = format!("https://{addr}{path}")
            .parse()
            .map_err(|_| ComponentError::Unreachable("the address is not valid".into()))?;
        let response = client
            .request(Request::from_parts(parts, body))
            .await
            .map_err(|error| ComponentError::Unreachable(format!("{error:?}")))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(Body::new(response.into_body()), usize::MAX)
            .await
            .map_err(|error| ComponentError::Unreachable(error.to_string()))?;
        Ok(Reply {
            status,
            headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        })
    }

    /// Sends a request to Hécate over TLS, after checking it with the fingerprint; with
    /// `present`, it shows the current certificate of that component.
    pub async fn send(
        &self,
        addr: SocketAddr,
        fingerprint: &RootFingerprint,
        present: Option<&TestComponent>,
        request: Request<Body>,
    ) -> Result<Reply, ComponentError> {
        let identity = present.map(|other| other.identity_of(None)).transpose()?;
        self.send_with(addr, fingerprint, identity, request).await
    }

    /// Sends a request showing one of its own certificates: the current one or an older one.
    pub async fn send_as(
        &self,
        addr: SocketAddr,
        from: Option<&Serial>,
        request: Request<Body>,
    ) -> Result<Reply, ComponentError> {
        let identity = self.identity_of(from)?;
        self.send_with(addr, &self.root_fingerprint()?, Some(identity), request)
            .await
    }

    /// The body of an enrollment with a token, and its Idempotency-Key. The request and the key
    /// stay until the certificate is issued, so an answer that never arrived is asked for again
    /// with the same request; once issued, they become the last ones.
    pub fn enrollment(&self, token: &str) -> Result<(Value, String), ComponentError> {
        let csr = match self.read(PENDING_REQUEST) {
            Ok(csr) => csr,
            Err(_) => self.new_request(None)?,
        };
        let key = match self.read(PENDING_KEY) {
            Ok(key) => key,
            Err(_) => {
                let key = random_hex()?;
                self.write(PENDING_KEY, &key)?;
                key
            }
        };
        let body = serde_json::json!({"token": token, "csr": csr, "call_key": self.call_key()?});
        Ok((body, key))
    }

    /// After an issued enrollment: its certificate becomes the current one.
    pub fn accept_enrollment(
        &self,
        certificate: &[u8],
        chain: &[Vec<u8>],
    ) -> Result<(), ComponentError> {
        self.keep(certificate, chain)?;
        std::fs::rename(self.path(PENDING_REQUEST), self.path(LAST_REQUEST))?;
        std::fs::rename(self.path(PENDING_KEY), self.path(LAST_KEY))?;
        Ok(())
    }

    fn post(
        path: &str,
        key: &str,
        bearer: Option<&str>,
        body: &Value,
    ) -> Result<Request<Body>, ComponentError> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .header("idempotency-key", key);
        if let Some(token) = bearer {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder
            .body(Body::from(body.to_string()))
            .map_err(|error| ComponentError::Unreachable(error.to_string()))
    }

    /// Enrolls with a token: it checks Hécate with the fingerprint of the token first.
    pub async fn enroll(&self, addr: SocketAddr, token: &str) -> Result<Reply, ComponentError> {
        self.enroll_presenting(addr, token, None).await
    }

    pub async fn enroll_presenting(
        &self,
        addr: SocketAddr,
        token: &str,
        present: Option<&TestComponent>,
    ) -> Result<Reply, ComponentError> {
        let fingerprint = *EnrollmentToken::parse(token)?.fingerprint();
        let (body, key) = self.enrollment(token)?;
        let request = Self::post(ENROLL, &key, None, &body)?;
        let reply = self.send(addr, &fingerprint, present, request).await?;
        if reply.status == 200 {
            self.keep_answer(&reply.body["data"])?;
            std::fs::rename(self.path(PENDING_REQUEST), self.path(LAST_REQUEST))?;
            std::fs::rename(self.path(PENDING_KEY), self.path(LAST_KEY))?;
        }
        Ok(reply)
    }

    /// Sends again, exactly, the last enrollment that was issued.
    pub async fn repeat_last(
        &self,
        addr: SocketAddr,
        token: &str,
    ) -> Result<Reply, ComponentError> {
        let fingerprint = *EnrollmentToken::parse(token)?.fingerprint();
        let body = serde_json::json!({
            "token": token,
            "csr": self.read(LAST_REQUEST)?,
            "call_key": self.call_key()?,
        });
        let request = Self::post(ENROLL, &self.read(LAST_KEY)?, None, &body)?;
        self.send(addr, &fingerprint, None, request).await
    }

    /// A query of the API with its current certificate and a new short call token.
    pub async fn query(&self, addr: SocketAddr, path: &str) -> Result<Reply, ComponentError> {
        let token = self.call_token(None)?;
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .map_err(|error| ComponentError::Unreachable(error.to_string()))?;
        self.send_as(addr, None, request).await
    }

    /// The keys valid for new calls (`GET /v1/keys`).
    pub async fn keys(&self, addr: SocketAddr) -> Result<Reply, ComponentError> {
        self.query(addr, "/v1/keys").await
    }

    /// Any registered key by its `kid` (`GET /v1/keys/{kid}`).
    pub async fn key(&self, addr: SocketAddr, kid: &str) -> Result<Reply, ComponentError> {
        self.query(addr, &format!("/v1/keys/{kid}")).await
    }

    /// The certificate with which Hécate identified itself in the last connection, in DER.
    pub fn hecate_certificate(&self) -> Option<Vec<u8>> {
        lock(&self.hecate).clone()
    }

    /// The CRLs (`GET /v1/revocations`): it keeps the last ones it receives, refreshes them before
    /// their `expires_at` and goes on with them while Hécate does not answer.
    pub async fn revocations(&self, addr: SocketAddr) -> Result<Reply, ComponentError> {
        let reply = self.query(addr, "/v1/revocations").await?;
        if reply.status == 200 {
            let text: String = reply.body["data"]["crls"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            self.write(CRLS, &text)?;
        }
        Ok(reply)
    }

    /// Keeps CRLs in DER as the ones it checks its peers against.
    pub fn keep_crls(&self, crls: &[Vec<u8>]) -> io::Result<()> {
        let text: String = crls
            .iter()
            .map(|der| pem::encode("X509 CRL", der))
            .collect();
        self.write(CRLS, &text)
    }

    /// The CRLs it keeps, in DER; none before it asks for them.
    pub fn crls(&self) -> Result<Vec<Vec<u8>>, ComponentError> {
        let text = match self.read(CRLS) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let blocks =
            pem::blocks(&text).map_err(|_| io::Error::other("the saved CRLs are not PEM"))?;
        Ok(blocks
            .into_iter()
            .filter(|block| block.label == "X509 CRL")
            .map(|block| block.der)
            .collect())
    }

    /// Whether it accepts the certificate of a peer and its intermediate (contracts/api.md «Lo
    /// que hace cada componente»): webpki checks the chain up to the root CA it trusts, at the
    /// time of its clock, and that the certificate is not in the CRLs it keeps.
    pub fn accepts(&self, chain: &[Vec<u8>]) -> Result<bool, ComponentError> {
        let own = self.certificate_chain_of(None)?;
        let root = own
            .last()
            .ok_or_else(|| io::Error::other("the saved chain is empty"))?;
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(root.clone()))?;
        let verifier = WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        )
        .with_crls(
            self.crls()?
                .into_iter()
                .map(CertificateRevocationListDer::from),
        )
        .only_check_end_entity_revocation()
        .build()?;
        let (peer, intermediates) = chain
            .split_first()
            .ok_or_else(|| io::Error::other("the chain of the peer is empty"))?;
        let intermediates: Vec<CertificateDer<'static>> = intermediates
            .iter()
            .cloned()
            .map(CertificateDer::from)
            .collect();
        let seconds = u64::try_from(self.clock.now().unix_timestamp())
            .map_err(|_| io::Error::other("the clock is before 1970"))?;
        let now = UnixTime::since_unix_epoch(std::time::Duration::from_secs(seconds));
        Ok(verifier
            .verify_client_cert(&CertificateDer::from(peer.clone()), &intermediates, now)
            .is_ok())
    }

    /// What the API offers (`GET /v1/capabilities`).
    pub async fn capabilities(&self, addr: SocketAddr) -> Result<Reply, ComponentError> {
        self.query(addr, "/v1/capabilities").await
    }

    /// The aggregate status of Hécate (`GET /v1/status`), for a component with the status
    /// permission.
    pub async fn status(&self, addr: SocketAddr) -> Result<Reply, ComponentError> {
        self.query(addr, "/v1/status").await
    }

    /// Resolves secrets (`POST /v1/secrets:resolve`) with its current certificate and a new
    /// short call token. Keeping the values in a cache and refreshing them is each component's
    /// own work, outside the tests of Hécate.
    pub async fn resolve(
        &self,
        addr: SocketAddr,
        references: &[&str],
    ) -> Result<Reply, ComponentError> {
        let token = self.call_token(None)?;
        let body = serde_json::json!({ "references": references });
        let request = Request::builder()
            .method("POST")
            .uri(RESOLVE)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .map_err(|error| ComponentError::Unreachable(error.to_string()))?;
        self.send_as(addr, None, request).await
    }

    /// Renews its certificate with new keys and a short call token. With `from`, it shows an
    /// older certificate with a request that is not kept: a second attempt with that one.
    pub async fn renew(
        &self,
        addr: SocketAddr,
        from: Option<&Serial>,
    ) -> Result<Reply, ComponentError> {
        let (body, key) = match from {
            None => self.renewal()?,
            Some(_) => self.throwaway_renewal(None)?,
        };
        let token = self.call_token(from)?;
        let request = Self::post(RENEW, &key, Some(&token), &body)?;
        let reply = self.send_as(addr, from, request).await?;
        if reply.status == 200 && from.is_none() {
            let data = &reply.body["data"];
            let certificate = pem::single(
                data["certificate"].as_str().unwrap_or_default(),
                "CERTIFICATE",
            )?;
            let chain = data["chain"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|block| pem::single(block.as_str().unwrap_or_default(), "CERTIFICATE"))
                .collect::<Result<Vec<_>, _>>()?;
            self.accept_renewal(&certificate, &chain)?;
        }
        Ok(reply)
    }
}
