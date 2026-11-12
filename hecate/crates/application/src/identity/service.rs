//! Enrollment tokens and enrollments (FR-007 to FR-011, FR-013, FR-015, FR-034): the admin creates,
//! lists and annuls tokens; a component enrolls with one and receives its first certificate,
//! signed by the active intermediate with the identity of the token as its only name. Only the
//! issuance uses up a token; a refused enrollment leaves it unused and counts for the throttle.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hecate_domain::ca::authority::CertificateAuthority;
use hecate_domain::events::{
    CertificateEvent, CertificateOrigin, EventRecord, IdentityEvent, InvalidEvents,
    IssuedCertificate, RevocationReason, RevokedBy,
};
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::certificate::{Certificate, CertificateState};
use hecate_domain::identity::identity::{Identity, IdentityStatus};
use hecate_domain::identity::names::{IdentityName, Serial, SpiffeId, TokenId};
use hecate_domain::identity::request::request_digest;
use hecate_domain::identity::throttle::EnrollAttempts;
use hecate_domain::identity::token::EnrollmentToken;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::secrets::path::SecretPath;
use serde_json::Value;
use time::{Duration, OffsetDateTime};
use zeroize::Zeroizing;

use crate::access_policy::service::AccessPolicyService;
use crate::ca::service::{CaService, HECATE};
use crate::errors::ServiceError;
use crate::identity::jti::SeenTokens;
use crate::identity::settings::IdentitySettings;
use crate::ports::{
    AccessLog, CallTokenVerifier, Clock, CryptoProvider, EnrollAttemptsRepository, IdentityStore,
    PkiProvider, RevokedCertificate,
};
use crate::sealing::service::SealingService;
use crate::secrets::service::{ADMIN, CallContext, MAX_REFERENCES, Resolved, SecretsService};

pub const IDENTITY_PREFIX: &str = "identity/";
const CERTIFICATE_PREFIX: &str = "certificate/";
const TOKEN_ID_SIZE: usize = 8;
const MAX_KEY_LEN: usize = 128;
const ENROLL_FIELDS: [&str; 3] = ["call_key", "csr", "token"];
const RENEW_FIELDS: [&str; 2] = ["call_key", "csr"];
const DAY: Duration = Duration::hours(24);
/// How long a list of published keys or the CRLs are valid (research R4, R5).
const PUBLISHED_FOR: Duration = Duration::seconds(300);
const KID_LEN: usize = 43;
const NOT_VALID: &str = "the enrollment token is not valid";
/// How many times an operation reads again the streams after losing a race for them.
const MAX_RACES: usize = 3;

/// A token just created: it is shown once, and its text is erased when dropped. It can be neither
/// cloned nor shown.
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::identity::service::TokenCreated>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::identity::service::TokenCreated>();
/// ```
pub struct TokenCreated {
    pub token: Zeroizing<String>,
    pub token_id: TokenId,
    pub identity: IdentityName,
    pub expires_at: OffsetDateTime,
}

/// A pending token as the admin lists it, without its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingToken {
    pub token_id: TokenId,
    pub identity: IdentityName,
    pub expires_at: OffsetDateTime,
}

/// A certificate issued by an enrollment, or the one its exact repetition returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrolled {
    pub identity: SpiffeId,
    pub serial: Serial,
    /// The DER of the certificate.
    pub certificate: Vec<u8>,
    /// The intermediate and the root CA, in DER.
    pub chain: Vec<Vec<u8>>,
    pub not_after: OffsetDateTime,
    pub call_key_id: String,
}

/// What a call to the API brings to authenticate itself: the certificates of its connection,
/// its own first, and the short call token of `Authorization: Bearer`.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub certificates: Vec<Vec<u8>>,
    pub bearer: Option<String>,
}

/// Why a call of a component was refused before its use case ran (FR-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallRefusal {
    /// Hécate is sealed: `unavailable`.
    Sealed,
    /// Its `X-Request-Id` does not follow its rule: `validation`.
    RequestId,
}

/// A call key as Hécate publishes it (research R5): with its identity, whether it signs new
/// calls (`active`) or only verifies past ones (`verify_only`), and the end of its certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedKey {
    pub key: CallKey,
    pub identity: SpiffeId,
    pub active: bool,
    pub not_after: OffsetDateTime,
}

/// The keys valid for new calls, and until when the list is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedKeys {
    pub keys: Vec<PublishedKey>,
    pub expires_at: OffsetDateTime,
}

/// What a revocation did: the certificate and, when it was Hécate's current one, the serial of
/// the certificate that replaced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    pub serial: Serial,
    pub reissued: Option<Serial>,
}

/// What a disable did: the serials it revoked urgently now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disabled {
    pub identity: IdentityName,
    pub revoked: Vec<Serial>,
}

/// An identity as `identities.list` shows it, with the end of its most recent certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySummary {
    pub identity: IdentityName,
    pub status: IdentityStatus,
    pub certificate_not_after: OffsetDateTime,
}

/// A certificate as `identities.get` shows it, without keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateSummary {
    pub serial: Serial,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    pub state: CertificateState,
    pub renewable: bool,
}

/// An identity with its certificates, from the oldest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityDetail {
    pub identity: IdentityName,
    pub status: IdentityStatus,
    pub certificates: Vec<CertificateSummary>,
}

/// The CRLs in DER and until when they are valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revocations {
    pub crls: Vec<Vec<u8>>,
    pub expires_at: OffsetDateTime,
}

/// The counts of `status`, which also answer while sealed and name no identity (FR-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityCounts {
    pub identities_expiring_24h: u64,
    pub suspicious_renewals_24h: u64,
}

/// The aggregate status that a component with the status permission reads (US3, FR-011): counts
/// computed from metadata, which name no secret and no identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggregateStatus {
    pub sealed: bool,
    pub secrets_expired: u64,
    pub secrets_expiring_soon: u64,
    pub secrets_due_rotation: u64,
    pub identities_expiring_24h: u64,
    pub suspicious_renewals_24h: u64,
    /// The days left to the active intermediate, only when fewer than 30.
    pub intermediate_days_left: Option<u64>,
    pub enrollment_throttled: bool,
}

/// What the row of an operation of a component says: the identity it names, who sent it once
/// recognized, the certificate, and its detail.
#[derive(Default)]
struct Trace {
    identity: Option<String>,
    subject: Option<String>,
    certificate: Option<String>,
    detail: Option<String>,
    repeat: bool,
}

/// The body of an enrollment, checked: exactly its three texts, and its Idempotency-Key.
struct EnrollRequest<'a> {
    token: &'a str,
    csr: &'a str,
    call_key: &'a str,
    idempotency_key: &'a str,
}

/// A caller authenticated by the certificate of its connection and a short call token.
struct Caller {
    spiffe_id: SpiffeId,
    identity: IdentityName,
    serial: Serial,
    public_key: Vec<u8>,
    call_key: Option<CallKey>,
}

