//! The ports of the application layer (research R13): synchronous traits used as `Arc<dyn …>`,
//! with transactions as closures that receive a write handle.

use hecate_domain::sealing::access_log::AccessLogEntry;
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::SealConfig;
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use time::OffsetDateTime;

use crate::errors::ServiceError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CryptoError(pub String);

pub trait CryptoProvider: Send + Sync {
    /// Random bytes that are not secret (salts, nonces, identifiers).
    fn random_bytes(&self, n: usize) -> Vec<u8>;
    /// A random 32-byte secret (root, recovery, scope or version key).
    fn random_key(&self) -> SecretKey;
    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        opslimit: u64,
        memlimit: usize,
    ) -> Result<SecretKey, CryptoError>;
    /// AES-256-GCM with a fresh 12-byte nonce; returns the nonce and the ciphertext with its tag.
    fn aead_encrypt(
        &self,
        key: &SecretKey,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CryptoError>;
    fn aead_decrypt(
        &self,
        key: &SecretKey,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, CryptoError>;
    fn hmac_sha256(&self, key: &SecretKey, data: &[u8]) -> [u8; 32];
    /// HKDF-SHA256 without salt, 32 bytes of output.
    fn hkdf_sha256(&self, key: &SecretKey, info: &[u8]) -> SecretKey;
    /// Splits a 32-byte secret into three shares (index 1 to 3); any two recover it.
    fn shamir_split(&self, secret: &SecretKey) -> Result<Vec<(u8, SecretBytes)>, CryptoError>;
    fn shamir_combine(&self, shares: &[(u8, &[u8])]) -> Result<SecretKey, CryptoError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// The writes of one sealing transaction: they all commit together or not at all.
pub trait SealWrites {
    fn load_config(&mut self) -> Result<Option<SealConfig>, ServiceError>;
    fn save_config(&mut self, config: &SealConfig) -> Result<(), ServiceError>;
    fn load_attempts(&mut self) -> Result<AttemptPolicy, ServiceError>;
    fn save_attempts(&mut self, attempts: &AttemptPolicy) -> Result<(), ServiceError>;
}

pub type SealWork<'a> = dyn FnMut(&mut dyn SealWrites) -> Result<(), ServiceError> + 'a;

pub trait SealRepository: Send + Sync {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError>;
    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError>;
    fn transaction(&self, work: &mut SealWork<'_>) -> Result<(), ServiceError>;
}

pub trait AccessLog: Send + Sync {
    fn append(&self, entry: &AccessLogEntry) -> Result<(), ServiceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the stream is at version {current_version}")]
pub struct ConcurrencyConflict {
    pub current_version: u64,
}

/// A stored event: its type, its JSON data (never a value or a key) and its instant.
pub use hecate_domain::events::EventRecord;

/// A scope key wrapped by the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// The encrypted value of one version and its version key wrapped by the scope key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMaterial {
    pub key_version: u64,
    pub value_nonce: Vec<u8>,
    pub value_ciphertext: Vec<u8>,
    pub dek_nonce: Vec<u8>,
    pub dek_ciphertext: Vec<u8>,
}

pub trait EventStore: Send + Sync {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError>;
    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError>;
}

pub trait MaterialStore: Send + Sync {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError>;
    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError>;
}

/// The writes of one secrets transaction: events and material commit together or not at all.
pub trait SecretWrites {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError>;
    /// Appends events if the stream is still at `expected_version`; otherwise
    /// [`ServiceError::Concurrency`]. Returns the new stream version.
    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError>;
    fn save_secret_material(
        &mut self,
        path: &str,
        version: u64,
        material: &SecretMaterial,
    ) -> Result<(), ServiceError>;
    fn load_secret_material(
        &mut self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError>;
    fn delete_secret_material(&mut self, path: &str, version: u64) -> Result<(), ServiceError>;
    fn save_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
        material: &KeyMaterial,
    ) -> Result<(), ServiceError>;
    fn load_key_material(
        &mut self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError>;
}

pub type SecretWork<'a> = dyn FnMut(&mut dyn SecretWrites) -> Result<(), ServiceError> + 'a;

pub trait SecretStore: EventStore + MaterialStore {
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError>;
}

// The identities (feature 003): the PKI, the short call token, the store of the CA, the identities
// and the certificates, and the enrollment throttle.

use hecate_domain::events::RevocationReason;
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::{RootFingerprint, Serial, SpiffeId, TrustDomain};
use hecate_domain::sealing::errors::HecateError;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PkiError(pub String);

/// The certificate profiles of contracts/formats.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateProfile {
    Root {
        trust_domain: TrustDomain,
    },
    Intermediate {
        trust_domain: TrustDomain,
        generation: u64,
        version: u64,
    },
    /// A component or Hécate: its only name is the SPIFFE ID.
    Leaf {
        spiffe_id: SpiffeId,
    },
}

const ROOT_NAME: &str = "Cerbero Root CA ";
const INTERMEDIATE_NAME: &str = "Cerbero Intermediate CA ";

impl CertificateProfile {
    /// The only attribute of the subject.
    pub fn common_name(&self) -> String {
        match self {
            Self::Root { trust_domain } => format!("{ROOT_NAME}{trust_domain}"),
            Self::Intermediate {
                trust_domain,
                generation,
                version,
            } => format!("{INTERMEDIATE_NAME}{trust_domain} {generation}.{version}"),
            Self::Leaf { spiffe_id } => spiffe_id.identity().to_string(),
        }
    }

    /// The intermediate whose common name this is, written exactly as [`Self::common_name`] does.
    pub fn intermediate_named(common_name: &str) -> Option<Self> {
        let rest = common_name.strip_prefix(INTERMEDIATE_NAME)?;
        let (trust_domain, numbers) = rest.split_once(' ')?;
        let (generation, version) = numbers.split_once('.')?;
        let profile = Self::Intermediate {
            trust_domain: TrustDomain::parse(trust_domain).ok()?,
            generation: generation.parse().ok()?,
            version: version.parse().ok()?,
        };
        (profile.common_name() == common_name).then_some(profile)
    }
}

/// What Hécate reads of a certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateInfo {
    pub serial: Serial,
    pub subject: String,
    pub issuer: String,
    pub spiffe_id: Option<SpiffeId>,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    pub is_ca: bool,
    /// The SubjectPublicKeyInfo in DER.
    pub public_key: Vec<u8>,
}

/// A certificate request whose signature and key were checked: the URI SAN it asks for, if it
/// asks for one, and the common name of its subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateRequest {
    /// The SubjectPublicKeyInfo in DER.
    pub public_key: Vec<u8>,
    pub requested_uri: Option<String>,
    pub common_name: Option<String>,
}

/// A certificate to issue.
#[derive(Debug, Clone, Copy)]
pub struct Issuance<'a> {
    pub profile: &'a CertificateProfile,
    /// The SubjectPublicKeyInfo of the subject, in DER.
    pub public_key: &'a [u8],
    pub serial: &'a Serial,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
}

