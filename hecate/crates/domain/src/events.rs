//! How domain events are stored, as in the Python version: the type name, the fields of the event
//! as a JSON object without `at` (instants in ISO 8601) and the instant. Events carry metadata
//! only, never a value or a key.

use data_encoding::{BASE64, HEXLOWER};
use serde_json::{Map, Value};
use time::OffsetDateTime;

use crate::identity::call_key::CallKey;
use crate::identity::names::{IdentityName, RootFingerprint, Serial, TokenId, TrustDomain};
use crate::instant::{isoformat, parse_iso};
use crate::secrets::path::SecretPath;

#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub kind: String,
    pub data: Map<String, Value>,
    pub at: OffsetDateTime,
}

/// A stored stream that the aggregates cannot read: an unknown type, other fields, a field of
/// another type, or a stream that does not start with its creation event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid event stream: {0}")]
pub struct InvalidEvents(pub &'static str);

/// The JSON fields of an event, from its name and value pairs.
pub fn event_data(fields: Vec<(&str, Value)>) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

pub fn instant_value(instant: Option<OffsetDateTime>) -> Value {
    instant.map_or(Value::Null, |instant| Value::from(isoformat(instant)))
}

/// The fields of one stored event, which must be exactly the expected ones.
pub struct Fields<'a>(&'a Map<String, Value>);

impl<'a> Fields<'a> {
    pub fn exactly(record: &'a EventRecord, names: &[&str]) -> Result<Self, InvalidEvents> {
        let expected = record.data.len() == names.len()
            && names.iter().all(|name| record.data.contains_key(*name));
        if expected {
            Ok(Self(&record.data))
        } else {
            Err(InvalidEvents("the event does not have the expected fields"))
        }
    }

    pub fn number(&self, name: &str) -> Result<u64, InvalidEvents> {
        self.0[name]
            .as_u64()
            .ok_or(InvalidEvents("an event field is not a number"))
    }

    pub fn text(&self, name: &str) -> Result<String, InvalidEvents> {
        self.0[name]
            .as_str()
            .map(str::to_string)
            .ok_or(InvalidEvents("an event field is not a text"))
    }

    pub fn optional_text(&self, name: &str) -> Result<Option<String>, InvalidEvents> {
        match &self.0[name] {
            Value::Null => Ok(None),
            _ => self.text(name).map(Some),
        }
    }

    pub fn optional_instant(&self, name: &str) -> Result<Option<OffsetDateTime>, InvalidEvents> {
        self.optional_text(name)?
            .map(|text| {
                parse_iso(&text).map_err(|_| InvalidEvents("an event field is not an instant"))
            })
            .transpose()
    }
}

impl Fields<'_> {
    pub fn boolean(&self, name: &str) -> Result<bool, InvalidEvents> {
        self.0[name]
            .as_bool()
            .ok_or(InvalidEvents("an event field is not a boolean"))
    }

    pub fn instant(&self, name: &str) -> Result<OffsetDateTime, InvalidEvents> {
        parse_iso(&self.text(name)?).map_err(|_| InvalidEvents("an event field is not an instant"))
    }

    pub fn value(&self, name: &str) -> &Value {
        &self.0[name]
    }
}

// The events of the identities (data-model): the CA (stream `ca`), each identity
// (`identity/<name>`, with its enrollment tokens) and each certificate
// (`certificate/<identity>/<serial>`). None carries the secret of a token nor a private key.

fn invalid_field() -> InvalidEvents {
    InvalidEvents("an event field is not valid")
}

fn instant_text(instant: OffsetDateTime) -> Value {
    Value::from(isoformat(instant))
}

fn hash_text(hash: &[u8; 32]) -> Value {
    Value::from(HEXLOWER.encode(hash))
}

fn parse_hash(text: &str) -> Result<[u8; 32], InvalidEvents> {
    let lower = text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if text.len() != 64 || !lower {
        return Err(invalid_field());
    }
    HEXLOWER
        .decode(text.as_bytes())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(invalid_field)
}

macro_rules! event_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Result<Self, InvalidEvents> {
                match text {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(invalid_field()),
                }
            }
        }
    };
}

event_enum!(
    /// Why an intermediate key was destroyed.
    RetireReason {
        ExpiredCertificates => "expired_certificates",
        CaReplaced => "ca_replaced",
    }
);

event_enum!(
    /// How a certificate was issued; `self` is Hécate's own certificate.
    CertificateOrigin {
        Enrollment => "enrollment",
        Renewal => "renewal",
        OwnCertificate => "self",
    }
);

event_enum!(
    /// Who revoked a certificate.
    RevokedBy {
        Admin => "admin",
        Hecate => "hecate",
    }
);

event_enum!(
    /// Why a certificate was revoked.
    RevocationReason {
        Admin => "admin",
        SuspiciousRenewal => "suspicious_renewal",
    }
);

