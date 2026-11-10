//! The CA of Cerbero (FR-001 to FR-004, FR-012): the creation ceremony with the CLI, which keeps
//! the root CA key, the key of each intermediate wrapped by the root of encryption and decrypted
//! only for the operation that signs (research R14), and Hécate's own certificate, which it
//! renews by itself at two thirds of its life while unsealed.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use data_encoding::BASE64URL_NOPAD;
use hecate_domain::ca::authority::{CertificateAuthority, IntermediateState};
use hecate_domain::ca::{INTERMEDIATE_LIFETIME, NOT_BEFORE_MARGIN, ROOT_LIFETIME};
use hecate_domain::events::{
    CaEvent, CertificateEvent, CertificateOrigin, EventRecord, InvalidEvents, IssuedCertificate,
    RetireReason,
};
use hecate_domain::identity::names::{
    IdentityName, RootFingerprint, Serial, SpiffeId, TrustDomain,
};
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::passphrase::{MIN_LENGTH, Passphrase};
use hecate_domain::sensitive::SecretBytes;
use serde_json::{Map, Value};
use time::{Duration, OffsetDateTime};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::ca::settings::CaSettings;
use crate::errors::ServiceError;
use crate::ports::{
    AccessLog, CaMaterial, CertificateInfo, CertificateProfile, Clock, CryptoProvider,
    IdentityStore, Issuance, Issuer, OwnTlsKey, PkiProvider, RevokedCertificate,
};
use crate::sealing::service::SealingService;
use crate::secrets::service::ADMIN;

pub const HECATE: &str = "hecate";
pub const CA_STREAM: &str = "ca";
const CERTIFICATE: &str = "CERTIFICATE";
const CEREMONY_ID_SIZE: usize = 16;
const SERIAL_SIZE: usize = 16;
const ABORT_REASONS: [&str; 2] = ["cancelled", "backup_check_failed"];
const RENEWAL_ABORT_REASONS: [&str; 3] = ["cancelled", "wrong_passphrase", "wrong_root_key"];
/// The warning of `status` starts when the intermediate has fewer days left (FR-025).
const DAYS_LEFT_WARNING: Duration = Duration::days(30);
const UNKNOWN_CEREMONY: &str = "unknown or expired ceremony: start it again";
const CERTIFICATE_PREFIX: &str = "certificate/";
/// How many times an operation reads the CA again after losing a race for its stream.
const MAX_RACES: usize = 3;

/// A ceremony begun: what the CLI needs to sign the intermediate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeremonyBegun {
    pub ceremony_id: String,
    /// The request of the new intermediate, in PEM.
    pub intermediate_csr: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaCreated {
    pub trust_domain: TrustDomain,
    pub root_fingerprint: RootFingerprint,
    pub intermediate_not_after: OffsetDateTime,
}

/// Hécate's current certificate, as `status` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnCertificate {
    pub serial: Serial,
    pub not_after: OffsetDateTime,
}

/// The `ca` block of `status`, which also answers while sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaSummary {
    pub trust_domain: TrustDomain,
    pub root_fingerprint: RootFingerprint,
    pub hecate_certificate: Option<OwnCertificate>,
    /// The days left to the intermediate that issues, only when fewer than 30; 0 once expired.
    pub intermediate_days_left: Option<u64>,
}

/// A renewal of the intermediate begun: what the CLI needs to sign it with the root CA key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntermediateBegun {
    pub ceremony_id: String,
    /// The request of the new intermediate, in PEM.
    pub intermediate_csr: String,
    /// The root CA of the current generation, in PEM, to check the copy of its key against.
    pub root_certificate: String,
    pub expires_at: OffsetDateTime,
}

/// The CA that replaced the previous one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacedCa {
    pub trust_domain: TrustDomain,
    pub root_fingerprint: RootFingerprint,
    pub generation: u64,
}

/// The intermediate that issues from now on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewedIntermediate {
    pub intermediate_version: u64,
    pub intermediate_not_after: OffsetDateTime,
}

/// What the API presents while Hécate's certificate is valid: the chain up to the root CA, the
/// key and the current root CA for the client certificates. It is erased when dropped, and it can
/// be neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::ca::service::TlsMaterial>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::ca::service::TlsMaterial>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::ca::service::TlsMaterial>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct TlsMaterial {
    /// Hécate's certificate, its intermediate and its root CA, in DER.
    pub chain: Vec<Vec<u8>>,
    /// The PKCS#8 of the key.
    pub key: SecretBytes,
    pub root: Vec<u8>,
    #[zeroize(skip)]
    pub not_after: OffsetDateTime,
}

/// A pending ceremony: the key of the new intermediate lives here until the ceremony is
/// confirmed, aborted or expires, or Hécate is sealed or restarted (FR-004).
struct Ceremony {
    id: String,
    kind: CeremonyKind,
    trust_domain: TrustDomain,
    key: SecretBytes,
    expires_at: OffsetDateTime,
    epoch: u64,
}

/// What a ceremony does: its id only works with the operations of its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CeremonyKind {
    Create,
    /// The renewal of the intermediate, with the version that it signs.
    Renewal {
        generation: u64,
        version: u64,
    },
    /// The replacement of the CA, with the generation that it starts.
    Replace {
        generation: u64,
    },
}

impl CeremonyKind {
    fn is_renewal(self) -> bool {
        matches!(self, Self::Renewal { .. })
    }

    fn is_replace(self) -> bool {
        matches!(self, Self::Replace { .. })
    }
}

/// A certificate that the admin sent, with what Hécate reads of it.
struct Received {
    der: Vec<u8>,
    info: CertificateInfo,
}

/// A certificate signed by an intermediate, with what its issuance event keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedLeaf {
    pub serial: Serial,
    /// The DER of the certificate.
    pub certificate: Vec<u8>,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    pub generation: u64,
    pub intermediate_version: u64,
}

/// Hécate's current certificate with its key and the length of its stream.
struct CurrentOwn {
    key: OwnTlsKey,
    issued: IssuedCertificate,
    stream_version: u64,
}