/// One entry of a CRL: a certificate revoked urgently and not expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokedCertificate {
    pub serial: Serial,
    pub revoked_at: OffsetDateTime,
    pub reason: RevocationReason,
}

/// The CA that signs: its certificate in DER and its key in PKCS#8.
#[derive(Clone, Copy)]
pub struct Issuer<'a> {
    pub certificate: &'a [u8],
    pub key: &'a SecretBytes,
}

/// Certificates, requests and CRLs with ECDSA P-256 and SHA-256 (research R2).
pub trait PkiProvider: Send + Sync {
    /// A new P-256 key, its PKCS#8 in memory that is erased when dropped.
    fn generate_key(&self) -> Result<SecretBytes, PkiError>;
    /// The SubjectPublicKeyInfo of a key, in DER.
    fn public_key(&self, key: &SecretBytes) -> Result<Vec<u8>, PkiError>;
    /// A request for a key and a profile, in PEM.
    fn create_request(
        &self,
        key: &SecretBytes,
        profile: &CertificateProfile,
    ) -> Result<String, PkiError>;
    /// Reads a request in strict PEM and checks its signature and its P-256 key, and that a SAN, if
    /// it asks for one, is a single URI; anything else is `validation`.
    fn read_request(&self, pem: &str) -> Result<CertificateRequest, HecateError>;
    /// A certificate signed with its own key: the root CA.
    fn self_signed(&self, key: &SecretBytes, issuance: &Issuance<'_>) -> Result<Vec<u8>, PkiError>;
    /// A certificate signed by a CA; it never outlives the certificate of the CA.
    fn issue(&self, issuer: &Issuer<'_>, issuance: &Issuance<'_>) -> Result<Vec<u8>, PkiError>;
    fn read_certificate(&self, der: &[u8]) -> Result<CertificateInfo, PkiError>;
    /// Whether the certificate names the issuer and is signed by the key of its certificate.
    fn is_signed_by(&self, der: &[u8], issuer_der: &[u8]) -> Result<bool, PkiError>;
    /// Whether the certificate follows its profile of contracts/formats.md: the name, the
    /// extensions and their criticality, the key and the algorithm. The dates are the caller's.
    fn follows_profile(&self, der: &[u8], profile: &CertificateProfile) -> Result<bool, PkiError>;
    /// Reads a call key: one PEM `PUBLIC KEY` block with an Ed25519 SubjectPublicKeyInfo
    /// (RFC 8410); anything else is `validation`.
    fn read_call_key(&self, pem: &str) -> Result<CallKey, HecateError>;
    /// An X.509 v2 CRL signed by a CA (contracts/formats.md): `keyCompromise` after a suspicious
    /// renewal and `unspecified` when the admin asks for it.
    fn crl(
        &self,
        issuer: &Issuer<'_>,
        revoked: &[RevokedCertificate],
        this_update: OffsetDateTime,
        next_update: OffsetDateTime,
        number: u64,
    ) -> Result<Vec<u8>, PkiError>;
    /// Whether the chain of a client (its certificate and the intermediates that came with it)
    /// reaches the root CA and is valid at `now`, for client authentication; `Err` if it does not.
    fn verify_client_chain(
        &self,
        chain: &[Vec<u8>],
        root: &[u8],
        now: OffsetDateTime,
    ) -> Result<(), PkiError>;
    fn fingerprint(&self, der: &[u8]) -> RootFingerprint {
        RootFingerprint::of_der(der)
    }
}

/// What a verified short call token says (research R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTokenClaims {
    pub issuer: SpiffeId,
    pub audience: SpiffeId,
    pub issued_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub jti: String,
}