fn invalid(message: impl Into<String>) -> ServiceError {
    HecateError::invalid(message).into()
}

fn conflict(message: &str) -> ServiceError {
    HecateError::state_conflict(None, Some(message.to_string()), None).into()
}

fn auth() -> ServiceError {
    HecateError::auth(NOT_VALID).into()
}

fn refused(message: &str) -> ServiceError {
    HecateError::auth(message).into()
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn identity_stream(name: &IdentityName) -> String {
    format!("{IDENTITY_PREFIX}{name}")
}

fn certificate_prefix(name: &IdentityName) -> String {
    format!("certificate/{name}/")
}

/// A JWK thumbprint of RFC 7638 in base64url without padding.
fn is_kid(text: &str) -> bool {
    text.len() == KID_LEN
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// From 1 to 128 printable ASCII characters, as `X-Request-Id`.
fn is_header_text(text: &str) -> bool {
    (1..=MAX_KEY_LEN).contains(&text.len())
        && text.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

/// The texts of a body that has exactly these fields, in that order.
fn body_texts<'a>(body: &'a Value, names: &[&str]) -> Result<Vec<&'a str>, ServiceError> {
    let fields = body
        .as_object()
        .filter(|fields| {
            let mut present: Vec<&str> = fields.keys().map(String::as_str).collect();
            present.sort_unstable();
            present == names
        })
        .ok_or_else(|| invalid(format!("the body must have exactly {}", names.join(", "))))?;
    names
        .iter()
        .map(|name| {
            fields[*name]
                .as_str()
                .ok_or_else(|| invalid(format!("'{name}' must be a string")))
        })
        .collect()
}

fn checked_key(idempotency_key: Option<&str>) -> Result<&str, ServiceError> {
    idempotency_key
        .filter(|key| is_header_text(key))
        .ok_or_else(|| invalid("Idempotency-Key must have 1 to 128 printable ASCII characters"))
}

fn parse_request<'a>(
    idempotency_key: Option<&'a str>,
    body: &'a Value,
) -> Result<EnrollRequest<'a>, ServiceError> {
    let texts = body_texts(body, &ENROLL_FIELDS)?;
    let idempotency_key = checked_key(idempotency_key)?;
    Ok(EnrollRequest {
        call_key: texts[0],
        csr: texts[1],
        token: texts[2],
        idempotency_key,
    })
}

/// The references of a resolution: the body is exactly `{"references": [...]}`.
fn references_of(body: &Value) -> Result<&Value, ServiceError> {
    let rule = || {
        invalid(format!(
            "the body is {{\"references\": [...]}} with 1 to {MAX_REFERENCES} secret references"
        ))
    };
    match body.as_object() {
        Some(fields) if fields.len() == 1 => fields.get("references").ok_or_else(rule),
        _ => Err(rule()),
    }
}

