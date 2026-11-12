#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::Value;

pub fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors")
}

pub fn vectors(name: &str) -> Value {
    let text = std::fs::read_to_string(vectors_dir().join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn hex(value: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER
        .decode(value.as_str().unwrap().as_bytes())
        .unwrap()
}

use std::sync::Arc;

use hecate_application::ports::CryptoProvider;
use hecate_application::sealing::service::SealingService;
use hecate_application::testing::{GOOD_PASSPHRASE, SealingRig, initialize};
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::control::server::{LineHandler, RequestHandler};
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use serde_json::json;

pub fn crypto() -> Arc<dyn CryptoProvider> {
    Arc::new(SoftwareCryptoProvider::new())
}

pub fn rig() -> SealingRig {
    SealingRig::new(crypto())
}

/// A handler over an initialized (sealed) service; returns the shares too.
pub fn initialized_handler() -> (SealingRig, RequestHandler, Vec<String>) {
    let rig = rig();
    let shares = initialize(&rig.service, GOOD_PASSPHRASE);
    let handler = request_handler(rig.service.clone(), None);
    (rig, handler, shares)
}

pub fn handler_for(service: Arc<SealingService>) -> RequestHandler {
    request_handler(service, None)
}

/// Sends one request through the handler, as the socket would.
pub fn call(handler: &RequestHandler, op: &str, args: Value) -> Value {
    let raw = json!({"request_id": "r", "op": op, "args": args}).to_string();
    serde_json::from_slice(&handler.handle(raw.as_bytes())).unwrap()
}

pub fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap_or("")
}

use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::SecretsRig;

pub fn secrets_rig(unsealed: bool) -> SecretsRig {
    SecretsRig::new(crypto(), unsealed, SecretsSettings::default())
}

/// The channel with both services, as `hecate serve` builds it.
pub fn secrets_handler(rig: &SecretsRig) -> RequestHandler {
    request_handler(rig.sealing.clone(), Some(rig.service.clone()))
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

use std::path::Path;

use hecate_application::testing::FakeClock;
use hecate_infrastructure::testing::SqliteStack;

/// The real adapters over one database file, with a fixed clock.
pub fn stack(db: &Path) -> SqliteStack {
    SqliteStack::open(db, Arc::new(FakeClock::new()))
}

/// A stack initialized and unsealed with the passphrase.
pub fn ready(db: &Path) -> SqliteStack {
    let stack = stack(db);
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    stack
}

pub fn no_options() -> hecate_application::secrets::service::CreateOptions {
    hecate_application::secrets::service::CreateOptions::default()
}

use std::sync::atomic::{AtomicUsize, Ordering};

use hecate_application::ports::Clock;
use hecate_application::testing::CaRig;
use hecate_domain::identity::names::{Serial, TrustDomain};
use hecate_domain::pem;
use hecate_infrastructure::call_token::JwtCallTokenVerifier;
use hecate_infrastructure::control::operations::{
    IdentityOperations, request_handler_with_identities,
};
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};

pub const TRUST_DOMAIN: &str = "cerbero.test";
pub const CA_PASSPHRASE: &str = "orchid lantern quarry 42";

/// An initialized and unsealed Hécate with a CA service and the PKI of rcgen.
pub fn ca_rig() -> CaRig {
    CaRig::new(
        crypto(),
        Arc::new(RcgenPki::new()),
        Arc::new(JwtCallTokenVerifier::new()),
    )
}

/// The channel with the CA, as `hecate serve` builds it, and how many times it said that what
/// the API presents may have changed.
pub fn ca_handler(rig: &CaRig) -> (RequestHandler, Arc<AtomicUsize>) {
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = changes.clone();
    let handler = request_handler_with_identities(
        rig.sealing.clone(),
        None,
        IdentityOperations {
            ca: rig.service.clone(),
            identities: rig.identities.clone(),
            policies: rig.policies.clone(),
            changed: Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        },
    );
    (handler, changes)
}

pub fn changes(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

pub fn ca_serial(byte: u8) -> Serial {
    Serial::from_be_bytes(&[byte; 16]).unwrap()
}

/// What the CLI does with the request that `ca.create.begin` returns: the root CA and the
/// intermediate, in PEM, and the root key.
pub fn sign_request(rig: &CaRig, request: &str) -> (String, String, RootCaKey) {
    let now = rig.clock.now();
    let root_key = RootCaKey::generate().unwrap();
    let trust_domain = TrustDomain::parse(TRUST_DOMAIN).unwrap();
    let root = root_key
        .root_certificate(&trust_domain, &ca_serial(0x9f), now)
        .unwrap();
    let intermediate = root_key
        .sign_intermediate(&root, request, &ca_serial(0x9e), now)
        .unwrap();
    (
        pem::encode("CERTIFICATE", &root),
        pem::encode("CERTIFICATE", &intermediate),
        root_key,
    )
}

/// The whole ceremony through the channel; returns the reply of `ca.create.confirm`.
pub fn create_ca_through(handler: &RequestHandler, rig: &CaRig) -> Value {
    create_ca_keeping_root(handler, rig).0
}

/// The whole ceremony through the channel, keeping the root CA key and its certificate in DER
/// for the renewals of the intermediate.
pub fn create_ca_keeping_root(
    handler: &RequestHandler,
    rig: &CaRig,
) -> (Value, RootCaKey, Vec<u8>) {
    let begun = call(
        handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    );
    let (root, intermediate, root_key) =
        sign_request(rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let confirmed = call(
        handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    );
    let root_der = pem::single(&root, "CERTIFICATE").unwrap();
    (confirmed, root_key, root_der)
}

/// What the CLI signs for a renewal of the intermediate, with the root CA key; in PEM.
pub fn sign_renewal(rig: &CaRig, root_key: &RootCaKey, root_der: &[u8], request: &str) -> String {
    let intermediate = root_key
        .sign_intermediate(root_der, request, &ca_serial(0x9d), rig.clock.now())
        .unwrap();
    pem::encode("CERTIFICATE", &intermediate)
}

use std::net::SocketAddr;

use hecate_infrastructure::api::routes::{refusals, routes, sealed_check};
use hecate_infrastructure::api::supervisor::ApiSupervisor;

/// The API of a rig, with the operations of the components, on a free port of 127.0.0.1.
pub fn api_supervisor(rig: &CaRig) -> Arc<ApiSupervisor> {
    Arc::new(ApiSupervisor::new(
        "127.0.0.1:0".parse().unwrap(),
        rig.service.clone(),
        routes(rig.identities.clone()),
        sealed_check(rig.sealing.clone()),
        refusals(rig.identities.clone()),
    ))
}

/// The channel as `hecate serve` builds it: after a change, the supervisor runs its tick.
pub fn api_handler(rig: &CaRig, supervisor: &Arc<ApiSupervisor>) -> Arc<RequestHandler> {
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
pub async fn operation(handler: &Arc<RequestHandler>, op: &'static str, args: Value) -> Value {
    let handler = handler.clone();
    tokio::task::spawn_blocking(move || call(&handler, op, args))
        .await
        .unwrap()
}

/// A rig with a CA whose API listens; returns where.
pub async fn listening_api(rig: &CaRig) -> (Arc<ApiSupervisor>, Arc<RequestHandler>, SocketAddr) {
    let (supervisor, handler, addr, _) = listening_api_keeping_root(rig).await;
    (supervisor, handler, addr)
}

/// The root CA key of a ceremony and its certificate in DER, as the admin keeps them.
pub struct RootOfCa {
    pub key: RootCaKey,
    pub der: Vec<u8>,
}

/// A rig with a CA whose API listens, keeping the root CA key for renewals of the intermediate.
pub async fn listening_api_keeping_root(
    rig: &CaRig,
) -> (
    Arc<ApiSupervisor>,
    Arc<RequestHandler>,
    SocketAddr,
    RootOfCa,
) {
    let supervisor = api_supervisor(rig);
    let handler = api_handler(rig, &supervisor);
    let begun = operation(
        &handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    )
    .await;
    let (root, intermediate, root_key) =
        sign_request(rig, begun["data"]["intermediate_csr"].as_str().unwrap());
    let confirmed = operation(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": root,
            "intermediate_certificate": intermediate,
        }),
    )
    .await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    let addr = supervisor.local_addr().await.expect("the API listens");
    let root = RootOfCa {
        key: root_key,
        der: pem::single(&root, "CERTIFICATE").unwrap(),
    };
    (supervisor, handler, addr, root)
}

/// The renewal of the intermediate through the channel, as the CLI does it; returns the DER of
/// the new one.
pub async fn renew_intermediate_through(
    rig: &CaRig,
    handler: &Arc<RequestHandler>,
    root: &RootOfCa,
) -> Vec<u8> {
    let begun = operation(handler, "ca.intermediate.begin", json!({})).await;
    let certificate = sign_renewal(
        rig,
        &root.key,
        &root.der,
        begun["data"]["intermediate_csr"].as_str().unwrap(),
    );
    let confirmed = operation(
        handler,
        "ca.intermediate.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "intermediate_certificate": certificate,
        }),
    )
    .await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    pem::single(&certificate, "CERTIFICATE").unwrap()
}