/// The events of the CA (`Key` aggregate of the CA, stream `ca`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaEvent {
    CaCreated {
        at: OffsetDateTime,
        generation: u64,
        trust_domain: TrustDomain,
        root_fingerprint: RootFingerprint,
        root_not_after: OffsetDateTime,
        intermediate_version: u64,
        intermediate_not_after: OffsetDateTime,
    },
    IntermediateRenewed {
        at: OffsetDateTime,
        generation: u64,
        intermediate_version: u64,
        intermediate_not_after: OffsetDateTime,
    },
    IntermediateRetired {
        at: OffsetDateTime,
        generation: u64,
        intermediate_version: u64,
        reason: RetireReason,
    },
    CaReplaced {
        at: OffsetDateTime,
        generation: u64,
        root_fingerprint: RootFingerprint,
        root_not_after: OffsetDateTime,
        intermediate_version: u64,
        intermediate_not_after: OffsetDateTime,
    },
}

impl CaEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CaCreated { .. } => "CaCreated",
            Self::IntermediateRenewed { .. } => "IntermediateRenewed",
            Self::IntermediateRetired { .. } => "IntermediateRetired",
            Self::CaReplaced { .. } => "CaReplaced",
        }
    }

    pub fn encode(&self) -> EventRecord {
        let (at, data) = match self {
            Self::CaCreated {
                at,
                generation,
                trust_domain,
                root_fingerprint,
                root_not_after,
                intermediate_version,
                intermediate_not_after,
            } => (
                at,
                event_data(vec![
                    ("generation", Value::from(*generation)),
                    ("trust_domain", Value::from(trust_domain.as_str())),
                    (
                        "root_fingerprint",
                        Value::from(root_fingerprint.to_string()),
                    ),
                    ("root_not_after", instant_text(*root_not_after)),
                    ("intermediate_version", Value::from(*intermediate_version)),
                    (
                        "intermediate_not_after",
                        instant_text(*intermediate_not_after),
                    ),
                ]),
            ),
            Self::IntermediateRenewed {
                at,
                generation,
                intermediate_version,
                intermediate_not_after,
            } => (
                at,
                event_data(vec![
                    ("generation", Value::from(*generation)),
                    ("intermediate_version", Value::from(*intermediate_version)),
                    (
                        "intermediate_not_after",
                        instant_text(*intermediate_not_after),
                    ),
                ]),
            ),
            Self::IntermediateRetired {
                at,
                generation,
                intermediate_version,
                reason,
            } => (
                at,
                event_data(vec![
                    ("generation", Value::from(*generation)),
                    ("intermediate_version", Value::from(*intermediate_version)),
                    ("reason", Value::from(reason.as_str())),
                ]),
            ),
            Self::CaReplaced {
                at,
                generation,
                root_fingerprint,
                root_not_after,
                intermediate_version,
                intermediate_not_after,
            } => (
                at,
                event_data(vec![
                    ("generation", Value::from(*generation)),
                    (
                        "root_fingerprint",
                        Value::from(root_fingerprint.to_string()),
                    ),
                    ("root_not_after", instant_text(*root_not_after)),
                    ("intermediate_version", Value::from(*intermediate_version)),
                    (
                        "intermediate_not_after",
                        instant_text(*intermediate_not_after),
                    ),
                ]),
            ),
        };
        EventRecord {
            kind: self.kind().to_string(),
            data,
            at: *at,
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        let at = record.at;
        let fingerprint = |fields: &Fields<'_>| {
            RootFingerprint::parse(&fields.text("root_fingerprint")?).map_err(|_| invalid_field())
        };
        match record.kind.as_str() {
            "CaCreated" => {
                let fields = Fields::exactly(
                    record,
                    &[
                        "generation",
                        "trust_domain",
                        "root_fingerprint",
                        "root_not_after",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                )?;
                Ok(Self::CaCreated {
                    at,
                    generation: fields.number("generation")?,
                    trust_domain: TrustDomain::parse(&fields.text("trust_domain")?)
                        .map_err(|_| invalid_field())?,
                    root_fingerprint: fingerprint(&fields)?,
                    root_not_after: fields.instant("root_not_after")?,
                    intermediate_version: fields.number("intermediate_version")?,
                    intermediate_not_after: fields.instant("intermediate_not_after")?,
                })
            }
            "IntermediateRenewed" => {
                let fields = Fields::exactly(
                    record,
                    &[
                        "generation",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                )?;
                Ok(Self::IntermediateRenewed {
                    at,
                    generation: fields.number("generation")?,
                    intermediate_version: fields.number("intermediate_version")?,
                    intermediate_not_after: fields.instant("intermediate_not_after")?,
                })
            }
            "IntermediateRetired" => {
                let fields =
                    Fields::exactly(record, &["generation", "intermediate_version", "reason"])?;
                Ok(Self::IntermediateRetired {
                    at,
                    generation: fields.number("generation")?,
                    intermediate_version: fields.number("intermediate_version")?,
                    reason: RetireReason::parse(&fields.text("reason")?)?,
                })
            }
            "CaReplaced" => {
                let fields = Fields::exactly(
                    record,
                    &[
                        "generation",
                        "root_fingerprint",
                        "root_not_after",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                )?;
                Ok(Self::CaReplaced {
                    at,
                    generation: fields.number("generation")?,
                    root_fingerprint: fingerprint(&fields)?,
                    root_not_after: fields.instant("root_not_after")?,
                    intermediate_version: fields.number("intermediate_version")?,
                    intermediate_not_after: fields.instant("intermediate_not_after")?,
                })
            }
            _ => Err(InvalidEvents("unknown CA event")),
        }
    }
}

/// The events of an identity and its enrollment tokens (stream `identity/<name>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityEvent {
    EnrollmentTokenCreated {
        at: OffsetDateTime,
        token_id: TokenId,
        secret_sha256: [u8; 32],
        generation: u64,
        expires_at: OffsetDateTime,
    },
    EnrollmentTokenAnnulled {
        at: OffsetDateTime,
        token_id: TokenId,
    },
    IdentityEnrolled {
        at: OffsetDateTime,
        token_id: TokenId,
        serial: Serial,
    },
    IdentityDisabled {
        at: OffsetDateTime,
        urgent: bool,
    },
    IdentityEnabled {
        at: OffsetDateTime,
    },
    SuspiciousRenewalDetected {
        at: OffsetDateTime,
        serial: Serial,
    },
}

impl IdentityEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::EnrollmentTokenCreated { .. } => "EnrollmentTokenCreated",
            Self::EnrollmentTokenAnnulled { .. } => "EnrollmentTokenAnnulled",
            Self::IdentityEnrolled { .. } => "IdentityEnrolled",
            Self::IdentityDisabled { .. } => "IdentityDisabled",
            Self::IdentityEnabled { .. } => "IdentityEnabled",
            Self::SuspiciousRenewalDetected { .. } => "SuspiciousRenewalDetected",
        }
    }

    pub fn encode(&self) -> EventRecord {
        let (at, data) = match self {
            Self::EnrollmentTokenCreated {
                at,
                token_id,
                secret_sha256,
                generation,
                expires_at,
            } => (
                at,
                event_data(vec![
                    ("token_id", Value::from(token_id.as_str())),
                    ("secret_sha256", hash_text(secret_sha256)),
                    ("generation", Value::from(*generation)),
                    ("expires_at", instant_text(*expires_at)),
                ]),
            ),
            Self::EnrollmentTokenAnnulled { at, token_id } => (
                at,
                event_data(vec![("token_id", Value::from(token_id.as_str()))]),
            ),
            Self::IdentityEnrolled {
                at,
                token_id,
                serial,
            } => (
                at,
                event_data(vec![
                    ("token_id", Value::from(token_id.as_str())),
                    ("serial", Value::from(serial.as_str())),
                ]),
            ),
            Self::IdentityDisabled { at, urgent } => {
                (at, event_data(vec![("urgent", Value::from(*urgent))]))
            }
            Self::IdentityEnabled { at } => (at, Map::new()),
            Self::SuspiciousRenewalDetected { at, serial } => (
                at,
                event_data(vec![("serial", Value::from(serial.as_str()))]),
            ),
        };
        EventRecord {
            kind: self.kind().to_string(),
            data,
            at: *at,
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        let at = record.at;
        let token_id = |fields: &Fields<'_>| {
            TokenId::parse(&fields.text("token_id")?).map_err(|_| invalid_field())
        };
        let serial = |fields: &Fields<'_>| {
            Serial::parse(&fields.text("serial")?).map_err(|_| invalid_field())
        };
        match record.kind.as_str() {
            "EnrollmentTokenCreated" => {
                let fields = Fields::exactly(
                    record,
                    &["token_id", "secret_sha256", "generation", "expires_at"],
                )?;
                Ok(Self::EnrollmentTokenCreated {
                    at,
                    token_id: token_id(&fields)?,
                    secret_sha256: parse_hash(&fields.text("secret_sha256")?)?,
                    generation: fields.number("generation")?,
                    expires_at: fields.instant("expires_at")?,
                })
            }
            "EnrollmentTokenAnnulled" => {
                let fields = Fields::exactly(record, &["token_id"])?;
                Ok(Self::EnrollmentTokenAnnulled {
                    at,
                    token_id: token_id(&fields)?,
                })
            }
            "IdentityEnrolled" => {
                let fields = Fields::exactly(record, &["token_id", "serial"])?;
                Ok(Self::IdentityEnrolled {
                    at,
                    token_id: token_id(&fields)?,
                    serial: serial(&fields)?,
                })
            }
            "IdentityDisabled" => {
                let fields = Fields::exactly(record, &["urgent"])?;
                Ok(Self::IdentityDisabled {
                    at,
                    urgent: fields.boolean("urgent")?,
                })
            }
            "IdentityEnabled" => {
                Fields::exactly(record, &[])?;
                Ok(Self::IdentityEnabled { at })
            }
            "SuspiciousRenewalDetected" => {
                let fields = Fields::exactly(record, &["serial"])?;
                Ok(Self::SuspiciousRenewalDetected {
                    at,
                    serial: serial(&fields)?,
                })
            }
            _ => Err(InvalidEvents("unknown identity event")),
        }
    }
}