fn decode_identity(name: IdentityName, records: &[EventRecord]) -> Result<Identity, ServiceError> {
    let events = records
        .iter()
        .map(IdentityEvent::decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(corrupt)?;
    Identity::from_events(name, &events).map_err(corrupt)
}

fn decode_certificate(records: &[EventRecord]) -> Result<Certificate, ServiceError> {
    let events = records
        .iter()
        .map(CertificateEvent::decode)
        .collect::<Result<Vec<_>, _>>()
        .map_err(corrupt)?;
    Certificate::from_events(&events).map_err(corrupt)
}

pub struct IdentityService {
    sealing: Arc<SealingService>,
    ca: Arc<CaService>,
    policies: Arc<AccessPolicyService>,
    secrets: Arc<SecretsService>,
    store: Arc<dyn IdentityStore>,
    attempts: Arc<dyn EnrollAttemptsRepository>,
    pki: Arc<dyn PkiProvider>,
    crypto: Arc<dyn CryptoProvider>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
    verifier: Arc<dyn CallTokenVerifier>,
    settings: IdentitySettings,
    /// Enrollments go one at a time: their throttle and their tokens never race.
    enrolling: Mutex<()>,
    seen: SeenTokens,
}

impl IdentityService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sealing: Arc<SealingService>,
        ca: Arc<CaService>,
        policies: Arc<AccessPolicyService>,
        secrets: Arc<SecretsService>,
        store: Arc<dyn IdentityStore>,
        attempts: Arc<dyn EnrollAttemptsRepository>,
        pki: Arc<dyn PkiProvider>,
        crypto: Arc<dyn CryptoProvider>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
        verifier: Arc<dyn CallTokenVerifier>,
        settings: IdentitySettings,
    ) -> Self {
        let seen = SeenTokens::new(clock.now());
        Self {
            sealing,
            ca,
            policies,
            secrets,
            store,
            attempts,
            pki,
            crypto,
            access_log,
            clock,
            verifier,
            settings,
            enrolling: Mutex::new(()),
            seen,
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

    /// Leaves a row for an operation of the admin with the identity and the certificate that
    /// `work` names.
    fn audited<T>(
        &self,
        operation: Operation,
        detail: &str,
        work: impl FnOnce(&mut Trace) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let mut trace = Trace::default();
        let outcome = work(&mut trace);
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(
            operation,
            Method::Local,
            Some(ADMIN),
            result,
            trace.identity.as_deref(),
            trace.certificate.as_deref(),
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

    /// Leaves the row of a call of a component refused before its use case ran (FR-024): with
    /// its operation and without a subject, an identity or a certificate, since nobody was
    /// authenticated. Sealed, it is `rejected_conflict`, as on the local channel.
    ///
    /// An enrollment refused for its request id goes through the throttle, like one with a
    /// malformed Idempotency-Key (FR-034): it counts as a failure, and while the enrollments are
    /// throttled it is `rate_limited`, counted in the batch without a row of its own. Sealed, it
    /// does not count, as in [`Self::enroll`].
    pub fn record_refused_call(
        &self,
        operation: Operation,
        method: Method,
        refusal: CallRefusal,
    ) -> Result<(), ServiceError> {
        let (result, detail) = match refusal {
            CallRefusal::Sealed => (AccessResult::RejectedConflict, "sealed"),
            CallRefusal::RequestId => (AccessResult::InvalidInput, "request_id"),
        };
        if operation == Operation::IdentityEnroll
            && refusal == CallRefusal::RequestId
            && self.sealing.unsealed_epoch().is_ok()
        {
            let _turn = lock(&self.enrolling);
            let now = self.now();
            let attempts = self.admitted(now)?;
            self.attempts.save(&attempts.register_failure(now))?;
        }
        self.log(operation, method, None, result, None, None, detail)
    }

    /// The epoch of the unseal for a call of a component; sealed, the refusal leaves its row.
    fn unsealed_for(&self, operation: Operation, method: Method) -> Result<u64, ServiceError> {
        self.sealing.unsealed_epoch().or_else(|error| {
            self.record_refused_call(operation, method, CallRefusal::Sealed)?;
            Err(error)
        })
    }

    fn append(
        &self,
        stream: &str,
        records: &[EventRecord],
        expected_version: u64,
    ) -> Result<(), ServiceError> {
        self.store.transaction(&mut |writes| {
            writes.append(stream, records, expected_version)?;
            Ok(())
        })
    }

    fn token_id(&self) -> TokenId {
        let mut bytes = [0; TOKEN_ID_SIZE];
        bytes.copy_from_slice(&self.crypto.random_bytes(TOKEN_ID_SIZE));
        TokenId::from_bytes(bytes)
    }

    fn load_identity(&self, name: &IdentityName) -> Result<Identity, ServiceError> {
        decode_identity(name.clone(), &self.store.load(&identity_stream(name))?)
    }

    fn identities(&self) -> Result<Vec<Identity>, ServiceError> {
        self.store
            .streams(IDENTITY_PREFIX)?
            .iter()
            .map(|stream| {
                let name = IdentityName::parse(&stream[IDENTITY_PREFIX.len()..])
                    .map_err(|_| ServiceError::internal("InvalidStream"))?;
                decode_identity(name, &self.store.load(stream)?)
            })
            .collect()
    }

    fn load_certificate(
        &self,
        name: &IdentityName,
        serial: &Serial,
    ) -> Result<Certificate, ServiceError> {
        decode_certificate(
            &self
                .store
                .load(&format!("{}{serial}", certificate_prefix(name)))?,
        )
    }

    fn certificates_of(&self, name: &IdentityName) -> Result<Vec<Certificate>, ServiceError> {
        self.store
            .streams(&certificate_prefix(name))?
            .iter()
            .map(|stream| decode_certificate(&self.store.load(stream)?))
            .collect()
    }

    fn authority(&self) -> Result<CertificateAuthority, ServiceError> {
        self.ca
            .authority()?
            .ok_or_else(|| conflict("there is no CA"))
    }

    /// A token for an identity, with a 32-byte random secret and the fingerprint of the current
    /// root CA; Hécate keeps only the SHA-256 of the secret (FR-007).
    pub fn tokens_create(&self, identity: &str) -> Result<TokenCreated, ServiceError> {
        self.audited(Operation::TokenCreate, "create", |trace| {
            trace.identity = IdentityName::parse(identity)
                .ok()
                .map(|name| name.to_string());
            self.sealing.unsealed_epoch()?;
            let name = IdentityName::parse(identity)?;
            if name.is_reserved() {
                return Err(invalid(format!("the identity '{name}' is reserved")));
            }
            let ca = self.authority()?;
            let now = self.now();
            if !ca.active().is_some_and(|active| now < active.not_after) {
                return Err(conflict("the current intermediate expired"));
            }
            for _ in 0..MAX_RACES {
                let current = self.load_identity(&name)?;
                let token = EnrollmentToken::new(
                    name.clone(),
                    self.token_id(),
                    self.crypto.random_key(),
                    ca.root_fingerprint,
                );
                let expires_at = now + self.settings.token_ttl;
                let event = current.create_token(
                    token.id().clone(),
                    token.secret_sha256(),
                    ca.generation,
                    expires_at,
                    now,
                )?;
                match self.append(
                    &identity_stream(&name),
                    &[event.encode()],
                    current.stream_version,
                ) {
                    Ok(()) => {
                        return Ok(TokenCreated {
                            token: token.text(),
                            token_id: token.id().clone(),
                            identity: name,
                            expires_at,
                        });
                    }
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ServiceError::internal("TooManyRaces"))
        })
    }

    /// The pending tokens, without their value.
    pub fn tokens_list(&self) -> Result<Vec<PendingToken>, ServiceError> {
        self.audited(Operation::TokenList, "list", |_| {
            self.sealing.unsealed_epoch()?;
            let Some(ca) = self.ca.authority()? else {
                return Ok(Vec::new());
            };
            let now = self.now();
            let mut pending = Vec::new();
            for identity in self.identities()? {
                for token in identity.pending_tokens(ca.generation, now) {
                    pending.push(PendingToken {
                        token_id: token.id.clone(),
                        identity: identity.name.clone(),
                        expires_at: token.expires_at,
                    });
                }
            }
            pending.sort_by(|a, b| (a.expires_at, &a.token_id).cmp(&(b.expires_at, &b.token_id)));
            Ok(pending)
        })
    }

    /// Annuls a pending token; one that is no longer pending changes nothing (FR-026).
    pub fn tokens_annul(&self, token_id: &str) -> Result<TokenId, ServiceError> {
        self.audited(Operation::TokenAnnul, "annul", |trace| {
            self.sealing.unsealed_epoch()?;
            let id = TokenId::parse(token_id)?;
            let generation = self.ca.authority()?.map_or(0, |ca| ca.generation);
            let now = self.now();
            for _ in 0..MAX_RACES {
                let identity = self
                    .identities()?
                    .into_iter()
                    .find(|identity| identity.tokens.contains_key(&id))
                    .ok_or_else(|| HecateError::not_found("token not found"))?;
                trace.identity = Some(identity.name.to_string());
                let Some(event) = identity.annul(&id, generation, now, now)? else {
                    return Ok(id);
                };
                match self.append(
                    &identity_stream(&identity.name),
                    &[event.encode()],
                    identity.stream_version,
                ) {
                    Ok(()) => return Ok(id),
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ServiceError::internal("TooManyRaces"))
        })
    }

    /// Whether enrollments are throttled now (FR-025, `status`).
    pub fn enrollment_throttled(&self) -> Result<bool, ServiceError> {
        Ok(self.attempts.load()?.is_throttled(self.now()))
    }

    /// Closes the batch whose wait is over with its row; the task of every minute calls it.
    pub fn close_throttle_batch(&self) -> Result<(), ServiceError> {
        let _turn = lock(&self.enrolling);
        self.close_batch(self.attempts.load()?, self.now())
            .map(|_| ())
    }

    /// The throttle in front of an enrollment, under the turn of the enrollments (FR-034): it
    /// closes a batch whose wait is over and, while the enrollments are throttled, counts the
    /// request in the batch and refuses it with `rate_limited`.
    fn admitted(&self, now: OffsetDateTime) -> Result<EnrollAttempts, ServiceError> {
        let attempts = self.close_batch(self.attempts.load()?, now)?;
        if attempts.is_throttled(now) {
            self.attempts.save(&attempts.register_rejected())?;
            return Err(HecateError::RateLimited {
                retry_after_s: attempts.retry_after(now),
            }
            .into());
        }
        Ok(attempts)
    }

    fn close_batch(
        &self,
        attempts: EnrollAttempts,
        now: OffsetDateTime,
    ) -> Result<EnrollAttempts, ServiceError> {
        let Some((batch, after)) = attempts.close_batch(now) else {
            return Ok(attempts);
        };
        self.attempts.save(&after)?;
        self.log(
            Operation::IdentityEnrollThrottled,
            Method::Internal,
            None,
            AccessResult::RejectedRateLimited,
            None,
            None,
            &format!("failures {}, rejected {}", batch.failures, batch.rejected),
        )?;
        Ok(after)
    }

    /// An enrollment with a token (FR-008 to FR-011). Sealed, Hécate answers `unavailable` and
    /// throttled, `rate_limited`: neither counts as a failure (FR-034). The sealed refusal leaves
    /// its row; the throttled ones, one per batch.
    pub fn enroll(
        &self,
        idempotency_key: Option<&str>,
        body: &Value,
    ) -> Result<Enrolled, ServiceError> {
        self.unsealed_for(Operation::IdentityEnroll, Method::EnrollmentToken)?;
        let _turn = lock(&self.enrolling);
        let now = self.now();
        let attempts = self.admitted(now)?;
        let mut trace = Trace::default();
        let outcome = self.enroll_checked(idempotency_key, body, now, &mut trace);
        match &outcome {
            Ok(_) if trace.repeat => {}
            Ok(_) if attempts != EnrollAttempts::default() => {
                self.attempts.save(&attempts.register_success())?;
            }
            Ok(_) => {}
            Err(ServiceError::Hecate(_)) => self.attempts.save(&attempts.register_failure(now))?,
            Err(_) => {}
        }
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(
            Operation::IdentityEnroll,
            Method::EnrollmentToken,
            trace.subject.as_deref(),
            result,
            trace.identity.as_deref(),
            trace.certificate.as_deref(),
            if trace.repeat { "repeat" } else { "enroll" },
        )?;
        outcome
    }

    fn enroll_checked(
        &self,
        idempotency_key: Option<&str>,
        body: &Value,
        now: OffsetDateTime,
        trace: &mut Trace,
    ) -> Result<Enrolled, ServiceError> {
        let request = parse_request(idempotency_key, body)?;
        let token = EnrollmentToken::parse(request.token)?;
        trace.identity = Some(token.identity().to_string());
        let csr = self.pki.read_request(request.csr)?;
        let call_key = self.pki.read_call_key(request.call_key)?;
        let digest = request_digest(&[
            ("call_key", request.call_key),
            ("csr", request.csr),
            ("token", request.token),
        ]);
        let ca = self.ca.authority()?.ok_or_else(auth)?;
        let spiffe_id = SpiffeId::new(ca.trust_domain.clone(), token.identity().clone());
        for _ in 0..MAX_RACES {
            let identity = self.load_identity(token.identity())?;
            let record = identity
                .tokens
                .get(token.id())
                .filter(|record| token.matches_hash(&record.secret_sha256));
            if record.is_some() {
                trace.subject = Some(spiffe_id.to_string());
            }
            // The exact repetition of the request that used the token gets its certificate.
            if let Some(serial) = record.and_then(|record| record.used_by.as_ref()) {
                let certificate = self.load_certificate(token.identity(), serial)?;
                if certificate.issued.request_sha256 != Some(digest) {
                    return Err(auth());
                }
                trace.certificate = Some(serial.to_string());
                trace.repeat = true;
                return self.enrolled(&spiffe_id, &certificate.issued);
            }
            let pending = identity
                .pending_token(token.id(), ca.generation, now)
                .is_some_and(|record| token.matches_hash(&record.secret_sha256));
            if !pending {
                return Err(auth());
            }
            let reused = self
                .certificates_of(token.identity())?
                .iter()
                .any(|certificate| {
                    certificate.issued.idempotency_key.as_deref() == Some(request.idempotency_key)
                        && certificate.issued.request_sha256 != Some(digest)
                });
            if reused {
                return Err(conflict(
                    "the Idempotency-Key was used with another request",
                ));
            }
            if csr
                .requested_uri
                .as_ref()
                .is_some_and(|uri| *uri != spiffe_id.to_string())
            {
                return Err(invalid("the request asks for another identity"));
            }
            let leaf = self
                .ca
                .issue_component(token.identity(), &csr.public_key, now)?;
            let issued = IssuedCertificate {
                at: now,
                identity: token.identity().clone(),
                serial: leaf.serial,
                generation: leaf.generation,
                intermediate_version: leaf.intermediate_version,
                epoch: identity.epoch + 1,
                not_before: leaf.not_before,
                not_after: leaf.not_after,
                call_key: Some(call_key.clone()),
                origin: CertificateOrigin::Enrollment,
                renewed_from: None,
                request_sha256: Some(digest),
                idempotency_key: Some(request.idempotency_key.to_string()),
                certificate: leaf.certificate,
            };
            let identity_records = [IdentityEvent::IdentityEnrolled {
                at: now,
                token_id: token.id().clone(),
                serial: issued.serial.clone(),
            }
            .encode()];
            let certificate_records =
                [CertificateEvent::CertificateIssued(Box::new(issued.clone())).encode()];
            let stream = format!("{}{}", certificate_prefix(token.identity()), issued.serial);
            let written = self.store.transaction(&mut |writes| {
                writes.append(
                    &identity_stream(token.identity()),
                    &identity_records,
                    identity.stream_version,
                )?;
                writes.append(&stream, &certificate_records, 0)?;
                Ok(())
            });
            match written {
                Ok(()) => {
                    trace.certificate = Some(issued.serial.to_string());
                    return self.enrolled(&spiffe_id, &issued);
                }
                Err(ServiceError::Concurrency(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ServiceError::internal("TooManyRaces"))
    }

    fn enrolled(
        &self,
        identity: &SpiffeId,
        issued: &IssuedCertificate,
    ) -> Result<Enrolled, ServiceError> {
        Ok(Enrolled {
            identity: identity.clone(),
            serial: issued.serial.clone(),
            certificate: issued.certificate.clone(),
            chain: self
                .ca
                .chain(issued.generation, issued.intermediate_version)?,
            not_after: issued.not_after,
            call_key_id: issued
                .call_key
                .as_ref()
                .map(|key| key.kid())
                .unwrap_or_default(),
        })
    }

    fn find_certificate(
        &self,
        name: &IdentityName,
        serial: &Serial,
    ) -> Result<Option<Certificate>, ServiceError> {
        let records = self
            .store
            .load(&format!("{}{serial}", certificate_prefix(name)))?;
        if records.is_empty() {
            return Ok(None);
        }
        decode_certificate(&records).map(Some)
    }

    /// The caller of an operation of the API, by the certificate of its connection and a short
    /// call token (FR-014, FR-016, FR-021, research R1, R3). The chain is checked up to the
    /// current root CA at the time of the clock; then that Hécate issued and knows the
    /// certificate, that it is valid and its identity enabled, and the token, once.
    fn authenticate(
        &self,
        credentials: &Credentials,
        now: OffsetDateTime,
        trace: &mut Trace,
    ) -> Result<Caller, ServiceError> {
        let certificate = credentials
            .certificates
            .first()
            .ok_or_else(|| refused("a client certificate is required"))?;
        let not_known =
            || refused("the certificate is not one that Hécate issued with its current CA");
        let info = self
            .pki
            .read_certificate(certificate)
            .map_err(|_| not_known())?;
        trace.certificate = Some(info.serial.to_string());
        let spiffe_id = info.spiffe_id.clone().ok_or_else(not_known)?;
        trace.identity = Some(spiffe_id.identity().to_string());
        let ca = self.ca.authority()?.ok_or_else(not_known)?;
        let root = self.ca.root_certificate()?.ok_or_else(not_known)?;
        self.pki
            .verify_client_chain(&credentials.certificates, &root, now)
            .map_err(|_| not_known())?;
        if spiffe_id.trust_domain() != &ca.trust_domain {
            return Err(not_known());
        }
        let name = spiffe_id.identity().clone();
        let known = self
            .find_certificate(&name, &info.serial)?
            .filter(|known| {
                known.issued.certificate == *certificate && known.issued.generation == ca.generation
            })
            .ok_or_else(not_known)?;
        trace.subject = Some(spiffe_id.to_string());
        if known.state(now) != CertificateState::Valid {
            return Err(refused("the certificate is revoked or expired"));
        }
        if self.load_identity(&name)?.status != IdentityStatus::Enrolled {
            return Err(refused("the identity is disabled"));
        }
        let bearer = credentials
            .bearer
            .as_deref()
            .ok_or_else(|| refused("a short call token is required"))?;
        let key = known
            .issued
            .call_key
            .as_ref()
            .ok_or_else(|| refused("the certificate has no call key"))?;
        let hecate = SpiffeId::new(ca.trust_domain.clone(), IdentityName::hecate());
        let claims = self
            .verifier
            .verify(bearer, key, &spiffe_id, &hecate, now)?;
        let first_use = self.seen.accept(
            &spiffe_id.to_string(),
            &claims.jti,
            claims.issued_at,
            claims.expires_at,
            now,
        );
        if !first_use {
            return Err(refused("the short call token was already used"));
        }
        Ok(Caller {
            spiffe_id,
            identity: name,
            serial: info.serial,
            public_key: info.public_key,
            call_key: known.issued.call_key,
        })
    }

    /// Drops the short call tokens that can no longer be accepted; the task of every minute
    /// calls it.
    pub fn purge_seen_tokens(&self) {
        self.seen.purge(self.now());
    }

    /// A renewal with the current certificate and a short call token (FR-014 to FR-016): once,
    /// with new keys. Sealed, Hécate answers `unavailable`, with its row.
    pub fn renew(
        &self,
        credentials: &Credentials,
        idempotency_key: Option<&str>,
        body: &Value,
    ) -> Result<Enrolled, ServiceError> {
        self.unsealed_for(Operation::CertificateRenew, Method::Identity)?;
        let now = self.now();
        let mut trace = Trace::default();
        let outcome = self.renew_checked(credentials, idempotency_key, body, now, &mut trace);
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(
            Operation::CertificateRenew,
            Method::Identity,
            trace.subject.as_deref(),
            result,
            trace.identity.as_deref(),
            trace.certificate.as_deref(),
            trace.detail.as_deref().unwrap_or("renew"),
        )?;
        outcome
    }

    fn renew_checked(
        &self,
        credentials: &Credentials,
        idempotency_key: Option<&str>,
        body: &Value,
        now: OffsetDateTime,
        trace: &mut Trace,
    ) -> Result<Enrolled, ServiceError> {
        let caller = self.authenticate(credentials, now, trace)?;
        let texts = body_texts(body, &RENEW_FIELDS)?;
        let (call_key_text, csr_text) = (texts[0], texts[1]);
        let idempotency_key = checked_key(idempotency_key)?;
        let csr = self.pki.read_request(csr_text)?;
        let call_key = self.pki.read_call_key(call_key_text)?;
        if csr.public_key == caller.public_key {
            return Err(invalid("the new certificate key repeats the current one"));
        }
        if caller.call_key.as_ref() == Some(&call_key) {
            return Err(invalid("the new call key repeats the current one"));
        }
        if csr
            .requested_uri
            .as_ref()
            .is_some_and(|uri| *uri != caller.spiffe_id.to_string())
        {
            return Err(invalid("the request asks for another identity"));
        }
        let digest = request_digest(&[
            ("call_key", call_key_text),
            ("certificate", caller.serial.as_str()),
            ("csr", csr_text),
        ]);
        let ca = self.authority()?;
        for _ in 0..MAX_RACES {
            let identity = self.load_identity(&caller.identity)?;
            let certificate = self.load_certificate(&caller.identity, &caller.serial)?;
            if let Some(next) = &certificate.renewed_to {
                let renewed = self.load_certificate(&caller.identity, next)?;
                if renewed.issued.request_sha256 == Some(digest) {
                    trace.detail = Some(format!("repeat {next}"));
                    return self.enrolled(&caller.spiffe_id, &renewed.issued);
                }
                // A second attempt with a renewed certificate: its key may have been stolen.
                match self.revoke_identity(&identity, &caller.serial, now) {
                    Ok(()) => {
                        trace.detail = Some("second attempt".to_string());
                        return Err(refused("the certificate was already renewed"));
                    }
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            let reused = self.certificates_of(&caller.identity)?.iter().any(|known| {
                known.issued.idempotency_key.as_deref() == Some(idempotency_key)
                    && known.issued.request_sha256 != Some(digest)
            });
            if reused {
                return Err(conflict(
                    "the Idempotency-Key was used with another request",
                ));
            }
            let enabled = identity.status == IdentityStatus::Enrolled;
            if !certificate.renewable(now, ca.generation, identity.epoch, enabled) {
                return Err(refused("the certificate cannot be renewed"));
            }
            let leaf = self
                .ca
                .issue_component(&caller.identity, &csr.public_key, now)?;
            let issued = IssuedCertificate {
                at: now,
                identity: caller.identity.clone(),
                serial: leaf.serial,
                generation: leaf.generation,
                intermediate_version: leaf.intermediate_version,
                epoch: identity.epoch,
                not_before: leaf.not_before,
                not_after: leaf.not_after,
                call_key: Some(call_key.clone()),
                origin: CertificateOrigin::Renewal,
                renewed_from: Some(caller.serial.clone()),
                request_sha256: Some(digest),
                idempotency_key: Some(idempotency_key.to_string()),
                certificate: leaf.certificate,
            };
            let issued_records =
                [CertificateEvent::CertificateIssued(Box::new(issued.clone())).encode()];
            let renewed_records = [CertificateEvent::CertificateRenewed {
                at: now,
                new_serial: issued.serial.clone(),
            }
            .encode()];
            let prefix = certificate_prefix(&caller.identity);
            let written = self.store.transaction(&mut |writes| {
                writes.append(&format!("{prefix}{}", issued.serial), &issued_records, 0)?;
                writes.append(
                    &format!("{prefix}{}", caller.serial),
                    &renewed_records,
                    certificate.stream_version,
                )?;
                Ok(())
            });
            match written {
                Ok(()) => {
                    trace.detail = Some(format!("issued {}", issued.serial));
                    return self.enrolled(&caller.spiffe_id, &issued);
                }
                Err(ServiceError::Concurrency(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ServiceError::internal("TooManyRaces"))
    }

    /// A possible theft of a key (FR-015): the suspicious attempt and the urgent revocation of
    /// every valid certificate of the identity, in one transaction. Each revocation leaves its
    /// row, from Hécate.
    fn revoke_identity(
        &self,
        identity: &Identity,
        presented: &Serial,
        now: OffsetDateTime,
    ) -> Result<(), ServiceError> {
        let prefix = certificate_prefix(&identity.name);
        let revocations = self.urgent_revocations(
            &identity.name,
            RevokedBy::Hecate,
            RevocationReason::SuspiciousRenewal,
            now,
        )?;
        let suspicious = [IdentityEvent::SuspiciousRenewalDetected {
            at: now,
            serial: presented.clone(),
        }
        .encode()];
        self.store.transaction(&mut |writes| {
            writes.append(
                &identity_stream(&identity.name),
                &suspicious,
                identity.stream_version,
            )?;
            for (serial, version, records) in &revocations {
                writes.append(&format!("{prefix}{serial}"), records, *version)?;
            }
            Ok(())
        })?;
        for (serial, _, _) in &revocations {
            self.log(
                Operation::CertificateRevoke,
                Method::Internal,
                Some(HECATE),
                AccessResult::Success,
                Some(identity.name.as_str()),
                Some(serial.as_str()),
                "suspicious_renewal",
            )?;
        }
        Ok(())
    }

    /// The urgent revocation of every valid certificate of an identity, with the version of its
    /// stream; a passively revoked one is valid until it expires (FR-018), so it is there too.
    fn urgent_revocations(
        &self,
        name: &IdentityName,
        by: RevokedBy,
        reason: RevocationReason,
        now: OffsetDateTime,
    ) -> Result<Vec<(Serial, u64, [EventRecord; 1])>, ServiceError> {
        let mut revocations = Vec::new();
        for certificate in self.certificates_of(name)? {
            if certificate.state(now) != CertificateState::Valid {
                continue;
            }
            if let Some(event) = certificate.revoke(true, by, reason, now)? {
                revocations.push((
                    certificate.serial().clone(),
                    certificate.stream_version,
                    [event.encode()],
                ));
            }
        }
        Ok(revocations)
    }

    /// Runs a query of a component: served, it leaves no row; refused, also sealed, it does
    /// (FR-024).
    fn queried<T>(
        &self,
        operation: Operation,
        detail: &str,
        credentials: &Credentials,
        work: impl FnOnce(OffsetDateTime) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        self.unsealed_for(operation, Method::Identity)?;
        let now = self.now();
        let mut trace = Trace::default();
        let outcome = self
            .authenticate(credentials, now, &mut trace)
            .and_then(|_| work(now));
        if let Err(error) = &outcome {
            self.log(
                operation,
                Method::Identity,
                trace.subject.as_deref(),
                result_for(error),
                trace.identity.as_deref(),
                trace.certificate.as_deref(),
                detail,
            )?;
        }
        outcome
    }

    /// The secrets that a component resolves (US1, FR-001 to FR-006): authenticated by the
    /// certificate of its connection and a short call token, its own secrets and the ones that its
    /// Access Policy shares with it, read once per request, all or nothing. Sealed, Hécate
    /// answers `unavailable`, with its row; a refusal before the resolution leaves its own row,
    /// and the resolution leaves the rows of the secrets service (FR-015).
    pub fn resolve(
        &self,
        credentials: &Credentials,
        body: &Value,
    ) -> Result<Vec<Resolved>, ServiceError> {
        self.unsealed_for(Operation::SecretResolve, Method::Identity)?;
        let now = self.now();
        let mut trace = Trace::default();
        let checked = self
            .authenticate(credentials, now, &mut trace)
            .and_then(|caller| {
                let references = references_of(body)?;
                let policy = self.policies.policy_of(&caller.identity)?;
                Ok((caller, references, policy))
            });
        let (caller, references, policy) = match checked {
            Ok(checked) => checked,
            Err(error) => {
                self.log(
                    Operation::SecretResolve,
                    Method::Identity,
                    trace.subject.as_deref(),
                    result_for(&error),
                    trace.identity.as_deref(),
                    trace.certificate.as_deref(),
                    "resolve",
                )?;
                return Err(error);
            }
        };
        let subject = caller.spiffe_id.to_string();
        let identity = caller.identity.to_string();
        let certificate = caller.serial.to_string();
        let context = CallContext {
            subject: &subject,
            identity: Some(&identity),
            certificate: Some(&certificate),
        };
        self.secrets.resolve_for(
            &context,
            &|path: &SecretPath| policy.can_read(path),
            references,
        )
    }

    /// The least time that a `not_found` of the resolution takes (research R2).
    pub fn refusal_floor(&self) -> std::time::Duration {
        self.settings.refusal_floor
    }

    /// The aggregate status for a component with the status permission (US3, FR-011, FR-012).
    /// It also answers sealed: neither the authentication nor the policy nor the counts touch
    /// encrypted material. Served, it leaves no row; refused, it does (FR-015).
    pub fn status(&self, credentials: &Credentials) -> Result<AggregateStatus, ServiceError> {
        let now = self.now();
        let mut trace = Trace::default();
        let outcome = self
            .authenticate(credentials, now, &mut trace)
            .and_then(|caller| {
                if !self.policies.policy_of(&caller.identity)?.has_status() {
                    return Err(refused("the identity does not have the status permission"));
                }
                self.aggregate_status()
            });
        if let Err(error) = &outcome {
            self.log(
                Operation::StatusGet,
                Method::Identity,
                trace.subject.as_deref(),
                result_for(error),
                trace.identity.as_deref(),
                trace.certificate.as_deref(),
                "get",
            )?;
        }
        outcome
    }

    /// The counts of the aggregate status, from the same sources as the `status` of the local
    /// channel.
    fn aggregate_status(&self) -> Result<AggregateStatus, ServiceError> {
        let secrets = self.secrets.expiry_counts(None)?;
        let identities = self.counts()?;
        Ok(AggregateStatus {
            sealed: self.sealing.unsealed_epoch().is_err(),
            secrets_expired: secrets.secrets_expired,
            secrets_expiring_soon: secrets.secrets_expiring_soon,
            secrets_due_rotation: secrets.secrets_due_rotation,
            identities_expiring_24h: identities.identities_expiring_24h,
            suspicious_renewals_24h: identities.suspicious_renewals_24h,
            intermediate_days_left: self
                .ca
                .status()?
                .and_then(|summary| summary.intermediate_days_left),
            enrollment_throttled: self.enrollment_throttled()?,
        })
    }

    /// Every registered call key, with whether it signs new calls now.
    fn published(&self, now: OffsetDateTime) -> Result<Vec<PublishedKey>, ServiceError> {
        let ca = self.authority()?;
        let mut keys = Vec::new();
        for identity in self.identities()? {
            let enabled = identity.status == IdentityStatus::Enrolled;
            let spiffe_id = SpiffeId::new(ca.trust_domain.clone(), identity.name.clone());
            for certificate in self.certificates_of(&identity.name)? {
                let Some(key) = certificate.issued.call_key.clone() else {
                    continue;
                };
                keys.push(PublishedKey {
                    key,
                    identity: spiffe_id.clone(),
                    active: certificate.call_key_active(now, ca.generation, enabled),
                    not_after: certificate.issued.not_after,
                });
            }
        }
        Ok(keys)
    }

    /// The list of the keys valid for new calls: a valid certificate and its identity enabled;
    /// a key that is not there does not sign new calls (FR-017).
    pub fn keys(&self, credentials: &Credentials) -> Result<PublishedKeys, ServiceError> {
        self.queried(Operation::KeyList, "list", credentials, |now| {
            let keys = self
                .published(now)?
                .into_iter()
                .filter(|key| key.active)
                .collect();
            Ok(PublishedKeys {
                keys,
                expires_at: now + PUBLISHED_FOR,
            })
        })
    }

    /// Any registered key, also an old one, to verify past signatures.
    pub fn key(&self, credentials: &Credentials, kid: &str) -> Result<PublishedKey, ServiceError> {
        self.queried(Operation::KeyGet, "get", credentials, |now| {
            if !is_kid(kid) {
                return Err(invalid(
                    "kid must be a JWK thumbprint of 43 base64url characters",
                ));
            }
            let matching: Vec<PublishedKey> = self
                .published(now)?
                .into_iter()
                .filter(|key| key.key.kid() == kid)
                .collect();
            matching
                .iter()
                .find(|key| key.active)
                .or_else(|| matching.iter().max_by_key(|key| key.not_after))
                .cloned()
                .ok_or_else(|| HecateError::not_found("key not found").into())
        })
    }

    /// What `GET /v1/capabilities` needs: an authenticated caller.
    pub fn capabilities(&self, credentials: &Credentials) -> Result<(), ServiceError> {
        self.queried(Operation::CapabilitiesGet, "get", credentials, |_| Ok(()))
    }

    /// The CRLs for the components (FR-020): one per intermediate of the current CA that is not
    /// retired, with the urgent revocations of the certificates it issued that have not expired,
    /// Hécate's included; valid 300 s.
    pub fn revocations(&self, credentials: &Credentials) -> Result<Revocations, ServiceError> {
        self.queried(Operation::RevocationList, "list", credentials, |now| {
            let ca = self.authority()?;
            let mut revoked = Vec::new();
            for stream in self.store.streams(CERTIFICATE_PREFIX)? {
                let certificate = decode_certificate(&self.store.load(&stream)?)?;
                let Some(revocation) = certificate
                    .revocation
                    .as_ref()
                    .filter(|revocation| revocation.urgent)
                else {
                    continue;
                };
                if now >= certificate.issued.not_after
                    || certificate.issued.generation != ca.generation
                {
                    continue;
                }
                revoked.push((
                    certificate.issued.intermediate_version,
                    RevokedCertificate {
                        serial: certificate.serial().clone(),
                        revoked_at: revocation.at,
                        reason: revocation.reason,
                    },
                ));
            }
            revoked.sort_by(|(_, a), (_, b)| {
                (a.revoked_at, a.serial.as_str()).cmp(&(b.revoked_at, b.serial.as_str()))
            });
            let (crls, expires_at) = self.ca.crls(&revoked, now, PUBLISHED_FOR)?;
            Ok(Revocations { crls, expires_at })
        })
    }

    /// The identities that have enrolled, with their status and the end of their most recent
    /// certificate (FR-022).
    pub fn identities_list(&self) -> Result<Vec<IdentitySummary>, ServiceError> {
        self.audited(Operation::IdentityList, "list", |_| {
            self.sealing.unsealed_epoch()?;
            let mut summaries = Vec::new();
            for identity in self.identities()? {
                if identity.status == IdentityStatus::NotEnrolled {
                    continue;
                }
                let latest = self
                    .certificates_of(&identity.name)?
                    .into_iter()
                    .max_by_key(|certificate| certificate.issued.at)
                    .ok_or_else(|| ServiceError::internal("MissingCertificate"))?;
                summaries.push(IdentitySummary {
                    identity: identity.name,
                    status: identity.status,
                    certificate_not_after: latest.issued.not_after,
                });
            }
            summaries.sort_by(|a, b| a.identity.as_str().cmp(b.identity.as_str()));
            Ok(summaries)
        })
    }

    /// An enrolled identity with its certificates, without keys nor tokens (FR-022); one that
    /// only has tokens does not exist for the admin.
    pub fn identities_get(&self, identity: &str) -> Result<IdentityDetail, ServiceError> {
        self.audited(Operation::IdentityGet, "get", |trace| {
            trace.identity = IdentityName::parse(identity)
                .ok()
                .map(|name| name.to_string());
            self.sealing.unsealed_epoch()?;
            let name = IdentityName::parse(identity)?;
            let current = self.load_identity(&name)?;
            if current.status == IdentityStatus::NotEnrolled {
                return Err(HecateError::not_found("identity not found").into());
            }
            let generation = self.authority()?.generation;
            let now = self.now();
            let enabled = current.status == IdentityStatus::Enrolled;
            let mut certificates = self.certificates_of(&name)?;
            certificates.sort_by(|a, b| {
                (a.issued.at, a.serial().as_str()).cmp(&(b.issued.at, b.serial().as_str()))
            });
            Ok(IdentityDetail {
                identity: name,
                status: current.status,
                certificates: certificates
                    .iter()
                    .map(|certificate| CertificateSummary {
                        serial: certificate.serial().clone(),
                        not_before: certificate.issued.not_before,
                        not_after: certificate.issued.not_after,
                        state: certificate.state(now),
                        renewable: certificate.renewable(now, generation, current.epoch, enabled),
                    })
                    .collect(),
            })
        })
    }

    /// Disables an identity (FR-019): its certificates no longer renew, its keys no longer sign
    /// new calls and its pending tokens no longer enroll. With `urgent`, its valid certificates
    /// are revoked urgently in the same transaction, each with its row.
    pub fn identities_disable(
        &self,
        identity: &str,
        urgent: bool,
    ) -> Result<Disabled, ServiceError> {
        let detail = if urgent { "disable urgent" } else { "disable" };
        self.audited(Operation::IdentityDisable, detail, |trace| {
            trace.identity = IdentityName::parse(identity)
                .ok()
                .map(|name| name.to_string());
            self.sealing.unsealed_epoch()?;
            let name = IdentityName::parse(identity)?;
            let now = self.now();
            for _ in 0..MAX_RACES {
                let current = self.load_identity(&name)?;
                let Some(event) = current.disable(urgent, now)? else {
                    return Ok(Disabled {
                        identity: name,
                        revoked: Vec::new(),
                    });
                };
                let revocations = if urgent {
                    self.urgent_revocations(&name, RevokedBy::Admin, RevocationReason::Admin, now)?
                } else {
                    Vec::new()
                };
                let records = [event.encode()];
                let prefix = certificate_prefix(&name);
                let written = self.store.transaction(&mut |writes| {
                    writes.append(&identity_stream(&name), &records, current.stream_version)?;
                    for (serial, version, records) in &revocations {
                        writes.append(&format!("{prefix}{serial}"), records, *version)?;
                    }
                    Ok(())
                });
                match written {
                    Ok(()) => {
                        let revoked: Vec<Serial> = revocations
                            .into_iter()
                            .map(|(serial, _, _)| serial)
                            .collect();
                        for serial in &revoked {
                            self.log(
                                Operation::CertificateRevoke,
                                Method::Local,
                                Some(ADMIN),
                                AccessResult::Success,
                                Some(name.as_str()),
                                Some(serial.as_str()),
                                detail,
                            )?;
                        }
                        return Ok(Disabled {
                            identity: name,
                            revoked,
                        });
                    }
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ServiceError::internal("TooManyRaces"))
        })
    }

    /// Enables a disabled identity (FR-019); it gets a certificate again only by enrolling with
    /// a new token, since the disable left its certificates without renewal.
    pub fn identities_enable(&self, identity: &str) -> Result<IdentityName, ServiceError> {
        self.audited(Operation::IdentityEnable, "enable", |trace| {
            trace.identity = IdentityName::parse(identity)
                .ok()
                .map(|name| name.to_string());
            self.sealing.unsealed_epoch()?;
            let name = IdentityName::parse(identity)?;
            let now = self.now();
            for _ in 0..MAX_RACES {
                let current = self.load_identity(&name)?;
                let Some(event) = current.enable(now)? else {
                    return Ok(name);
                };
                match self.append(
                    &identity_stream(&name),
                    &[event.encode()],
                    current.stream_version,
                ) {
                    Ok(()) => return Ok(name),
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ServiceError::internal("TooManyRaces"))
        })
    }

    /// The certificate with a serial, and its identity.
    fn certificate_by_serial(
        &self,
        serial: &Serial,
    ) -> Result<Option<(IdentityName, Certificate)>, ServiceError> {
        let suffix = format!("/{serial}");
        let Some(stream) = self
            .store
            .streams(CERTIFICATE_PREFIX)?
            .into_iter()
            .find(|stream| stream.ends_with(&suffix))
        else {
            return Ok(None);
        };
        let name =
            IdentityName::parse(&stream[CERTIFICATE_PREFIX.len()..stream.len() - suffix.len()])
                .map_err(|_| ServiceError::internal("InvalidStream"))?;
        Ok(Some((
            name,
            decode_certificate(&self.store.load(&stream)?)?,
        )))
    }

    /// Revokes a certificate (FR-018): passive, it only stops renewing; urgent, it stops being
    /// valid and enters the CRL until it expires. Hécate's current certificate only admits the
    /// urgent one, which replaces it at once with a new key (FR-012).
    pub fn certificates_revoke(&self, serial: &str, urgent: bool) -> Result<Revoked, ServiceError> {
        let detail = if urgent { "revoke urgent" } else { "revoke" };
        self.audited(Operation::CertificateRevoke, detail, |trace| {
            trace.certificate = Serial::parse(serial).ok().map(|serial| serial.to_string());
            self.sealing.unsealed_epoch()?;
            let serial = Serial::parse(serial)?;
            let now = self.now();
            for _ in 0..MAX_RACES {
                let (name, certificate) = self
                    .certificate_by_serial(&serial)?
                    .ok_or_else(|| HecateError::not_found("certificate not found"))?;
                trace.identity = Some(name.to_string());
                let revocation =
                    certificate.revoke(urgent, RevokedBy::Admin, RevocationReason::Admin, now)?;
                let Some(event) = revocation else {
                    return Ok(Revoked {
                        serial,
                        reissued: None,
                    });
                };
                let record = event.encode();
                if certificate.is_own() {
                    match self.ca.replace_own_certificate(
                        &serial,
                        &record,
                        certificate.stream_version,
                        now,
                    ) {
                        Ok(Some(reissued)) => {
                            return Ok(Revoked {
                                serial,
                                reissued: Some(reissued),
                            });
                        }
                        Ok(None) => {}
                        Err(ServiceError::Concurrency(_)) => continue,
                        Err(error) => return Err(error),
                    }
                }
                match self.append(
                    &format!("{}{serial}", certificate_prefix(&name)),
                    &[record],
                    certificate.stream_version,
                ) {
                    Ok(()) => {
                        return Ok(Revoked {
                            serial,
                            reissued: None,
                        });
                    }
                    Err(ServiceError::Concurrency(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ServiceError::internal("TooManyRaces"))
        })
    }

    /// The counts of `status`: the enabled identities whose most recent certificate expires
    /// within 24 hours, and the suspicious renewals of the last 24 hours (FR-025).
    pub fn counts(&self) -> Result<IdentityCounts, ServiceError> {
        let now = self.now();
        let generation = self.ca.authority()?.map_or(0, |ca| ca.generation);
        let mut counts = IdentityCounts {
            identities_expiring_24h: 0,
            suspicious_renewals_24h: 0,
        };
        for identity in self.identities()? {
            counts.suspicious_renewals_24h += identity
                .suspicious_renewals
                .iter()
                .filter(|at| **at > now - DAY)
                .count() as u64;
            if identity.status != IdentityStatus::Enrolled {
                continue;
            }
            let latest = self
                .certificates_of(&identity.name)?
                .into_iter()
                .max_by_key(|certificate| certificate.issued.at);
            // A certificate of a previous CA is valid for nothing (FR-021).
            if latest.is_some_and(|certificate| {
                certificate.is_valid(now, generation) && certificate.issued.not_after - now <= DAY
            }) {
                counts.identities_expiring_24h += 1;
            }
        }
        Ok(counts)
    }
}