/// A rig whose certificates live as `ca_settings` says.
pub fn ca_rig_configured(ca_settings: hecate_application::ca::settings::CaSettings) -> CaRig {
    CaRig::configured(
        crypto(),
        Arc::new(RcgenPki::new()),
        Arc::new(JwtCallTokenVerifier::new()),
        ca_settings,
        hecate_application::identity::settings::IdentitySettings::default(),
    )
}

use hecate_application::ports::IdentityStore;
use hecate_domain::events::{
    CertificateEvent, CertificateOrigin, EventRecord, IdentityEvent, IssuedCertificate,
    RevocationReason, RevokedBy,
};
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::{IdentityName, TokenId};
use hecate_infrastructure::testing::{TestComponent, short_dir};
use time::Duration;

/// What a year leaves with 30 components that renew every two days: about 5,400 certificates,
/// written straight into the store. With `revoked`, the last one of each component, still valid,
/// is revoked urgently.
pub fn a_year_of_certificates(stack: &SqliteStack, revoked: bool) {
    let now = stack.clock.now();
    for identity in 0..30u8 {
        let name = IdentityName::parse(&format!("component-{identity}")).unwrap();
        let token = TokenId::from_bytes([identity; 8]);
        let mut certificates: Vec<(String, Vec<EventRecord>)> = Vec::new();
        for renewal in 0..180u32 {
            let issued_at = now - Duration::days(360) + Duration::days(i64::from(renewal) * 2);
            let mut serial = [0u8; 16];
            serial[0] = 0x40 | identity;
            serial[12..].copy_from_slice(&renewal.to_be_bytes());
            let serial = Serial::from_be_bytes(&serial).unwrap();
            let mut key = [identity; 32];
            key[..4].copy_from_slice(&renewal.to_be_bytes());
            let issued = IssuedCertificate {
                at: issued_at,
                identity: name.clone(),
                serial: serial.clone(),
                generation: 1,
                intermediate_version: 1,
                epoch: 1,
                not_before: issued_at,
                not_after: issued_at + Duration::hours(72),
                call_key: Some(CallKey::from_bytes(&key).unwrap()),
                origin: if renewal == 0 {
                    CertificateOrigin::Enrollment
                } else {
                    CertificateOrigin::Renewal
                },
                renewed_from: None,
                request_sha256: Some([renewal as u8; 32]),
                idempotency_key: Some(format!("k{renewal}")),
                certificate: vec![0x30; 600],
            };
            let mut events = vec![CertificateEvent::CertificateIssued(Box::new(issued)).encode()];
            if revoked && renewal == 179 {
                events.push(
                    CertificateEvent::CertificateRevoked {
                        at: now,
                        urgent: true,
                        by: RevokedBy::Admin,
                        reason: RevocationReason::Admin,
                    }
                    .encode(),
                );
            }
            certificates.push((format!("certificate/{name}/{serial}"), events));
        }
        let identity_events: Vec<EventRecord> = [
            IdentityEvent::EnrollmentTokenCreated {
                at: now - Duration::days(361),
                token_id: token.clone(),
                secret_sha256: [identity; 32],
                generation: 1,
                expires_at: now - Duration::days(361) + Duration::minutes(15),
            },
            IdentityEvent::IdentityEnrolled {
                at: now - Duration::days(360),
                token_id: token,
                serial: Serial::from_be_bytes(&[0x40 | identity, 1]).unwrap(),
            },
        ]
        .iter()
        .map(IdentityEvent::encode)
        .collect();
        stack
            .identity_store
            .transaction(&mut |writes| {
                writes.append(&format!("identity/{name}"), &identity_events, 0)?;
                for (stream, records) in &certificates {
                    writes.append(stream, records, 0)?;
                }
                Ok(())
            })
            .unwrap();
    }
}

