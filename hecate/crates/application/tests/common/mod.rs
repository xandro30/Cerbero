#![allow(dead_code)]

use std::sync::Arc;

use hecate_application::errors::ServiceError;
use hecate_application::ports::CryptoProvider;
use hecate_application::sealing::service::SealingService;
use hecate_application::testing::{GOOD_PASSPHRASE, SealingRig, initialize};
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;

pub fn crypto() -> Arc<dyn CryptoProvider> {
    Arc::new(SoftwareCryptoProvider::new())
}

pub fn rig() -> SealingRig {
    SealingRig::new(crypto())
}

/// A rig already initialized: the service is sealed; returns the three shares too.
pub fn initialized() -> (SealingRig, Vec<String>) {
    let rig = rig();
    let shares = initialize(&rig.service, GOOD_PASSPHRASE);
    (rig, shares)
}

/// A rig initialized and unsealed with the passphrase.
pub fn unsealed() -> (SealingRig, Vec<String>) {
    let (rig, shares) = initialized();
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    (rig, shares)
}

pub fn fresh_service() -> Arc<SealingService> {
    rig().service
}

pub fn foreign_shares() -> Vec<String> {
    initialize(&fresh_service(), GOOD_PASSPHRASE)
}

pub fn is_invalid(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Hecate(HecateError::InvalidInput(_)))
}

pub fn is_auth(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    )
}

pub fn conflict_state(error: &ServiceError) -> Option<SealState> {
    match error {
        ServiceError::Hecate(HecateError::StateConflict { state, .. }) => *state,
        _ => None,
    }
}

pub fn retry_after(error: &ServiceError) -> Option<u64> {
    match error {
        ServiceError::Hecate(HecateError::RateLimited { retry_after_s }) => Some(*retry_after_s),
        _ => None,
    }
}

pub fn entries_of(rig: &SealingRig, operation: Operation) -> Vec<AccessLogEntry> {
    rig.log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .collect()
}

pub fn with_result(entries: &[AccessLogEntry], result: AccessResult) -> usize {
    entries
        .iter()
        .filter(|entry| entry.result == result)
        .count()
}

use hecate_application::ports::{EventStore, MaterialStore};
use hecate_application::secrets::service::{CreateOptions, Created, Resolved};
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::SecretsRig;
use hecate_domain::events::EventRecord;
use serde_json::{Value, json};

pub fn secrets() -> SecretsRig {
    SecretsRig::new(crypto(), true, SecretsSettings::default())
}

pub fn sealed_secrets() -> SecretsRig {
    SecretsRig::new(crypto(), false, SecretsSettings::default())
}

pub fn create(rig: &SecretsRig, path: &str, value: &str, value_type: &str) -> Created {
    rig.service
        .create(path, value, value_type, &CreateOptions::default())
        .unwrap()
}

pub fn stream_of(path: &str) -> String {
    format!("secret/{}", path.trim_start_matches("secret://"))
}

pub fn events(rig: &SecretsRig, stream: &str) -> Vec<EventRecord> {
    rig.store.load(stream).unwrap()
}

pub fn kinds(records: &[EventRecord]) -> Vec<String> {
    records.iter().map(|record| record.kind.clone()).collect()
}

/// The number of events of a secret, read from the store without leaving a row.
pub fn current(rig: &SecretsRig, path: &str) -> Value {
    json!(events(rig, &stream_of(path)).len())
}

pub fn has_material(rig: &SecretsRig, path: &str, version: u64) -> bool {
    rig.store
        .load_secret_material(path, version)
        .unwrap()
        .is_some()
}

pub fn states(rig: &SecretsRig, path: &str) -> Vec<(u64, &'static str)> {
    rig.service
        .get(path)
        .unwrap()
        .versions
        .iter()
        .map(|version| (version.version, version.state.as_str()))
        .collect()
}

pub fn state_of(rig: &SecretsRig, path: &str, version: u64) -> &'static str {
    states(rig, path)
        .into_iter()
        .find(|(number, _)| *number == version)
        .unwrap()
        .1
}

pub fn resolve(
    rig: &SecretsRig,
    identity: &str,
    references: &[&str],
) -> Result<Vec<Resolved>, ServiceError> {
    rig.service.resolve(identity, &json!(references))
}