/// The issuance of a certificate, with its call-signing key (none for Hécate's own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedCertificate {
    pub at: OffsetDateTime,
    pub identity: IdentityName,
    pub serial: Serial,
    pub generation: u64,
    pub intermediate_version: u64,
    pub epoch: u64,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    pub call_key: Option<CallKey>,
    pub origin: CertificateOrigin,
    pub renewed_from: Option<Serial>,
    pub request_sha256: Option<[u8; 32]>,
    pub idempotency_key: Option<String>,
    /// The DER of the certificate.
    pub certificate: Vec<u8>,
}

/// The events of a certificate (stream `certificate/<identity>/<serial>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateEvent {
    CertificateIssued(Box<IssuedCertificate>),
    CertificateRenewed {
        at: OffsetDateTime,
        new_serial: Serial,
    },
    CertificateRevoked {
        at: OffsetDateTime,
        urgent: bool,
        by: RevokedBy,
        reason: RevocationReason,
    },
}

const ISSUED_FIELDS: [&str; 13] = [
    "identity",
    "serial",
    "generation",
    "intermediate_version",
    "epoch",
    "not_before",
    "not_after",
    "call_key",
    "origin",
    "renewed_from",
    "request_sha256",
    "idempotency_key",
    "certificate",
];

fn call_key_value(call_key: Option<&CallKey>) -> Value {
    call_key.map_or(Value::Null, |key| {
        Value::Object(event_data(vec![
            ("x", Value::from(key.x())),
            ("kid", Value::from(key.kid())),
        ]))
    })
}