/// A Hécate on SQLite with a year of certificates, whose API listens, and a caller enrolled
/// through it; `dirs` keeps the database and the keys of the caller.
pub struct YearOfData {
    pub caller: TestComponent,
    pub supervisor: ApiSupervisor,
    pub addr: SocketAddr,
    pub stack: SqliteStack,
    pub dirs: (tempfile::TempDir, tempfile::TempDir),
}

pub async fn a_year_of_data(revoked: bool) -> YearOfData {
    let dir = short_dir();
    let clock = Arc::new(FakeClock::new());
    let stack = SqliteStack::open(&dir.path().join("hecate.db"), clock.clone());
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    let begun = stack.ca.create_begin(TRUST_DOMAIN, CA_PASSPHRASE).unwrap();
    let root_key = RootCaKey::generate().unwrap();
    let now = clock.now();
    let root = root_key
        .root_certificate(
            &TrustDomain::parse(TRUST_DOMAIN).unwrap(),
            &ca_serial(0x9f),
            now,
        )
        .unwrap();
    let intermediate = root_key
        .sign_intermediate(&root, &begun.intermediate_csr, &ca_serial(0x9e), now)
        .unwrap();
    stack
        .ca
        .create_confirm(
            &begun.ceremony_id,
            &pem::encode("CERTIFICATE", &root),
            &pem::encode("CERTIFICATE", &intermediate),
        )
        .unwrap();
    a_year_of_certificates(&stack, revoked);
    let supervisor = ApiSupervisor::new(
        "127.0.0.1:0".parse().unwrap(),
        stack.ca.clone(),
        routes(stack.identities.clone()),
        sealed_check(stack.sealing.clone()),
        refusals(stack.identities.clone()),
    );
    supervisor.refresh().await.unwrap();
    let addr = supervisor.local_addr().await.unwrap();
    let caller_dir = tempfile::tempdir().unwrap();
    let caller = TestComponent::open(caller_dir.path(), clock.clone()).unwrap();
    let token = stack
        .identities
        .tokens_create("caller")
        .unwrap()
        .token
        .to_string();
    assert_eq!(caller.enroll(addr, &token).await.unwrap().status, 200);
    YearOfData {
        caller,
        supervisor,
        addr,
        stack,
        dirs: (dir, caller_dir),
    }
}