fn invalid(message: impl Into<String>) -> ServiceError {
    HecateError::invalid(message).into()
}

fn conflict(message: &str) -> ServiceError {
    HecateError::state_conflict(None, Some(message.to_string()), None).into()
}

fn corrupt(_: InvalidEvents) -> ServiceError {
    ServiceError::internal("InvalidEvents")
}

fn result_for(error: &ServiceError) -> AccessResult {
    match error {
        ServiceError::Hecate(HecateError::InvalidInput(_)) => AccessResult::InvalidInput,
        ServiceError::Hecate(HecateError::StateConflict { .. }) => AccessResult::RejectedConflict,
        ServiceError::Hecate(HecateError::RateLimited { .. }) => AccessResult::RejectedRateLimited,
        ServiceError::Hecate(HecateError::NotFound(_)) => AccessResult::NotFound,
        _ => AccessResult::Failure,
    }
}

/// Compares two identifiers without leaking where they differ.
fn same_token(expected: &str, given: &str) -> bool {
    let (expected, given) = (expected.as_bytes(), given.as_bytes());
    expected.len() == given.len()
        && expected
            .iter()
            .zip(given)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Certificates keep whole seconds; the events keep the same instants.
fn whole_seconds(instant: OffsetDateTime) -> OffsetDateTime {
    instant.replace_nanosecond(0).unwrap_or(instant)
}

/// The days left until `not_after` when they are fewer than 30, a part of a day counting as a
/// day; 0 once it is over (FR-025).
fn days_left(not_after: OffsetDateTime, now: OffsetDateTime) -> Option<u64> {
    let left = not_after - now;
    if left <= Duration::ZERO {
        return Some(0);
    }
    if left >= DAYS_LEFT_WARNING {
        return None;
    }
    let whole = left.whole_days();
    let days = if left > Duration::days(whole) {
        whole + 1
    } else {
        whole
    };
    u64::try_from(days).ok()
}

/// The associated data of the key of an intermediate: JSON with sorted keys, as in feature 002.
fn intermediate_aad(generation: u64, version: u64, root: &RootFingerprint) -> Vec<u8> {
    let mut fields = Map::new();
    fields.insert("generation".into(), Value::from(generation));
    fields.insert("purpose".into(), Value::from("ca_intermediate"));
    fields.insert("root".into(), Value::from(root.to_string()));
    fields.insert("version".into(), Value::from(version));
    Value::Object(fields).to_string().into_bytes()
}

fn own_stream(serial: &Serial) -> String {
    format!("certificate/{HECATE}/{serial}")
}

/// A ceremony that is past its expiry or belongs to another unseal is gone.
fn drop_lost(slot: &mut Option<Ceremony>, epoch: Option<u64>, now: OffsetDateTime) {
    if slot
        .as_ref()
        .is_some_and(|ceremony| Some(ceremony.epoch) != epoch || now > ceremony.expires_at)
    {
        *slot = None;
    }
}

fn decode_ca(records: &[EventRecord]) -> Result<Option<CertificateAuthority>, ServiceError> {
    if records.is_empty() {
        return Ok(None);
    }
    let events = records
        .iter()
        .map(CaEvent::decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(corrupt)?;
    CertificateAuthority::from_events(&events)
        .map(Some)
        .map_err(corrupt)
}

/// The serial of a new certificate: 128 random bits, positive and not zero.
pub fn random_serial(crypto: &dyn CryptoProvider) -> Serial {
    loop {
        if let Ok(serial) = Serial::from_be_bytes(&crypto.random_bytes(SERIAL_SIZE)) {
            return serial;
        }
    }
}

pub struct CaService {
    sealing: Arc<SealingService>,
    store: Arc<dyn IdentityStore>,
    pki: Arc<dyn PkiProvider>,
    crypto: Arc<dyn CryptoProvider>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
    settings: CaSettings,
    ceremony: Mutex<Option<Ceremony>>,
    renewal: Mutex<()>,
    /// The `cRLNumber` of the last CRLs, so the next ones get a higher one.
    crl_number: Mutex<u64>,
}

impl CaService {
    pub fn new(
        sealing: Arc<SealingService>,
        store: Arc<dyn IdentityStore>,
        pki: Arc<dyn PkiProvider>,
        crypto: Arc<dyn CryptoProvider>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
        settings: CaSettings,
    ) -> Self {
        Self {
            sealing,
            store,
            pki,
            crypto,
            access_log,
            clock,
            settings,
            ceremony: Mutex::new(None),
            renewal: Mutex::new(()),
            crl_number: Mutex::new(0),
        }
    }

    fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        &self,
        operation: Operation,
        method: Method,
        subject: Option<&str>,
        result: AccessResult,
        identity: Option<&str>,
        certificate: Option<&str>,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.access_log.append(&AccessLogEntry {
            at: self.now(),
            operation,
            method,
            result,
            detail: detail.to_string(),
            subject: subject.map(str::to_string),
            path: None,
            version: None,
            identity: identity.map(str::to_string),
            certificate: certificate.map(str::to_string),
        })
    }

    /// Leaves a row for an operation of the admin: its success or why it was refused.
    fn audited<T>(
        &self,
        operation: Operation,
        detail: &str,
        work: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let outcome = work();
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(
            operation,
            Method::Local,
            Some(ADMIN),
            result,
            None,
            None,
            detail,
        )?;
        outcome
    }

    /// Leaves a row for a recognized operation whose arguments were malformed.
    pub fn record_invalid_request(
        &self,
        operation: Operation,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.log(
            operation,
            Method::Local,
            Some(ADMIN),
            AccessResult::InvalidInput,
            None,
            None,
            detail,
        )
    }

    fn token(&self) -> String {
        BASE64URL_NOPAD.encode(&self.crypto.random_bytes(CEREMONY_ID_SIZE))
    }

    fn new_serial(&self) -> Serial {
        random_serial(self.crypto.as_ref())
    }

    fn ca(&self) -> Result<Option<CertificateAuthority>, ServiceError> {
        decode_ca(&self.store.load(CA_STREAM)?)
    }

    fn material(&self, generation: u64, version: u64) -> Result<CaMaterial, ServiceError> {
        self.store
            .load_ca_material()?
            .into_iter()
            .find(|material| {
                material.generation == generation && material.intermediate_version == version
            })
            .ok_or_else(|| ServiceError::internal("MissingCaMaterial"))
    }

    /// The key of an intermediate, decrypted with the root of encryption for one operation.
    fn intermediate_key(&self, material: &CaMaterial) -> Result<SecretBytes, ServiceError> {
        let root = self.sealing.root()?;
        let fingerprint = self.pki.fingerprint(&material.root_certificate);
        self.crypto
            .aead_decrypt(
                &root,
                &material.key_nonce,
                &material.key_ciphertext,
                &intermediate_aad(
                    material.generation,
                    material.intermediate_version,
                    &fingerprint,
                ),
            )
            .map_err(|_| ServiceError::internal("UndecryptableIntermediateKey"))
    }

    fn current_own(&self) -> Result<Option<CurrentOwn>, ServiceError> {
        let Some(key) = self.store.load_own_tls_key()? else {
            return Ok(None);
        };
        let records = self.store.load(&own_stream(&key.serial))?;
        let first = records
            .first()
            .ok_or_else(|| ServiceError::internal("MissingOwnCertificate"))?;
        let CertificateEvent::CertificateIssued(issued) =
            CertificateEvent::decode(first).map_err(corrupt)?
        else {
            return Err(ServiceError::internal("InvalidEvents"));
        };
        Ok(Some(CurrentOwn {
            key,
            issued: *issued,
            stream_version: records.len() as u64,
        }))
    }

    /// A certificate for `spiffe_id` signed by an intermediate: from a minute ago and for the
    /// configured lifetime without outliving the intermediate (FR-009).
    #[allow(clippy::too_many_arguments)]
    fn sign_leaf(
        &self,
        spiffe_id: SpiffeId,
        public_key: &[u8],
        intermediate: &[u8],
        intermediate_key: &SecretBytes,
        intermediate_not_after: OffsetDateTime,
        (generation, intermediate_version): (u64, u64),
        now: OffsetDateTime,
    ) -> Result<SignedLeaf, ServiceError> {
        let serial = self.new_serial();
        let not_before = whole_seconds(now - NOT_BEFORE_MARGIN);
        let not_after =
            whole_seconds((now + self.settings.certificate_lifetime).min(intermediate_not_after));
        let certificate = self.pki.issue(
            &Issuer {
                certificate: intermediate,
                key: intermediate_key,
            },
            &Issuance {
                profile: &CertificateProfile::Leaf { spiffe_id },
                public_key,
                serial: &serial,
                not_before,
                not_after,
            },
        )?;
        Ok(SignedLeaf {
            serial,
            certificate,
            not_before,
            not_after,
            generation,
            intermediate_version,
        })
    }

    /// A new certificate of Hécate with a new key.
    #[allow(clippy::too_many_arguments)]
    fn issue_own(
        &self,
        trust_domain: &TrustDomain,
        version: (u64, u64),
        intermediate: &[u8],
        intermediate_key: &SecretBytes,
        intermediate_not_after: OffsetDateTime,
        renewed_from: Option<Serial>,
        now: OffsetDateTime,
    ) -> Result<(OwnTlsKey, IssuedCertificate), ServiceError> {
        let key = self.pki.generate_key()?;
        let leaf = self.sign_leaf(
            SpiffeId::new(trust_domain.clone(), IdentityName::hecate()),
            &self.pki.public_key(&key)?,
            intermediate,
            intermediate_key,
            intermediate_not_after,
            version,
            now,
        )?;
        let issued = IssuedCertificate {
            at: now,
            identity: IdentityName::hecate(),
            serial: leaf.serial.clone(),
            generation: leaf.generation,
            intermediate_version: leaf.intermediate_version,
            epoch: 0,
            not_before: leaf.not_before,
            not_after: leaf.not_after,
            call_key: None,
            origin: CertificateOrigin::OwnCertificate,
            renewed_from,
            request_sha256: None,
            idempotency_key: None,
            certificate: leaf.certificate,
        };
        Ok((
            OwnTlsKey {
                serial: leaf.serial,
                private_key: key,
            },
            issued,
        ))
    }

    /// The CA now, if there is one.
    pub fn authority(&self) -> Result<Option<CertificateAuthority>, ServiceError> {
        self.ca()
    }

    /// A certificate of a component signed by the active intermediate, whose key is decrypted
    /// for this signature only (research R14).
    pub fn issue_component(
        &self,
        identity: &IdentityName,
        public_key: &[u8],
        now: OffsetDateTime,
    ) -> Result<SignedLeaf, ServiceError> {
        let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
        let active = ca
            .active()
            .filter(|active| now < active.not_after)
            .ok_or_else(|| conflict("the current intermediate expired"))?;
        let material = self.material(active.generation, active.version)?;
        let key = self.intermediate_key(&material)?;
        self.sign_leaf(
            SpiffeId::new(ca.trust_domain.clone(), identity.clone()),
            public_key,
            &material.intermediate_certificate,
            &key,
            active.not_after,
            (active.generation, active.version),
            now,
        )
    }

    /// The root CA of the current generation, in DER.
    pub fn root_certificate(&self) -> Result<Option<Vec<u8>>, ServiceError> {
        let Some(ca) = self.ca()? else {
            return Ok(None);
        };
        Ok(self
            .store
            .load_ca_material()?
            .into_iter()
            .find(|material| material.generation == ca.generation)
            .map(|material| material.root_certificate))
    }

    /// The intermediate and the root CA of a version, in DER: the chain of its certificates.
    pub fn chain(&self, generation: u64, version: u64) -> Result<Vec<Vec<u8>>, ServiceError> {
        let material = self.material(generation, version)?;
        Ok(vec![
            material.intermediate_certificate,
            material.root_certificate,
        ])
    }

    fn received(&self, text: &str) -> Result<Received, ServiceError> {
        let der = pem::single(text, CERTIFICATE)?;
        let info = self
            .pki
            .read_certificate(&der)
            .map_err(|_| invalid("the certificate is not a valid X.509 certificate"))?;
        Ok(Received { der, info })
    }

    fn check_root(
        &self,
        root: &Received,
        trust_domain: &TrustDomain,
        now: OffsetDateTime,
    ) -> Result<(), ServiceError> {
        let profile = CertificateProfile::Root {
            trust_domain: trust_domain.clone(),
        };
        let info = &root.info;
        let valid = self
            .pki
            .follows_profile(&root.der, &profile)
            .unwrap_or(false)
            && self.pki.is_signed_by(&root.der, &root.der).unwrap_or(false)
            && info.not_before <= now
            && now < info.not_after
            && info.not_after - info.not_before <= ROOT_LIFETIME;
        if !valid {
            return Err(invalid(
                "the root CA is not a self-signed root CA of the trust domain valid now",
            ));
        }
        Ok(())
    }

    fn check_intermediate(
        &self,
        intermediate: &Received,
        root: &Received,
        key: &SecretBytes,
        profile: &CertificateProfile,
        now: OffsetDateTime,
    ) -> Result<(), ServiceError> {
        let info = &intermediate.info;
        let valid = self
            .pki
            .follows_profile(&intermediate.der, profile)
            .unwrap_or(false)
            && self
                .pki
                .is_signed_by(&intermediate.der, &root.der)
                .unwrap_or(false)
            && info.public_key == self.pki.public_key(key)?
            && info.not_before <= now
            && now < info.not_after
            && info.not_after <= root.info.not_after
            && info.not_after - info.not_before <= INTERMEDIATE_LIFETIME;
        if !valid {
            return Err(invalid(
                "the intermediate is not the one of the ceremony, signed by the root CA and valid now",
            ));
        }
        Ok(())
    }

    /// Starts the creation: the CA passphrase only reaches Hécate to check it and is dropped at
    /// the end (FR-002); the key of the intermediate stays in the ceremony.
    pub fn create_begin(
        &self,
        trust_domain: &str,
        ca_passphrase: &str,
    ) -> Result<CeremonyBegun, ServiceError> {
        self.audited(Operation::CaCreate, "begin", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            if self.ca()?.is_some() {
                return Err(conflict("there is a CA already"));
            }
            if slot.is_some() {
                return Err(conflict("another ceremony is in progress"));
            }
            let trust_domain = TrustDomain::parse(trust_domain)?;
            let passphrase = Passphrase::new(ca_passphrase).map_err(|_| {
                invalid(format!(
                    "the CA passphrase must have at least {MIN_LENGTH} characters"
                ))
            })?;
            if self.sealing.is_unseal_passphrase(passphrase.expose())? {
                return Err(invalid(
                    "the CA passphrase must not be the unseal passphrase",
                ));
            }
            drop(passphrase);
            let key = self.pki.generate_key()?;
            let request = self.pki.create_request(
                &key,
                &CertificateProfile::Intermediate {
                    trust_domain: trust_domain.clone(),
                    generation: 1,
                    version: 1,
                },
            )?;
            let ceremony = Ceremony {
                id: self.token(),
                kind: CeremonyKind::Create,
                trust_domain,
                key,
                expires_at: now + self.settings.ceremony_ttl,
                epoch,
            };
            let begun = CeremonyBegun {
                ceremony_id: ceremony.id.clone(),
                intermediate_csr: request,
                expires_at: ceremony.expires_at,
            };
            *slot = Some(ceremony);
            Ok(begun)
        })
    }

    /// Checks the certificates and stores, in one transaction, the CA, its material with the key
    /// of the intermediate wrapped by the root, and Hécate's first certificate with its key.
    pub fn create_confirm(
        &self,
        ceremony_id: &str,
        root_certificate: &str,
        intermediate_certificate: &str,
    ) -> Result<CaCreated, ServiceError> {
        self.audited(Operation::CaCreate, "confirm", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            let ceremony = slot
                .as_ref()
                .filter(|ceremony| {
                    ceremony.kind == CeremonyKind::Create && same_token(&ceremony.id, ceremony_id)
                })
                .ok_or_else(|| conflict(UNKNOWN_CEREMONY))?;
            if self.ca()?.is_some() {
                return Err(conflict("there is a CA already"));
            }
            let trust_domain = ceremony.trust_domain.clone();
            let root = self.received(root_certificate)?;
            let intermediate = self.received(intermediate_certificate)?;
            self.check_root(&root, &trust_domain, now)?;
            let profile = CertificateProfile::Intermediate {
                trust_domain: trust_domain.clone(),
                generation: 1,
                version: 1,
            };
            self.check_intermediate(&intermediate, &root, &ceremony.key, &profile, now)?;
            let fingerprint = self.pki.fingerprint(&root.der);
            let (key_nonce, key_ciphertext) = self.crypto.aead_encrypt(
                &self.sealing.root()?,
                ceremony.key.expose(),
                &intermediate_aad(1, 1, &fingerprint),
            )?;
            let (own_key, issued) = self.issue_own(
                &trust_domain,
                (1, 1),
                &intermediate.der,
                &ceremony.key,
                intermediate.info.not_after,
                None,
                now,
            )?;
            let ca_events: Vec<EventRecord> = CertificateAuthority::create(
                trust_domain.clone(),
                fingerprint,
                root.info.not_after,
                intermediate.info.not_after,
                now,
            )
            .iter()
            .map(CaEvent::encode)
            .collect();
            let own_events =
                [CertificateEvent::CertificateIssued(Box::new(issued.clone())).encode()];
            let material = CaMaterial {
                generation: 1,
                intermediate_version: 1,
                root_certificate: root.der.clone(),
                intermediate_certificate: intermediate.der.clone(),
                key_nonce,
                key_ciphertext,
            };
            self.store.transaction(&mut |writes| {
                writes
                    .append(CA_STREAM, &ca_events, 0)
                    .map_err(|error| match error {
                        ServiceError::Concurrency(_) => conflict("there is a CA already"),
                        other => other,
                    })?;
                writes.append(&own_stream(&issued.serial), &own_events, 0)?;
                writes.save_ca_material(&material)?;
                writes.save_own_tls_key(&own_key)?;
                Ok(())
            })?;
            *slot = None;
            Ok(CaCreated {
                trust_domain,
                root_fingerprint: fingerprint,
                intermediate_not_after: intermediate.info.not_after,
            })
        })
    }

    /// Discards the ceremony; its reason stays in the access log, also when the CLI is the one
    /// that found the failure.
    pub fn create_abort(&self, ceremony_id: &str, reason: &str) -> Result<(), ServiceError> {
        self.abort(
            Operation::CaCreate,
            &ABORT_REASONS,
            |kind| kind == CeremonyKind::Create,
            ceremony_id,
            reason,
        )
    }

    /// Discards a pending ceremony of the kind that `is_kind` accepts, with one of its reasons.
    fn abort(
        &self,
        operation: Operation,
        reasons: &[&str],
        is_kind: impl Fn(CeremonyKind) -> bool,
        ceremony_id: &str,
        reason: &str,
    ) -> Result<(), ServiceError> {
        let valid_reason = reasons.contains(&reason);
        let detail = if valid_reason {
            format!("abort: {reason}")
        } else {
            "abort".to_string()
        };
        self.audited(operation, &detail, || {
            let epoch = self.sealing.unsealed_epoch()?;
            if !valid_reason {
                return Err(invalid(format!(
                    "reason must be one of {}",
                    reasons.join(", ")
                )));
            }
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), self.now());
            if !slot.as_ref().is_some_and(|ceremony| {
                is_kind(ceremony.kind) && same_token(&ceremony.id, ceremony_id)
            }) {
                return Err(conflict(UNKNOWN_CEREMONY));
            }
            *slot = None;
            Ok(())
        })
    }

    /// Starts the renewal of the intermediate (FR-005, research R9): the key of the new version
    /// stays in the ceremony, and the CLI signs its request with the copy of the root CA key. It
    /// also works with the intermediate expired, which is the way out.
    pub fn intermediate_begin(&self) -> Result<IntermediateBegun, ServiceError> {
        self.audited(Operation::CaIntermediateRenew, "begin", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
            if slot.is_some() {
                return Err(conflict("another ceremony is in progress"));
            }
            let root = self
                .root_certificate()?
                .ok_or_else(|| ServiceError::internal("MissingCaMaterial"))?;
            let (generation, version) = (ca.generation, ca.next_version());
            let key = self.pki.generate_key()?;
            let request = self.pki.create_request(
                &key,
                &CertificateProfile::Intermediate {
                    trust_domain: ca.trust_domain.clone(),
                    generation,
                    version,
                },
            )?;
            let ceremony = Ceremony {
                id: self.token(),
                kind: CeremonyKind::Renewal {
                    generation,
                    version,
                },
                trust_domain: ca.trust_domain,
                key,
                expires_at: now + self.settings.ceremony_ttl,
                epoch,
            };
            let begun = IntermediateBegun {
                ceremony_id: ceremony.id.clone(),
                intermediate_csr: request,
                root_certificate: pem::encode(CERTIFICATE, &root),
                expires_at: ceremony.expires_at,
            };
            *slot = Some(ceremony);
            Ok(begun)
        })
    }

    /// Checks the new intermediate and makes it the one that issues, in one transaction with its
    /// key wrapped by the root; the previous one only verifies from now on.
    pub fn intermediate_confirm(
        &self,
        ceremony_id: &str,
        intermediate_certificate: &str,
    ) -> Result<RenewedIntermediate, ServiceError> {
        self.audited(Operation::CaIntermediateRenew, "confirm", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            let ceremony = slot
                .as_ref()
                .filter(|ceremony| {
                    ceremony.kind.is_renewal() && same_token(&ceremony.id, ceremony_id)
                })
                .ok_or_else(|| conflict(UNKNOWN_CEREMONY))?;
            let CeremonyKind::Renewal {
                generation,
                version,
            } = ceremony.kind
            else {
                return Err(conflict(UNKNOWN_CEREMONY));
            };
            let root_der = self
                .root_certificate()?
                .ok_or_else(|| ServiceError::internal("MissingCaMaterial"))?;
            let root = Received {
                info: self
                    .pki
                    .read_certificate(&root_der)
                    .map_err(|_| ServiceError::internal("InvalidCaMaterial"))?,
                der: root_der,
            };
            let intermediate = self.received(intermediate_certificate)?;
            let profile = CertificateProfile::Intermediate {
                trust_domain: ceremony.trust_domain.clone(),
                generation,
                version,
            };
            self.check_intermediate(&intermediate, &root, &ceremony.key, &profile, now)?;
            let fingerprint = self.pki.fingerprint(&root.der);
            let (key_nonce, key_ciphertext) = self.crypto.aead_encrypt(
                &self.sealing.root()?,
                ceremony.key.expose(),
                &intermediate_aad(generation, version, &fingerprint),
            )?;
            let material = CaMaterial {
                generation,
                intermediate_version: version,
                root_certificate: root.der.clone(),
                intermediate_certificate: intermediate.der.clone(),
                key_nonce,
                key_ciphertext,
            };
            let not_after = intermediate.info.not_after;
            let renewed = [CaEvent::IntermediateRenewed {
                at: now,
                generation,
                intermediate_version: version,
                intermediate_not_after: not_after,
            }
            .encode()];
            // The task of every minute may retire an intermediate meanwhile: read the CA again.
            let mut written = Err(ServiceError::internal("TooManyRaces"));
            for _ in 0..MAX_RACES {
                let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
                if ca.generation != generation || ca.next_version() != version {
                    return Err(conflict("the CA changed during the ceremony"));
                }
                written = self.store.transaction(&mut |writes| {
                    writes.append(CA_STREAM, &renewed, ca.stream_version)?;
                    writes.save_ca_material(&material)?;
                    Ok(())
                });
                if !matches!(written, Err(ServiceError::Concurrency(_))) {
                    break;
                }
            }
            written?;
            *slot = None;
            Ok(RenewedIntermediate {
                intermediate_version: version,
                intermediate_not_after: not_after,
            })
        })
    }

    /// Discards the renewal; `wrong_passphrase` and `wrong_root_key` are failures that the CLI
    /// found, and none counts for the common counter.
    pub fn intermediate_abort(&self, ceremony_id: &str, reason: &str) -> Result<(), ServiceError> {
        self.abort(
            Operation::CaIntermediateRenew,
            &RENEWAL_ABORT_REASONS,
            CeremonyKind::is_renewal,
            ceremony_id,
            reason,
        )
    }

    /// Starts the replacement of the CA (FR-006, research R9): the proof counts for the common
    /// counter, and the new CA passphrase follows the rules of FR-002 and is dropped at the end.
    /// A request refused before the proof spends no attempt.
    pub fn replace_begin(
        &self,
        proof: &Value,
        ca_passphrase: &str,
    ) -> Result<CeremonyBegun, ServiceError> {
        self.audited(Operation::CaReplace, "begin", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
            if slot.is_some() {
                return Err(conflict("another ceremony is in progress"));
            }
            let passphrase = Passphrase::new(ca_passphrase).map_err(|_| {
                invalid(format!(
                    "the CA passphrase must have at least {MIN_LENGTH} characters"
                ))
            })?;
            self.sealing.verify_proof(proof)?;
            // A proof with the passphrase is the unseal passphrase: no second pass of Argon2id.
            let unseal = match proof.get("passphrase").and_then(Value::as_str) {
                Some(proven) => same_token(proven, passphrase.expose()),
                None => self.sealing.is_unseal_passphrase(passphrase.expose())?,
            };
            if unseal {
                return Err(invalid(
                    "the CA passphrase must not be the unseal passphrase",
                ));
            }
            drop(passphrase);
            let generation = ca.generation + 1;
            let key = self.pki.generate_key()?;
            let request = self.pki.create_request(
                &key,
                &CertificateProfile::Intermediate {
                    trust_domain: ca.trust_domain.clone(),
                    generation,
                    version: 1,
                },
            )?;
            let ceremony = Ceremony {
                id: self.token(),
                kind: CeremonyKind::Replace { generation },
                trust_domain: ca.trust_domain,
                key,
                expires_at: now + self.settings.ceremony_ttl,
                epoch,
            };
            let begun = CeremonyBegun {
                ceremony_id: ceremony.id.clone(),
                intermediate_csr: request,
                expires_at: ceremony.expires_at,
            };
            *slot = Some(ceremony);
            Ok(begun)
        })
    }

    /// Checks the new root CA and its intermediate and changes the generation in one
    /// transaction: every previous intermediate retires and its key is destroyed, and Hécate gets
    /// a certificate of the new CA. Tokens and certificates of the previous generation are no
    /// longer valid, without an event for each one.
    pub fn replace_confirm(
        &self,
        ceremony_id: &str,
        root_certificate: &str,
        intermediate_certificate: &str,
    ) -> Result<ReplacedCa, ServiceError> {
        self.audited(Operation::CaReplace, "confirm", || {
            let epoch = self.sealing.unsealed_epoch()?;
            let now = self.now();
            let mut slot = lock(&self.ceremony);
            drop_lost(&mut slot, Some(epoch), now);
            let ceremony = slot
                .as_ref()
                .filter(|ceremony| {
                    ceremony.kind.is_replace() && same_token(&ceremony.id, ceremony_id)
                })
                .ok_or_else(|| conflict(UNKNOWN_CEREMONY))?;
            let CeremonyKind::Replace { generation } = ceremony.kind else {
                return Err(conflict(UNKNOWN_CEREMONY));
            };
            let trust_domain = ceremony.trust_domain.clone();
            let root = self.received(root_certificate)?;
            let intermediate = self.received(intermediate_certificate)?;
            self.check_root(&root, &trust_domain, now)?;
            let fingerprint = self.pki.fingerprint(&root.der);
            let current = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
            if fingerprint == current.root_fingerprint {
                return Err(invalid("the new root CA must be another one"));
            }
            let profile = CertificateProfile::Intermediate {
                trust_domain: trust_domain.clone(),
                generation,
                version: 1,
            };
            self.check_intermediate(&intermediate, &root, &ceremony.key, &profile, now)?;
            let (key_nonce, key_ciphertext) = self.crypto.aead_encrypt(
                &self.sealing.root()?,
                ceremony.key.expose(),
                &intermediate_aad(generation, 1, &fingerprint),
            )?;
            // Hécate's certificate changes only here, not in a renewal that runs meanwhile.
            let _renewing = lock(&self.renewal);
            let (own_key, issued) = self.issue_own(
                &trust_domain,
                (generation, 1),
                &intermediate.der,
                &ceremony.key,
                intermediate.info.not_after,
                None,
                now,
            )?;
            let own_events =
                [CertificateEvent::CertificateIssued(Box::new(issued.clone())).encode()];
            let material = CaMaterial {
                generation,
                intermediate_version: 1,
                root_certificate: root.der.clone(),
                intermediate_certificate: intermediate.der.clone(),
                key_nonce,
                key_ciphertext,
            };
            // The task of every minute may retire an intermediate meanwhile: read the CA again.
            let mut written = Err(ServiceError::internal("TooManyRaces"));
            for _ in 0..MAX_RACES {
                let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
                if ca.generation + 1 != generation {
                    return Err(conflict("the CA changed during the ceremony"));
                }
                let retired: Vec<(u64, u64)> = ca
                    .intermediates
                    .values()
                    .filter(|intermediate| intermediate.state != IntermediateState::Retired)
                    .map(|intermediate| (intermediate.generation, intermediate.version))
                    .collect();
                let ca_events: Vec<EventRecord> = ca
                    .replace(
                        fingerprint,
                        root.info.not_after,
                        intermediate.info.not_after,
                        now,
                    )
                    .iter()
                    .map(CaEvent::encode)
                    .collect();
                written = self.store.transaction(&mut |writes| {
                    writes.append(CA_STREAM, &ca_events, ca.stream_version)?;
                    for (generation, version) in &retired {
                        writes.delete_ca_material(*generation, *version)?;
                    }
                    writes.save_ca_material(&material)?;
                    writes.append(&own_stream(&issued.serial), &own_events, 0)?;
                    writes.save_own_tls_key(&own_key)?;
                    Ok(())
                });
                if !matches!(written, Err(ServiceError::Concurrency(_))) {
                    break;
                }
            }
            written?;
            *slot = None;
            Ok(ReplacedCa {
                trust_domain,
                root_fingerprint: fingerprint,
                generation,
            })
        })
    }

    /// Discards the replacement; `backup_check_failed` is a copy that the CLI could not open.
    pub fn replace_abort(&self, ceremony_id: &str, reason: &str) -> Result<(), ServiceError> {
        self.abort(
            Operation::CaReplace,
            &ABORT_REASONS,
            CeremonyKind::is_replace,
            ceremony_id,
            reason,
        )
    }

    /// The intermediates of the CA that only verify, by generation and version.
    fn verify_only(ca: &CertificateAuthority) -> Vec<(u64, u64)> {
        ca.intermediates
            .values()
            .filter(|intermediate| intermediate.state == IntermediateState::VerifyOnly)
            .map(|intermediate| (intermediate.generation, intermediate.version))
            .collect()
    }

    /// The end of the last certificate that each intermediate signed, Hécate's included.
    fn last_certificate_ends(&self) -> Result<BTreeMap<(u64, u64), OffsetDateTime>, ServiceError> {
        let mut last_end: BTreeMap<(u64, u64), OffsetDateTime> = BTreeMap::new();
        for stream in self.store.streams(CERTIFICATE_PREFIX)? {
            let records = self.store.load(&stream)?;
            let Some(first) = records.first() else {
                continue;
            };
            let CertificateEvent::CertificateIssued(issued) =
                CertificateEvent::decode(first).map_err(corrupt)?
            else {
                return Err(ServiceError::internal("InvalidEvents"));
            };
            let end = last_end
                .entry((issued.generation, issued.intermediate_version))
                .or_insert(issued.not_after);
            *end = (*end).max(issued.not_after);
        }
        Ok(last_end)
    }

    /// The time left until the key of a previous intermediate goes: until the last certificate it
    /// signed expires, and zero if it already did (FR-005). `None` without one that only verifies.
    pub fn next_retirement(&self) -> Result<Option<Duration>, ServiceError> {
        let Some(ca) = self.ca()? else {
            return Ok(None);
        };
        let verify_only = Self::verify_only(&ca);
        if verify_only.is_empty() {
            return Ok(None);
        }
        let last_end = self.last_certificate_ends()?;
        let now = self.now();
        Ok(verify_only
            .iter()
            .map(|version| {
                last_end
                    .get(version)
                    .map_or(Duration::ZERO, |end| (*end - now).max(Duration::ZERO))
            })
            .min())
    }

    /// Destroys the key of every intermediate that only verifies once the last certificate that
    /// it signed has expired (FR-005, SC-018), in one transaction with its `IntermediateRetired`.
    /// The task of every minute calls it, and also at the instant that [`Self::next_retirement`]
    /// gives, even while sealed, since it decrypts nothing. Returns the generation and version of
    /// each one retired.
    pub fn retire_intermediates(&self) -> Result<Vec<(u64, u64)>, ServiceError> {
        let Some(ca) = self.ca()? else {
            return Ok(Vec::new());
        };
        let verify_only = Self::verify_only(&ca);
        if verify_only.is_empty() {
            return Ok(Vec::new());
        }
        let last_end = self.last_certificate_ends()?;
        let now = self.now();
        let retired: Vec<(u64, u64)> = verify_only
            .into_iter()
            .filter(|version| last_end.get(version).is_none_or(|end| now >= *end))
            .collect();
        if retired.is_empty() {
            return Ok(retired);
        }
        let events: Vec<EventRecord> = retired
            .iter()
            .map(|(generation, version)| {
                CaEvent::IntermediateRetired {
                    at: now,
                    generation: *generation,
                    intermediate_version: *version,
                    reason: RetireReason::ExpiredCertificates,
                }
                .encode()
            })
            .collect();
        self.store.transaction(&mut |writes| {
            writes.append(CA_STREAM, &events, ca.stream_version)?;
            for (generation, version) in &retired {
                writes.delete_ca_material(*generation, *version)?;
            }
            Ok(())
        })?;
        Ok(retired)
    }

    /// Drops a ceremony that expired or belongs to an unseal that is over, so its key does not
    /// stay in memory.
    pub fn forget_lost_ceremony(&self) {
        let epoch = self.sealing.unsealed_epoch().ok();
        drop_lost(&mut lock(&self.ceremony), epoch, self.now());
    }

    /// Renews Hécate's certificate with a new key once two thirds of its life have passed, also
    /// after it expired; only while unsealed and with a valid intermediate. Returns the new
    /// serial.
    pub fn renew_own_certificate_if_due(&self) -> Result<Option<Serial>, ServiceError> {
        let _renewing = lock(&self.renewal);
        if self.sealing.unsealed_epoch().is_err() {
            return Ok(None);
        }
        let (Some(ca), Some(own)) = (self.ca()?, self.current_own()?) else {
            return Ok(None);
        };
        let now = self.now();
        let (not_before, not_after) = (own.issued.not_before, own.issued.not_after);
        if now < not_before + (not_after - not_before) * 2 / 3 {
            return Ok(None);
        }
        let Some(active) = ca.active().filter(|active| now < active.not_after) else {
            return Ok(None);
        };
        let material = self.material(active.generation, active.version)?;
        let (key, issued) = {
            let intermediate_key = self.intermediate_key(&material)?;
            self.issue_own(
                &ca.trust_domain,
                (active.generation, active.version),
                &material.intermediate_certificate,
                &intermediate_key,
                active.not_after,
                Some(own.issued.serial.clone()),
                now,
            )?
        };
        let new_serial = issued.serial.clone();
        let issued_events = [CertificateEvent::CertificateIssued(Box::new(issued)).encode()];
        let renewed_events = [CertificateEvent::CertificateRenewed {
            at: now,
            new_serial: new_serial.clone(),
        }
        .encode()];
        self.store.transaction(&mut |writes| {
            writes.append(&own_stream(&new_serial), &issued_events, 0)?;
            writes.append(
                &own_stream(&own.issued.serial),
                &renewed_events,
                own.stream_version,
            )?;
            writes.save_own_tls_key(&key)?;
            Ok(())
        })?;
        self.log(
            Operation::CertificateRenew,
            Method::Internal,
            Some(HECATE),
            AccessResult::Success,
            Some(HECATE),
            Some(new_serial.as_str()),
            "self",
        )?;
        Ok(Some(new_serial))
    }

    /// Hécate's current certificate revoked urgently and replaced at once by a new one with a new
    /// key, in one transaction (FR-012); the new one leaves its row from Hécate. `None` when
    /// `serial` is not the current one.
    pub fn replace_own_certificate(
        &self,
        serial: &Serial,
        revocation: &EventRecord,
        expected_version: u64,
        now: OffsetDateTime,
    ) -> Result<Option<Serial>, ServiceError> {
        let _renewing = lock(&self.renewal);
        let (Some(ca), Some(own)) = (self.ca()?, self.current_own()?) else {
            return Ok(None);
        };
        if own.issued.serial != *serial {
            return Ok(None);
        }
        let active = ca
            .active()
            .filter(|active| now < active.not_after)
            .ok_or_else(|| conflict("the current intermediate expired"))?;
        let material = self.material(active.generation, active.version)?;
        let (key, issued) = {
            let intermediate_key = self.intermediate_key(&material)?;
            self.issue_own(
                &ca.trust_domain,
                (active.generation, active.version),
                &material.intermediate_certificate,
                &intermediate_key,
                active.not_after,
                None,
                now,
            )?
        };
        let new_serial = issued.serial.clone();
        let issued_events = [CertificateEvent::CertificateIssued(Box::new(issued)).encode()];
        self.store.transaction(&mut |writes| {
            writes.append(
                &own_stream(serial),
                std::slice::from_ref(revocation),
                expected_version,
            )?;
            writes.append(&own_stream(&new_serial), &issued_events, 0)?;
            writes.save_own_tls_key(&key)?;
            Ok(())
        })?;
        self.log(
            Operation::CertificateRenew,
            Method::Internal,
            Some(HECATE),
            AccessResult::Success,
            Some(HECATE),
            Some(new_serial.as_str()),
            "reissue",
        )?;
        Ok(Some(new_serial))
    }

    /// The `cRLNumber` of CRLs generated now: the instant in milliseconds, always above the
    /// previous one (research R4).
    fn crl_number(&self, now: OffsetDateTime) -> u64 {
        let millis = u64::try_from(now.unix_timestamp_nanos() / 1_000_000).unwrap_or_default();
        let mut last = lock(&self.crl_number);
        *last = millis.max(*last + 1);
        *last
    }

    /// The CRL of every intermediate of the current CA that is not retired, signed by it with the
    /// revocations of the certificates it issued, given with their intermediate version (research
    /// R4), and their `nextUpdate`, `valid_for` after the second of `now`.
    pub fn crls(
        &self,
        revoked: &[(u64, RevokedCertificate)],
        now: OffsetDateTime,
        valid_for: Duration,
    ) -> Result<(Vec<Vec<u8>>, OffsetDateTime), ServiceError> {
        let ca = self.ca()?.ok_or_else(|| conflict("there is no CA"))?;
        let this_update = whole_seconds(now);
        let next_update = this_update + valid_for;
        let number = self.crl_number(now);
        let mut crls = Vec::new();
        for intermediate in ca.intermediates.values() {
            if intermediate.generation != ca.generation
                || intermediate.state == IntermediateState::Retired
            {
                continue;
            }
            let material = self.material(intermediate.generation, intermediate.version)?;
            let key = self.intermediate_key(&material)?;
            let entries: Vec<RevokedCertificate> = revoked
                .iter()
                .filter(|(version, _)| *version == intermediate.version)
                .map(|(_, entry)| entry.clone())
                .collect();
            crls.push(self.pki.crl(
                &Issuer {
                    certificate: &material.intermediate_certificate,
                    key: &key,
                },
                &entries,
                this_update,
                next_update,
                number,
            )?);
        }
        Ok((crls, next_update))
    }

    /// The `ca` block of `status`; `None` without a CA.
    pub fn status(&self) -> Result<Option<CaSummary>, ServiceError> {
        let Some(ca) = self.ca()? else {
            return Ok(None);
        };
        let hecate_certificate = self.current_own()?.map(|own| OwnCertificate {
            serial: own.issued.serial,
            not_after: own.issued.not_after,
        });
        let now = self.now();
        let intermediate_days_left = ca
            .active()
            .and_then(|active| days_left(active.not_after, now));
        Ok(Some(CaSummary {
            trust_domain: ca.trust_domain,
            root_fingerprint: ca.root_fingerprint,
            hecate_certificate,
            intermediate_days_left,
        }))
    }

    /// What the API presents, only while Hécate's certificate is valid (research R8).
    pub fn tls_material(&self) -> Result<Option<TlsMaterial>, ServiceError> {
        let (Some(ca), Some(mut own)) = (self.ca()?, self.current_own()?) else {
            return Ok(None);
        };
        let now = self.now();
        if now < own.issued.not_before || now >= own.issued.not_after {
            return Ok(None);
        }
        let signer = self.material(own.issued.generation, own.issued.intermediate_version)?;
        let current_root = self
            .store
            .load_ca_material()?
            .into_iter()
            .find(|material| material.generation == ca.generation)
            .map(|material| material.root_certificate)
            .ok_or_else(|| ServiceError::internal("MissingCaMaterial"))?;
        Ok(Some(TlsMaterial {
            chain: vec![
                own.issued.certificate,
                signer.intermediate_certificate,
                signer.root_certificate,
            ],
            key: std::mem::replace(&mut own.key.private_key, SecretBytes::new(Vec::new())),
            root: current_root,
            not_after: own.issued.not_after,
        }))
    }
}