/// The call-signing key of an issuance; its `kid` must be the one of its `x`.
fn parse_call_key(value: &Value) -> Result<Option<CallKey>, InvalidEvents> {
    match value {
        Value::Null => Ok(None),
        Value::Object(members) if members.len() == 2 => {
            let text = |name: &str| {
                members
                    .get(name)
                    .and_then(Value::as_str)
                    .ok_or_else(invalid_field)
            };
            let key = CallKey::from_x(text("x")?).map_err(|_| invalid_field())?;
            if key.kid() == text("kid")? {
                Ok(Some(key))
            } else {
                Err(invalid_field())
            }
        }
        _ => Err(invalid_field()),
    }
}

impl CertificateEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CertificateIssued(_) => "CertificateIssued",
            Self::CertificateRenewed { .. } => "CertificateRenewed",
            Self::CertificateRevoked { .. } => "CertificateRevoked",
        }
    }

    pub fn at(&self) -> OffsetDateTime {
        match self {
            Self::CertificateIssued(issued) => issued.at,
            Self::CertificateRenewed { at, .. } | Self::CertificateRevoked { at, .. } => *at,
        }
    }

    pub fn encode(&self) -> EventRecord {
        let data = match self {
            Self::CertificateIssued(issued) => event_data(vec![
                ("identity", Value::from(issued.identity.as_str())),
                ("serial", Value::from(issued.serial.as_str())),
                ("generation", Value::from(issued.generation)),
                (
                    "intermediate_version",
                    Value::from(issued.intermediate_version),
                ),
                ("epoch", Value::from(issued.epoch)),
                ("not_before", instant_text(issued.not_before)),
                ("not_after", instant_text(issued.not_after)),
                ("call_key", call_key_value(issued.call_key.as_ref())),
                ("origin", Value::from(issued.origin.as_str())),
                (
                    "renewed_from",
                    issued
                        .renewed_from
                        .as_ref()
                        .map_or(Value::Null, |serial| Value::from(serial.as_str())),
                ),
                (
                    "request_sha256",
                    issued
                        .request_sha256
                        .as_ref()
                        .map_or(Value::Null, hash_text),
                ),
                (
                    "idempotency_key",
                    issued
                        .idempotency_key
                        .as_deref()
                        .map_or(Value::Null, Value::from),
                ),
                (
                    "certificate",
                    Value::from(BASE64.encode(&issued.certificate)),
                ),
            ]),
            Self::CertificateRenewed { new_serial, .. } => {
                event_data(vec![("new_serial", Value::from(new_serial.as_str()))])
            }
            Self::CertificateRevoked {
                urgent, by, reason, ..
            } => event_data(vec![
                ("urgent", Value::from(*urgent)),
                ("by", Value::from(by.as_str())),
                ("reason", Value::from(reason.as_str())),
            ]),
        };
        EventRecord {
            kind: self.kind().to_string(),
            data,
            at: self.at(),
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        let at = record.at;
        let serial = |text: String| Serial::parse(&text).map_err(|_| invalid_field());
        match record.kind.as_str() {
            "CertificateIssued" => {
                let fields = Fields::exactly(record, &ISSUED_FIELDS)?;
                Ok(Self::CertificateIssued(Box::new(IssuedCertificate {
                    at,
                    identity: IdentityName::parse(&fields.text("identity")?)
                        .map_err(|_| invalid_field())?,
                    serial: serial(fields.text("serial")?)?,
                    generation: fields.number("generation")?,
                    intermediate_version: fields.number("intermediate_version")?,
                    epoch: fields.number("epoch")?,
                    not_before: fields.instant("not_before")?,
                    not_after: fields.instant("not_after")?,
                    call_key: parse_call_key(fields.value("call_key"))?,
                    origin: CertificateOrigin::parse(&fields.text("origin")?)?,
                    renewed_from: fields
                        .optional_text("renewed_from")?
                        .map(serial)
                        .transpose()?,
                    request_sha256: fields
                        .optional_text("request_sha256")?
                        .map(|text| parse_hash(&text))
                        .transpose()?,
                    idempotency_key: fields.optional_text("idempotency_key")?,
                    certificate: BASE64
                        .decode(fields.text("certificate")?.as_bytes())
                        .map_err(|_| invalid_field())?,
                })))
            }
            "CertificateRenewed" => {
                let fields = Fields::exactly(record, &["new_serial"])?;
                Ok(Self::CertificateRenewed {
                    at,
                    new_serial: serial(fields.text("new_serial")?)?,
                })
            }
            "CertificateRevoked" => {
                let fields = Fields::exactly(record, &["urgent", "by", "reason"])?;
                Ok(Self::CertificateRevoked {
                    at,
                    urgent: fields.boolean("urgent")?,
                    by: RevokedBy::parse(&fields.text("by")?)?,
                    reason: RevocationReason::parse(&fields.text("reason")?)?,
                })
            }
            _ => Err(InvalidEvents("unknown certificate event")),
        }
    }
}