/// The longest life of a short call token.
pub const CALL_TOKEN_MAX_LIFETIME: time::Duration = time::Duration::seconds(300);
/// How much the clocks of the caller and of Hécate may differ when a short call token is checked.
pub const CALL_TOKEN_MARGIN: time::Duration = time::Duration::seconds(30);

/// Checks a short call token: EdDSA with the registered key, `iss` the connection's identity,
/// `aud` Hécate, a lifetime of 300 s at most with 30 s of margin. Each failure is `auth`; the
/// single use of `jti` is the application's.
pub trait CallTokenVerifier: Send + Sync {
    fn verify(
        &self,
        token: &str,
        key: &CallKey,
        issuer: &SpiffeId,
        audience: &SpiffeId,
        now: OffsetDateTime,
    ) -> Result<CallTokenClaims, HecateError>;
}

/// The certificates of one version of the CA and the intermediate key wrapped by the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaMaterial {
    pub generation: u64,
    pub intermediate_version: u64,
    pub root_certificate: Vec<u8>,
    pub intermediate_certificate: Vec<u8>,
    pub key_nonce: Vec<u8>,
    pub key_ciphertext: Vec<u8>,
}

/// The key of Hécate's own certificate, in clear (accepted risk, research R8); erased when
/// dropped, and it can be neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::ports::OwnTlsKey>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::ports::OwnTlsKey>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::ports::OwnTlsKey>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct OwnTlsKey {
    #[zeroize(skip)]
    pub serial: Serial,
    /// The PKCS#8 of the key.
    pub private_key: SecretBytes,
}

/// The writes of one transaction of the identities: events of several streams and material
/// commit together or not at all.
pub trait IdentityWrites {
    fn load(&mut self, stream: &str) -> Result<Vec<EventRecord>, ServiceError>;
    /// Appends events if the stream is still at `expected_version`; otherwise
    /// [`ServiceError::Concurrency`]. Returns the new stream version.
    fn append(
        &mut self,
        stream: &str,
        events: &[EventRecord],
        expected_version: u64,
    ) -> Result<u64, ServiceError>;
    fn save_ca_material(&mut self, material: &CaMaterial) -> Result<(), ServiceError>;
    /// Destroys the key of an intermediate (cryptographic erasure).
    fn delete_ca_material(
        &mut self,
        generation: u64,
        intermediate_version: u64,
    ) -> Result<(), ServiceError>;
    fn save_own_tls_key(&mut self, key: &OwnTlsKey) -> Result<(), ServiceError>;
}

pub type IdentityWork<'a> = dyn FnMut(&mut dyn IdentityWrites) -> Result<(), ServiceError> + 'a;

/// The streams `ca`, `identity/<name>` and `certificate/<identity>/<serial>`, the table
/// `ca_material` and the table `hecate_tls` (research R10).
pub trait IdentityStore: EventStore {
    fn transaction(&self, work: &mut IdentityWork<'_>) -> Result<(), ServiceError>;
    fn load_ca_material(&self) -> Result<Vec<CaMaterial>, ServiceError>;
    fn load_own_tls_key(&self) -> Result<Option<OwnTlsKey>, ServiceError>;
}

/// The state of the enrollment throttle (FR-034): it survives restarts.
pub use hecate_domain::identity::throttle::EnrollAttempts;

pub trait EnrollAttemptsRepository: Send + Sync {
    fn load(&self) -> Result<EnrollAttempts, ServiceError>;
    fn save(&self, attempts: &EnrollAttempts) -> Result<(), ServiceError>;
}