// --- Feature 006: secrets that the components resolve ------------------------------------------

use hecate_application::ca::settings::CaSettings;
use hecate_application::identity::settings::IdentitySettings;

/// A rig whose identities have settings of their own, such as a lower refusal floor.
pub fn ca_rig_with(settings: IdentitySettings) -> CaRig {
    CaRig::configured(
        crypto(),
        Arc::new(RcgenPki::new()),
        Arc::new(JwtCallTokenVerifier::new()),
        CaSettings::default(),
        settings,
    )
}

/// Stores a secret of type `string` in the rig.
pub fn store_secret(rig: &CaRig, path: &str, value: &str) {
    rig.secrets
        .create(path, value, "string", &no_options())
        .unwrap();
}

/// Revokes a version of a secret of the rig.
pub fn revoke_secret(rig: &CaRig, path: &str, version: u64) {
    let expected = rig.secrets.get(path).unwrap().stream_version;
    rig.secrets
        .revoke(path, &json!(version), &json!(expected))
        .unwrap();
}

/// A Hécate on SQLite, unsealed and with a CA, as after the ceremony.
pub fn sqlite_with_ca(db: &Path, clock: Arc<FakeClock>) -> SqliteStack {
    let stack = SqliteStack::open(db, clock.clone());
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    let begun = stack.ca.create_begin(TRUST_DOMAIN, CA_PASSPHRASE).unwrap();
    let root_key = RootCaKey::generate().unwrap();
    let now = clock.now();
    let root = root_key
        .root_certificate(
            &TrustDomain::parse(TRUST_DOMAIN).unwrap(),
            &ca_serial(0x9f),
            now,
        )
        .unwrap();
    let intermediate = root_key
        .sign_intermediate(&root, &begun.intermediate_csr, &ca_serial(0x9e), now)
        .unwrap();
    stack
        .ca
        .create_confirm(
            &begun.ceremony_id,
            &pem::encode("CERTIFICATE", &root),
            &pem::encode("CERTIFICATE", &intermediate),
        )
        .unwrap();
    stack
}

/// A component enrolled on a stack through the use case, without the API.
pub fn enrolled_on(
    stack: &SqliteStack,
    dir: &Path,
    name: &str,
    clock: Arc<FakeClock>,
) -> TestComponent {
    let component = TestComponent::open(dir, clock).unwrap();
    let token = stack
        .identities
        .tokens_create(name)
        .unwrap()
        .token
        .to_string();
    let (body, key) = component.enrollment(&token).unwrap();
    let enrolled = stack.identities.enroll(Some(&key), &body).unwrap();
    component
        .accept_enrollment(&enrolled.certificate, &enrolled.chain)
        .unwrap();
    component
}