// The events of the Access Policy (data-model): one stream per identity, `policy/<identity>`,
// that exists only once the admin has changed something in it. None carries a value.

event_enum!(
    /// Who changed an Access Policy: the admin, from the local channel.
    ChangedBy {
        Admin => "admin",
    }
);

/// The events of the Access Policy of an identity (stream `policy/<identity>`): the reads of
/// another identity's secret and the status permission, granted and revoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyEvent {
    ReadGranted {
        at: OffsetDateTime,
        secret: SecretPath,
        by: ChangedBy,
    },
    ReadRevoked {
        at: OffsetDateTime,
        secret: SecretPath,
        by: ChangedBy,
    },
    StatusGranted {
        at: OffsetDateTime,
        by: ChangedBy,
    },
    StatusRevoked {
        at: OffsetDateTime,
        by: ChangedBy,
    },
}

impl PolicyEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ReadGranted { .. } => "ReadGranted",
            Self::ReadRevoked { .. } => "ReadRevoked",
            Self::StatusGranted { .. } => "StatusGranted",
            Self::StatusRevoked { .. } => "StatusRevoked",
        }
    }

    pub fn at(&self) -> OffsetDateTime {
        match self {
            Self::ReadGranted { at, .. }
            | Self::ReadRevoked { at, .. }
            | Self::StatusGranted { at, .. }
            | Self::StatusRevoked { at, .. } => *at,
        }
    }

    pub fn by(&self) -> ChangedBy {
        match self {
            Self::ReadGranted { by, .. }
            | Self::ReadRevoked { by, .. }
            | Self::StatusGranted { by, .. }
            | Self::StatusRevoked { by, .. } => *by,
        }
    }

    /// The secret of a read; none for the status permission.
    pub fn secret(&self) -> Option<&SecretPath> {
        match self {
            Self::ReadGranted { secret, .. } | Self::ReadRevoked { secret, .. } => Some(secret),
            Self::StatusGranted { .. } | Self::StatusRevoked { .. } => None,
        }
    }

    pub fn encode(&self) -> EventRecord {
        let mut data = event_data(vec![("by", Value::from(self.by().as_str()))]);
        if let Some(secret) = self.secret() {
            data.insert("secret".to_string(), Value::from(secret.to_string()));
        }
        EventRecord {
            kind: self.kind().to_string(),
            data,
            at: self.at(),
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        let at = record.at;
        let by = |fields: &Fields<'_>| ChangedBy::parse(&fields.text("by")?);
        let secret = |fields: &Fields<'_>| {
            SecretPath::parse(&fields.text("secret")?).map_err(|_| invalid_field())
        };
        match record.kind.as_str() {
            "ReadGranted" => {
                let fields = Fields::exactly(record, &["secret", "by"])?;
                Ok(Self::ReadGranted {
                    at,
                    secret: secret(&fields)?,
                    by: by(&fields)?,
                })
            }
            "ReadRevoked" => {
                let fields = Fields::exactly(record, &["secret", "by"])?;
                Ok(Self::ReadRevoked {
                    at,
                    secret: secret(&fields)?,
                    by: by(&fields)?,
                })
            }
            "StatusGranted" => {
                let fields = Fields::exactly(record, &["by"])?;
                Ok(Self::StatusGranted {
                    at,
                    by: by(&fields)?,
                })
            }
            "StatusRevoked" => {
                let fields = Fields::exactly(record, &["by"])?;
                Ok(Self::StatusRevoked {
                    at,
                    by: by(&fields)?,
                })
            }
            _ => Err(InvalidEvents("unknown policy event")),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::instant::{isoformat, parse_iso};
    use crate::keys::key::KeyEvent;
    use crate::secrets::secret::SecretEvent;

    #[test]
    fn events_are_read_and_written_with_the_same_json_content() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/events.json"
        );
        let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 10);
        let mut kinds = Vec::new();
        for case in cases {
            let Value::Object(data) = serde_json::from_str(case["data"].as_str().unwrap()).unwrap()
            else {
                panic!("event data is an object");
            };
            let record = EventRecord {
                kind: case["type"].as_str().unwrap().to_string(),
                data,
                at: parse_iso(case["at"].as_str().unwrap()).unwrap(),
            };
            let rewritten = if record.kind == "ScopeKeyCreated" {
                KeyEvent::decode(&record).unwrap().encode()
            } else {
                SecretEvent::decode(&record).unwrap().encode()
            };
            assert_eq!(rewritten, record);
            assert_eq!(isoformat(rewritten.at), case["at"].as_str().unwrap());
            kinds.push(record.kind);
        }
        kinds.sort();
        kinds.dedup();
        assert_eq!(
            kinds.len(),
            8,
            "every event type of Secret and Key: {kinds:?}"
        );
    }

    #[test]
    fn an_event_with_other_fields_or_types_is_corrupt() {
        let at = parse_iso("2026-11-08T12:30:00+00:00").unwrap();
        let record = |kind: &str, data: Value| EventRecord {
            kind: kind.to_string(),
            data: data.as_object().unwrap().clone(),
            at,
        };
        assert!(
            SecretEvent::decode(&record("VersionRevoked", serde_json::json!({"version": 1})))
                .is_ok()
        );
        for bad in [
            record("VersionRevoked", serde_json::json!({})),
            record("VersionRevoked", serde_json::json!({"version": "1"})),
            record(
                "VersionRevoked",
                serde_json::json!({"version": 1, "value": "x"}),
            ),
            record("Unknown", serde_json::json!({"version": 1})),
        ] {
            assert!(SecretEvent::decode(&bad).is_err(), "{bad:?}");
        }
    }

    mod identities {
        use time::macros::datetime;

        use super::super::*;
        use crate::identity::call_key::CallKey;
        use crate::identity::names::{IdentityName, RootFingerprint, Serial, TokenId, TrustDomain};

        const AT: OffsetDateTime = datetime!(2026-11-09 12:00:00.123456 UTC);

        fn serial(text: &str) -> Serial {
            Serial::parse(text).unwrap()
        }

        fn fields(record: &EventRecord) -> Vec<String> {
            let mut names: Vec<String> = record.data.keys().cloned().collect();
            names.sort();
            names
        }

        fn sorted(names: &[&str]) -> Vec<String> {
            let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
            names.sort();
            names
        }

        fn ca_events() -> Vec<(CaEvent, Vec<&'static str>)> {
            let fingerprint = RootFingerprint::of_der(b"root");
            vec![
                (
                    CaEvent::CaCreated {
                        at: AT,
                        generation: 1,
                        trust_domain: TrustDomain::parse("cerbero.test").unwrap(),
                        root_fingerprint: fingerprint,
                        root_not_after: datetime!(2036-11-09 12:00 UTC),
                        intermediate_version: 1,
                        intermediate_not_after: datetime!(2027-11-09 12:00 UTC),
                    },
                    vec![
                        "generation",
                        "trust_domain",
                        "root_fingerprint",
                        "root_not_after",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                ),
                (
                    CaEvent::IntermediateRenewed {
                        at: AT,
                        generation: 1,
                        intermediate_version: 2,
                        intermediate_not_after: datetime!(2028-11-09 12:00 UTC),
                    },
                    vec![
                        "generation",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                ),
                (
                    CaEvent::IntermediateRetired {
                        at: AT,
                        generation: 1,
                        intermediate_version: 1,
                        reason: RetireReason::ExpiredCertificates,
                    },
                    vec!["generation", "intermediate_version", "reason"],
                ),
                (
                    CaEvent::IntermediateRetired {
                        at: AT,
                        generation: 1,
                        intermediate_version: 2,
                        reason: RetireReason::CaReplaced,
                    },
                    vec!["generation", "intermediate_version", "reason"],
                ),
                (
                    CaEvent::CaReplaced {
                        at: AT,
                        generation: 2,
                        root_fingerprint: RootFingerprint::of_der(b"new root"),
                        root_not_after: datetime!(2036-11-10 12:00 UTC),
                        intermediate_version: 1,
                        intermediate_not_after: datetime!(2027-11-10 12:00 UTC),
                    },
                    vec![
                        "generation",
                        "root_fingerprint",
                        "root_not_after",
                        "intermediate_version",
                        "intermediate_not_after",
                    ],
                ),
            ]
        }

        fn identity_events() -> Vec<(IdentityEvent, Vec<&'static str>)> {
            let token_id = TokenId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8]);
            vec![
                (
                    IdentityEvent::EnrollmentTokenCreated {
                        at: AT,
                        token_id: token_id.clone(),
                        secret_sha256: [9; 32],
                        generation: 1,
                        expires_at: datetime!(2026-11-09 12:15 UTC),
                    },
                    vec!["token_id", "secret_sha256", "generation", "expires_at"],
                ),
                (
                    IdentityEvent::EnrollmentTokenAnnulled {
                        at: AT,
                        token_id: token_id.clone(),
                    },
                    vec!["token_id"],
                ),
                (
                    IdentityEvent::IdentityEnrolled {
                        at: AT,
                        token_id,
                        serial: serial("5f1c"),
                    },
                    vec!["token_id", "serial"],
                ),
                (
                    IdentityEvent::IdentityDisabled {
                        at: AT,
                        urgent: true,
                    },
                    vec!["urgent"],
                ),
                (IdentityEvent::IdentityEnabled { at: AT }, vec![]),
                (
                    IdentityEvent::SuspiciousRenewalDetected {
                        at: AT,
                        serial: serial("abc"),
                    },
                    vec!["serial"],
                ),
            ]
        }

        fn issued(origin: CertificateOrigin, call_key: Option<CallKey>) -> CertificateEvent {
            let own = origin == CertificateOrigin::OwnCertificate;
            CertificateEvent::CertificateIssued(Box::new(IssuedCertificate {
                at: AT,
                identity: IdentityName::parse(if own { "hecate" } else { "hermes" }).unwrap(),
                serial: serial("5f1c"),
                generation: 1,
                intermediate_version: 1,
                epoch: 2,
                not_before: datetime!(2026-11-09 11:59 UTC),
                not_after: datetime!(2026-11-12 12:00 UTC),
                call_key,
                origin,
                renewed_from: (origin == CertificateOrigin::Renewal).then(|| serial("4e0b")),
                request_sha256: (!own).then_some([3; 32]),
                idempotency_key: (!own).then(|| "key-1".to_string()),
                certificate: vec![0x30, 0x82, 0x01, 0x0a],
            }))
        }

        fn certificate_events() -> Vec<(CertificateEvent, Vec<&'static str>)> {
            let issued_fields = vec![
                "identity",
                "serial",
                "generation",
                "intermediate_version",
                "epoch",
                "not_before",
                "not_after",
                "call_key",
                "origin",
                "renewed_from",
                "request_sha256",
                "idempotency_key",
                "certificate",
            ];
            let key = CallKey::from_bytes(&[5; 32]).unwrap();
            vec![
                (
                    issued(CertificateOrigin::Enrollment, Some(key.clone())),
                    issued_fields.clone(),
                ),
                (
                    issued(CertificateOrigin::Renewal, Some(key)),
                    issued_fields.clone(),
                ),
                (
                    issued(CertificateOrigin::OwnCertificate, None),
                    issued_fields,
                ),
                (
                    CertificateEvent::CertificateRenewed {
                        at: AT,
                        new_serial: serial("6a2d"),
                    },
                    vec!["new_serial"],
                ),
                (
                    CertificateEvent::CertificateRevoked {
                        at: AT,
                        urgent: true,
                        by: RevokedBy::Hecate,
                        reason: RevocationReason::SuspiciousRenewal,
                    },
                    vec!["urgent", "by", "reason"],
                ),
                (
                    CertificateEvent::CertificateRevoked {
                        at: AT,
                        urgent: false,
                        by: RevokedBy::Admin,
                        reason: RevocationReason::Admin,
                    },
                    vec!["urgent", "by", "reason"],
                ),
            ]
        }

        #[test]
        fn ca_events_round_trip_with_exactly_their_fields() {
            for (event, expected) in ca_events() {
                let record = event.encode();
                assert_eq!(fields(&record), sorted(&expected), "{event:?}");
                assert_eq!(CaEvent::decode(&record).unwrap(), event);
            }
        }

        #[test]
        fn identity_events_round_trip_with_exactly_their_fields() {
            for (event, expected) in identity_events() {
                let record = event.encode();
                assert_eq!(fields(&record), sorted(&expected), "{event:?}");
                assert_eq!(IdentityEvent::decode(&record).unwrap(), event);
            }
        }

        #[test]
        fn certificate_events_round_trip_with_exactly_their_fields() {
            for (event, expected) in certificate_events() {
                let record = event.encode();
                assert_eq!(fields(&record), sorted(&expected), "{event:?}");
                assert_eq!(CertificateEvent::decode(&record).unwrap(), event);
            }
        }

        #[test]
        fn the_token_event_keeps_only_the_hash_of_the_secret() {
            let (event, _) = identity_events().remove(0);
            let record = event.encode();
            assert_eq!(record.data["secret_sha256"], Value::from("09".repeat(32)));
            assert_eq!(record.kind, "EnrollmentTokenCreated");
        }

        #[test]
        fn hecates_own_certificates_carry_no_call_key() {
            let record = issued(CertificateOrigin::OwnCertificate, None).encode();
            assert_eq!(record.data["call_key"], Value::Null);
            assert_eq!(record.data["origin"], Value::from("self"));
            let key = CallKey::from_bytes(&[5; 32]).unwrap();
            let record = issued(CertificateOrigin::Enrollment, Some(key.clone())).encode();
            assert_eq!(
                record.data["call_key"],
                serde_json::json!({"x": key.x(), "kid": key.kid()})
            );
            assert_eq!(record.data["certificate"], Value::from("MIIBCg=="));
        }

        #[test]
        fn corrupt_identity_events_are_rejected() {
            let at = AT;
            let record = |kind: &str, data: Value| EventRecord {
                kind: kind.to_string(),
                data: data.as_object().unwrap().clone(),
                at,
            };
            assert!(CaEvent::decode(&record("Unknown", serde_json::json!({}))).is_err());
            assert!(IdentityEvent::decode(&record("CaCreated", serde_json::json!({}))).is_err());
            assert!(
                CertificateEvent::decode(&record("IdentityEnabled", serde_json::json!({})))
                    .is_err()
            );
            for bad in [
                record("IdentityDisabled", serde_json::json!({})),
                record("IdentityDisabled", serde_json::json!({"urgent": "yes"})),
                record(
                    "IdentityDisabled",
                    serde_json::json!({"urgent": true, "x": 1}),
                ),
                record(
                    "IdentityEnrolled",
                    serde_json::json!({"token_id": "0102030405060708", "serial": "0abc"}),
                ),
                record(
                    "IdentityEnrolled",
                    serde_json::json!({"token_id": "01020304", "serial": "abc"}),
                ),
            ] {
                assert!(IdentityEvent::decode(&bad).is_err(), "{bad:?}");
            }
            let mut tampered = issued(
                CertificateOrigin::Enrollment,
                Some(CallKey::from_bytes(&[5; 32]).unwrap()),
            )
            .encode();
            tampered.data.insert(
                "call_key".into(),
                serde_json::json!({"x": CallKey::from_bytes(&[5; 32]).unwrap().x(), "kid": "x".repeat(43)}),
            );
            assert!(CertificateEvent::decode(&tampered).is_err());
            let mut bad_origin = issued(CertificateOrigin::Enrollment, None).encode();
            bad_origin
                .data
                .insert("origin".into(), Value::from("other"));
            assert!(CertificateEvent::decode(&bad_origin).is_err());
            let mut bad_reason = CaEvent::IntermediateRetired {
                at,
                generation: 1,
                intermediate_version: 1,
                reason: RetireReason::CaReplaced,
            }
            .encode();
            bad_reason
                .data
                .insert("reason".into(), Value::from("other"));
            assert!(CaEvent::decode(&bad_reason).is_err());
        }
    }

    mod policies {
        use time::macros::datetime;

        use super::super::*;
        use crate::secrets::path::SecretPath;

        const AT: OffsetDateTime = datetime!(2026-11-11 10:00:00.123456 UTC);
        const SMTP: &str = "secret://hades/smtp/password";

        fn record(kind: &str, data: Value) -> EventRecord {
            EventRecord {
                kind: kind.to_string(),
                data: data.as_object().unwrap().clone(),
                at: AT,
            }
        }

        #[test]
        fn policy_events_round_trip_with_their_secret_and_who_changed_them() {
            let secret = SecretPath::parse(SMTP).unwrap();
            let by = ChangedBy::Admin;
            for (event, data) in [
                (
                    PolicyEvent::ReadGranted {
                        at: AT,
                        secret: secret.clone(),
                        by,
                    },
                    serde_json::json!({"secret": SMTP, "by": "admin"}),
                ),
                (
                    PolicyEvent::ReadRevoked { at: AT, secret, by },
                    serde_json::json!({"secret": SMTP, "by": "admin"}),
                ),
                (
                    PolicyEvent::StatusGranted { at: AT, by },
                    serde_json::json!({"by": "admin"}),
                ),
                (
                    PolicyEvent::StatusRevoked { at: AT, by },
                    serde_json::json!({"by": "admin"}),
                ),
            ] {
                let encoded = event.encode();
                assert_eq!(encoded, record(event.kind(), data), "{event:?}");
                assert_eq!(PolicyEvent::decode(&encoded).unwrap(), event);
                assert_eq!(event.at(), AT);
            }
        }

        #[test]
        fn corrupt_policy_events_are_rejected() {
            for bad in [
                record("ReadGranted", serde_json::json!({"secret": SMTP})),
                record(
                    "ReadGranted",
                    serde_json::json!({"secret": SMTP, "by": "hecate"}),
                ),
                record(
                    "ReadGranted",
                    serde_json::json!({"secret": format!("{SMTP}@3"), "by": "admin"}),
                ),
                record(
                    "ReadGranted",
                    serde_json::json!({"secret": "hades/smtp", "by": "admin"}),
                ),
                record(
                    "ReadRevoked",
                    serde_json::json!({"secret": SMTP, "by": "admin", "x": 1}),
                ),
                record("StatusGranted", serde_json::json!({"by": 1})),
                record("StatusRevoked", serde_json::json!({})),
                record("IdentityEnabled", serde_json::json!({})),
            ] {
                assert!(PolicyEvent::decode(&bad).is_err(), "{bad:?}");
            }
        }
    }
}