pub fn resolve_one(rig: &SecretsRig, reference: &str) -> Resolved {
    resolve(rig, "hermes", &[reference]).unwrap().remove(0)
}

pub fn can_resolve(rig: &SecretsRig, reference: &str) -> bool {
    match resolve(rig, "hermes", &[reference]) {
        Ok(_) => true,
        Err(error) if is_not_found(&error) => false,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

pub fn is_not_found(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Hecate(HecateError::NotFound(_)))
}

pub fn is_conflict(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Hecate(HecateError::StateConflict { .. })
    )
}

pub fn current_version(error: &ServiceError) -> Option<u64> {
    match error {
        ServiceError::Hecate(HecateError::StateConflict {
            current_version, ..
        }) => *current_version,
        _ => None,
    }
}

pub fn is_fingerprint(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 4
        && groups.iter().all(|group| {
            group.len() == 4
                && group
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

pub fn failures(rig: &SecretsRig) -> u32 {
    rig.sealing.status().unwrap().consecutive_failures
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

use hecate_application::ca::service::CeremonyBegun;
use hecate_application::ca::settings::CaSettings;
use hecate_application::identity::settings::IdentitySettings;
use hecate_application::ports::Clock;
use hecate_application::testing::CaRig;
use hecate_domain::identity::names::{Serial, TrustDomain};
use hecate_domain::pem;
use hecate_infrastructure::call_token::JwtCallTokenVerifier;
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};
use time::OffsetDateTime;

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

pub fn serial(byte: u8) -> Serial {
    Serial::from_be_bytes(&[byte; 16]).unwrap()
}

pub fn trust_domain() -> TrustDomain {
    TrustDomain::parse(TRUST_DOMAIN).unwrap()
}

pub fn begin(rig: &CaRig) -> CeremonyBegun {
    rig.service
        .create_begin(TRUST_DOMAIN, CA_PASSPHRASE)
        .unwrap()
}

/// What the CLI does between `begin` and `confirm`: the root CA and the intermediate it signs.
pub struct SignedCa {
    pub root_key: RootCaKey,
    pub root_der: Vec<u8>,
    pub intermediate_der: Vec<u8>,
    pub root: String,
    pub intermediate: String,
}

/// The CLI side with a root CA issued at `root_issued_at` and the intermediate signed now.
pub fn sign_with_root_issued_at(
    rig: &CaRig,
    begun: &CeremonyBegun,
    root_issued_at: OffsetDateTime,
) -> SignedCa {
    let root_key = RootCaKey::generate().unwrap();
    let root_der = root_key
        .root_certificate(&trust_domain(), &serial(0x9f), root_issued_at)
        .unwrap();
    let intermediate_der = root_key
        .sign_intermediate(
            &root_der,
            &begun.intermediate_csr,
            &serial(0x9e),
            rig.clock.now(),
        )
        .unwrap();
    SignedCa {
        root: pem::encode("CERTIFICATE", &root_der),
        intermediate: pem::encode("CERTIFICATE", &intermediate_der),
        root_key,
        root_der,
        intermediate_der,
    }
}

pub fn sign(rig: &CaRig, begun: &CeremonyBegun) -> SignedCa {
    sign_with_root_issued_at(rig, begun, rig.clock.now())
}

/// The whole ceremony on a rig.
pub fn create_ca_on(rig: &CaRig) -> SignedCa {
    let begun = begin(rig);
    let signed = sign(rig, &begun);
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    signed
}

/// A CA created with the whole ceremony.
pub fn created_ca() -> (CaRig, SignedCa) {
    let rig = ca_rig();
    let signed = create_ca_on(&rig);
    (rig, signed)
}

/// A rig whose service of identities has other settings.
pub fn ca_rig_with(settings: IdentitySettings) -> CaRig {
    CaRig::configured(
        crypto(),
        Arc::new(RcgenPki::new()),
        Arc::new(JwtCallTokenVerifier::new()),
        CaSettings::default(),
        settings,
    )
}

use hecate_application::identity::service::Enrolled;
use hecate_application::ports::{CertificateProfile, IdentityStore};
use hecate_domain::events::IdentityEvent;
use hecate_domain::identity::names::{IdentityName, SpiffeId};

/// The DER of an Ed25519 SubjectPublicKeyInfo before its 32 bytes (RFC 8410).
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// A call key of a component, in PEM.
pub fn call_key_pem(byte: u8) -> String {
    pem::encode(
        "PUBLIC KEY",
        &[ED25519_SPKI_PREFIX.as_slice(), &[byte; 32]].concat(),
    )
}

pub fn spiffe(identity: &str) -> SpiffeId {
    SpiffeId::new(trust_domain(), IdentityName::parse(identity).unwrap())
}

/// The request of a component for a new P-256 key: it asks for `asks_for`, or for no name.
pub fn request_for(rig: &CaRig, asks_for: Option<&str>) -> String {
    let key = rig.pki.generate_key().unwrap();
    let profile = match asks_for {
        Some(identity) => CertificateProfile::Leaf {
            spiffe_id: spiffe(identity),
        },
        None => CertificateProfile::Root {
            trust_domain: trust_domain(),
        },
    };
    rig.pki.create_request(&key, &profile).unwrap()
}

pub fn enroll_body(token: &str, csr: &str, call_key: &str) -> serde_json::Value {
    json!({"token": token, "csr": csr, "call_key": call_key})
}

pub fn new_token(rig: &CaRig, identity: &str) -> String {
    rig.identities
        .tokens_create(identity)
        .unwrap()
        .token
        .to_string()
}

/// An enrollment of `identity` with a new token and new keys, under an Idempotency-Key.
pub fn enroll_new(rig: &CaRig, identity: &str, key: &str) -> Enrolled {
    let token = new_token(rig, identity);
    let body = enroll_body(&token, &request_for(rig, Some(identity)), &call_key_pem(1));
    rig.identities.enroll(Some(key), &body).unwrap()
}

/// Appends events to the stream of an identity, as the operations of later stories do.
pub fn append_identity_events(rig: &CaRig, identity: &str, events: &[IdentityEvent]) {
    let stream = format!("identity/{identity}");
    let records: Vec<EventRecord> = events.iter().map(IdentityEvent::encode).collect();
    rig.store
        .transaction(&mut |writes| {
            let version = writes.load(&stream)?.len() as u64;
            writes.append(&stream, &records, version)?;
            Ok(())
        })
        .unwrap();
}

use hecate_application::errors::ServiceError as Refusal;
use hecate_application::identity::service::Credentials;
use hecate_infrastructure::testing::{TempDir, TestComponent, short_dir};

/// A component of the tests that talks to the services directly, without the API.
pub struct Component {
    pub inner: TestComponent,
    _dir: TempDir,
}

impl Component {
    /// A component enrolled as `identity` with a new token.
    pub fn enrolled(rig: &CaRig, identity: &str) -> Self {
        let dir = short_dir();
        let inner = TestComponent::open(dir.path(), rig.clock.clone()).unwrap();
        let token = new_token(rig, identity);
        let (body, key) = inner.enrollment(&token).unwrap();
        let enrolled = rig.identities.enroll(Some(&key), &body).unwrap();
        inner
            .accept_enrollment(&enrolled.certificate, &enrolled.chain)
            .unwrap();
        Self { inner, _dir: dir }
    }

    /// The credentials of a call: a certificate it keeps and a new short call token.
    pub fn credentials(&self, from: Option<&Serial>) -> Credentials {
        Credentials {
            certificates: self
                .inner
                .certificate_chain_of(from)
                .unwrap()
                .into_iter()
                .take(2)
                .collect(),
            bearer: Some(self.inner.call_token(from).unwrap()),
        }
    }

    /// Its pending renewal, sent with a new short call token.
    pub fn renew(&self, rig: &CaRig) -> Result<Enrolled, Refusal> {
        let (body, key) = self.inner.renewal().unwrap();
        rig.identities
            .renew(&self.credentials(None), Some(&key), &body)
    }

    /// A renewal that is issued, whose certificate becomes the current one.
    pub fn renewed(&self, rig: &CaRig) -> Enrolled {
        let renewed = self.renew(rig).unwrap();
        self.inner
            .accept_renewal(&renewed.certificate, &renewed.chain)
            .unwrap();
        renewed
    }

    pub fn serial(&self) -> Serial {
        self.inner.serial().unwrap()
    }
}
