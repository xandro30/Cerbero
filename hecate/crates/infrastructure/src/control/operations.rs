//! The operations of the local channel: the argument checks of the Python version, the data of
//! each reply and the access log row that a refused request leaves (research R8, R10), for the
//! sealing, the secrets and the CA.

use std::sync::Arc;

use hecate_application::access_policy::service::{AccessPolicyService, PolicyChange};
use hecate_application::ca::service::{CaService, CaSummary};
use hecate_application::identity::service::{IdentityCounts, IdentityService};
use hecate_application::sealing::service::{SealingService, Status, Unsealed};
use hecate_application::secrets::service::{
    Added, CreateOptions, Created, SecretDetails, SecretSummary, SecretsService,
};
use hecate_domain::access_policy::policy::AccessPolicy;
use hecate_domain::events::PolicyEvent;
use hecate_domain::identity::names::Serial;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{Method, Operation};
use hecate_domain::sealing::state::{SealState, UnsealMethod};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::control::server::{self, OpError, OpResult, RequestHandler, text_arg, texts_arg};

/// The operation and detail of the row that a refused request leaves, as `_AUDITED` in Python.
fn audited(op: &str) -> Option<(Operation, &'static str)> {
    Some(match op {
        "init.begin" => (Operation::Init, "begin"),
        "init.confirm" => (Operation::Init, "confirm"),
        "init.abort" => (Operation::Init, "abort"),
        "unseal" => (Operation::Unseal, "unseal"),
        "seal" => (Operation::Seal, "seal"),
        "passphrase.change" => (Operation::PassphraseChange, "change"),
        "shares.rekey.begin" => (Operation::SharesRekey, "begin"),
        "shares.rekey.confirm" => (Operation::SharesRekey, "confirm"),
        "shares.rekey.abort" => (Operation::SharesRekey, "abort"),
        _ => return None,
    })
}

fn bound(
    sealing: &Arc<SealingService>,
    run: impl Fn(&SealingService, &Map<String, Value>) -> OpResult + Send + Sync + 'static,
) -> server::Operation {
    let sealing = sealing.clone();
    Box::new(move |args| run(&sealing, args))
}

fn proof_arg(args: &Map<String, Value>) -> Result<&Value, OpError> {
    match args.get("proof") {
        Some(proof @ Value::Object(_)) => Ok(proof),
        _ => Err(OpError::Arg("'proof' must be an object".into())),
    }
}

fn share_texts(shares: &[Zeroizing<String>]) -> Value {
    shares
        .iter()
        .map(|share| Value::from(share.as_str()))
        .collect()
}

fn state_data(state: SealState) -> Value {
    json!({"state": state.as_str()})
}

fn status_data(status: &Status) -> Value {
    json!({
        "state": status.state.as_str(),
        "initialized_at": status.initialized_at.map(isoformat),
        "unsealed_via": status.unsealed_via.map(UnsealMethod::as_str),
        "recommend_passphrase_change": status.recommend_passphrase_change,
        "consecutive_failures": status.consecutive_failures,
        "retry_after_s": status.retry_after_s,
    })
}

fn unsealed_data(unsealed: &Unsealed) -> Value {
    json!({
        "state": unsealed.state.as_str(),
        "unsealed_via": unsealed.unsealed_via.as_str(),
        "recommend_passphrase_change": unsealed.recommend_passphrase_change,
    })
}

/// The operations of the CA, of the identities and of their Access Policy, and what the channel
/// tells when what the API presents may change.
pub struct IdentityOperations {
    pub ca: Arc<CaService>,
    pub identities: Arc<IdentityService>,
    pub policies: Arc<AccessPolicyService>,
    /// Runs after the CA is created or replaced, after an unseal, after Hécate's certificate is
    /// replaced and after the intermediate is renewed: the supervisor of the API renews Hécate's
    /// certificate if it is due and starts, replaces or stops the listener.
    pub changed: Arc<dyn Fn() + Send + Sync>,
}

/// The operation and detail of the row that a refused request on the CA or the identities leaves.
fn identity_audited(op: &str) -> Option<(Operation, &'static str)> {
    Some(match op {
        "ca.create.begin" => (Operation::CaCreate, "begin"),
        "ca.create.confirm" => (Operation::CaCreate, "confirm"),
        "ca.create.abort" => (Operation::CaCreate, "abort"),
        "ca.intermediate.begin" => (Operation::CaIntermediateRenew, "begin"),
        "ca.intermediate.confirm" => (Operation::CaIntermediateRenew, "confirm"),
        "ca.intermediate.abort" => (Operation::CaIntermediateRenew, "abort"),
        "ca.replace.begin" => (Operation::CaReplace, "begin"),
        "ca.replace.confirm" => (Operation::CaReplace, "confirm"),
        "ca.replace.abort" => (Operation::CaReplace, "abort"),
        "tokens.create" => (Operation::TokenCreate, "create"),
        "tokens.list" => (Operation::TokenList, "list"),
        "tokens.annul" => (Operation::TokenAnnul, "annul"),
        "identities.list" => (Operation::IdentityList, "list"),
        "identities.get" => (Operation::IdentityGet, "get"),
        "identities.disable" => (Operation::IdentityDisable, "disable"),
        "identities.enable" => (Operation::IdentityEnable, "enable"),
        "certificates.revoke" => (Operation::CertificateRevoke, "revoke"),
        _ => return None,
    })
}

/// The operation and detail of the row that a refused request on the Access Policy leaves.
fn policy_audited(op: &str) -> Option<(Operation, &'static str)> {
    Some(match op {
        "policies.grant_read" => (Operation::PolicyGrant, "read"),
        "policies.revoke_read" => (Operation::PolicyRevoke, "read"),
        "policies.grant_status" => (Operation::PolicyGrant, "status"),
        "policies.revoke_status" => (Operation::PolicyRevoke, "status"),
        "policies.list" => (Operation::PolicyList, "list"),
        "policies.get" => (Operation::PolicyGet, "get"),
        _ => return None,
    })
}

/// The `ca` block of `status`: only `exists: false` without a CA.
fn ca_status(summary: Option<CaSummary>, counts: IdentityCounts, throttled: bool) -> Value {
    let Some(summary) = summary else {
        return json!({"exists": false});
    };
    json!({
        "exists": true,
        "trust_domain": summary.trust_domain.as_str(),
        "root_fingerprint": summary.root_fingerprint.to_string(),
        "hecate_certificate": summary.hecate_certificate.map(|own| json!({
            "serial": own.serial.as_str(),
            "not_after": isoformat(own.not_after),
        })),
        "intermediate_days_left": summary.intermediate_days_left,
        "identities_expiring_24h": counts.identities_expiring_24h,
        "suspicious_renewals_24h": counts.suspicious_renewals_24h,
        "enrollment_throttled": throttled,
    })
}

fn register_tokens(handler: &mut RequestHandler, identities: &Arc<IdentityService>) {
    let service = identities.clone();
    handler.register(
        "tokens.create",
        Box::new(move |args| {
            let identity = text_arg(args, "identity")?;
            let created = service.tokens_create(&identity)?;
            Ok(json!({
                "token": created.token.as_str(),
                "token_id": created.token_id.as_str(),
                "identity": created.identity.as_str(),
                "expires_at": isoformat(created.expires_at),
            }))
        }),
    );
    let service = identities.clone();
    handler.register(
        "tokens.list",
        Box::new(move |_| {
            let tokens: Vec<Value> = service
                .tokens_list()?
                .iter()
                .map(|token| {
                    json!({
                        "token_id": token.token_id.as_str(),
                        "identity": token.identity.as_str(),
                        "expires_at": isoformat(token.expires_at),
                    })
                })
                .collect();
            Ok(json!({"tokens": tokens}))
        }),
    );
    let service = identities.clone();
    handler.register(
        "tokens.annul",
        Box::new(move |args| {
            let token_id = text_arg(args, "token_id")?;
            Ok(json!({"token_id": service.tokens_annul(&token_id)?.as_str()}))
        }),
    );
}

/// An optional boolean argument, `false` when absent.
fn flag_arg(args: &Map<String, Value>, key: &str) -> Result<bool, OpError> {
    match args.get(key) {
        None => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(OpError::Arg(format!("'{key}' must be a boolean"))),
    }
}

fn register_identities(handler: &mut RequestHandler, ops: &IdentityOperations) {
    let service = ops.identities.clone();
    handler.register(
        "identities.list",
        Box::new(move |_| {
            let identities: Vec<Value> = service
                .identities_list()?
                .iter()
                .map(|summary| {
                    json!({
                        "identity": summary.identity.as_str(),
                        "status": summary.status.as_str(),
                        "certificate_not_after": isoformat(summary.certificate_not_after),
                    })
                })
                .collect();
            Ok(json!({"identities": identities}))
        }),
    );
    let service = ops.identities.clone();
    handler.register(
        "identities.get",
        Box::new(move |args| {
            let detail = service.identities_get(&text_arg(args, "identity")?)?;
            let certificates: Vec<Value> = detail
                .certificates
                .iter()
                .map(|certificate| {
                    json!({
                        "serial": certificate.serial.as_str(),
                        "not_before": isoformat(certificate.not_before),
                        "not_after": isoformat(certificate.not_after),
                        "state": certificate.state.as_str(),
                        "renewable": certificate.renewable,
                    })
                })
                .collect();
            Ok(json!({
                "identity": detail.identity.as_str(),
                "status": detail.status.as_str(),
                "certificates": certificates,
            }))
        }),
    );
    let service = ops.identities.clone();
    handler.register(
        "identities.disable",
        Box::new(move |args| {
            let identity = text_arg(args, "identity")?;
            let urgent = flag_arg(args, "urgent")?;
            let disabled = service.identities_disable(&identity, urgent)?;
            let revoked: Vec<&str> = disabled.revoked.iter().map(Serial::as_str).collect();
            Ok(json!({"identity": disabled.identity.as_str(), "revoked": revoked}))
        }),
    );
    let service = ops.identities.clone();
    handler.register(
        "identities.enable",
        Box::new(move |args| {
            let identity = service.identities_enable(&text_arg(args, "identity")?)?;
            Ok(json!({"identity": identity.as_str()}))
        }),
    );
    let (service, changed) = (ops.identities.clone(), ops.changed.clone());
    handler.register(
        "certificates.revoke",
        Box::new(move |args| {
            let serial = text_arg(args, "serial")?;
            let urgent = flag_arg(args, "urgent")?;
            let revoked = service.certificates_revoke(&serial, urgent)?;
            // A new certificate of Hécate: the API presents it at once (FR-012).
            if revoked.reissued.is_some() {
                changed();
            }
            Ok(json!({
                "serial": revoked.serial.as_str(),
                "reissued": revoked.reissued.as_ref().map(Serial::as_str),
            }))
        }),
    );
}

/// The arguments of an operation of the Access Policy are exactly its own: one missing or one
/// more is `validation` (contracts/control-protocol.md).
fn exact_args(args: &Map<String, Value>, names: &[&str]) -> Result<(), OpError> {
    if args.keys().all(|key| names.contains(&key.as_str())) {
        return Ok(());
    }
    Err(OpError::Arg(if names.is_empty() {
        "this operation takes no arguments".to_string()
    } else {
        format!("the arguments are {}", names.join(" and "))
    }))
}

fn change_data(change: &PolicyChange) -> Value {
    let mut data = json!({
        "identity": change.identity.as_str(),
        "version": change.version,
        "changed": change.changed,
    });
    if let Some(secret) = &change.secret {
        data["secret"] = json!(secret.to_string());
    }
    data
}

fn change_name(event: &PolicyEvent) -> &'static str {
    match event {
        PolicyEvent::ReadGranted { .. } => "read_granted",
        PolicyEvent::ReadRevoked { .. } => "read_revoked",
        PolicyEvent::StatusGranted { .. } => "status_granted",
        PolicyEvent::StatusRevoked { .. } => "status_revoked",
    }
}

/// A policy as `policies.list` shows it: what it grants now, without its own secrets.
fn policy_data(policy: &AccessPolicy) -> Value {
    json!({
        "identity": policy.identity.as_str(),
        "reads": policy.reads().collect::<Vec<_>>(),
        "status": policy.has_status(),
        "version": policy.version,
    })
}

/// A policy with its history, as `policies.get` shows it: each version with its change, who made
/// it and when.
fn policy_detail(policy: &AccessPolicy) -> Value {
    let history: Vec<Value> = policy
        .history
        .iter()
        .zip(1u64..)
        .map(|(event, version)| {
            json!({
                "version": version,
                "change": change_name(event),
                "secret": event.secret().map(ToString::to_string),
                "at": isoformat(event.at()),
                "by": event.by().as_str(),
            })
        })
        .collect();
    let mut data = policy_data(policy);
    data["history"] = json!(history);
    data
}

fn register_policies(handler: &mut RequestHandler, policies: &Arc<AccessPolicyService>) {
    let service = policies.clone();
    handler.register(
        "policies.grant_read",
        Box::new(move |args| {
            exact_args(args, &["identity", "secret"])?;
            let change =
                service.grant_read(&text_arg(args, "identity")?, &text_arg(args, "secret")?)?;
            Ok(change_data(&change))
        }),
    );
    let service = policies.clone();
    handler.register(
        "policies.revoke_read",
        Box::new(move |args| {
            exact_args(args, &["identity", "secret"])?;
            let change =
                service.revoke_read(&text_arg(args, "identity")?, &text_arg(args, "secret")?)?;
            Ok(change_data(&change))
        }),
    );
    let service = policies.clone();
    handler.register(
        "policies.grant_status",
        Box::new(move |args| {
            exact_args(args, &["identity"])?;
            Ok(change_data(
                &service.grant_status(&text_arg(args, "identity")?)?,
            ))
        }),
    );
    let service = policies.clone();
    handler.register(
        "policies.revoke_status",
        Box::new(move |args| {
            exact_args(args, &["identity"])?;
            Ok(change_data(
                &service.revoke_status(&text_arg(args, "identity")?)?,
            ))
        }),
    );
    let service = policies.clone();
    handler.register(
        "policies.list",
        Box::new(move |args| {
            exact_args(args, &[])?;
            let policies: Vec<Value> = service.list()?.iter().map(policy_data).collect();
            Ok(json!({"policies": policies}))
        }),
    );
    let service = policies.clone();
    handler.register(
        "policies.get",
        Box::new(move |args| {
            exact_args(args, &["identity"])?;
            Ok(policy_detail(&service.get(&text_arg(args, "identity")?)?))
        }),
    );
}

fn register_ca(handler: &mut RequestHandler, ca: &IdentityOperations) {
    let service = ca.ca.clone();
    handler.register(
        "ca.create.begin",
        Box::new(move |args| {
            let trust_domain = text_arg(args, "trust_domain")?;
            let ca_passphrase = Zeroizing::new(text_arg(args, "ca_passphrase")?);
            let begun = service.create_begin(&trust_domain, &ca_passphrase)?;
            Ok(json!({
                "ceremony_id": begun.ceremony_id,
                "intermediate_csr": begun.intermediate_csr,
                "expires_at": isoformat(begun.expires_at),
            }))
        }),
    );
    let (service, changed) = (ca.ca.clone(), ca.changed.clone());
    handler.register(
        "ca.create.confirm",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let root_certificate = text_arg(args, "root_certificate")?;
            let intermediate_certificate = text_arg(args, "intermediate_certificate")?;
            let created = service.create_confirm(
                &ceremony_id,
                &root_certificate,
                &intermediate_certificate,
            )?;
            changed();
            Ok(json!({
                "trust_domain": created.trust_domain.as_str(),
                "root_fingerprint": created.root_fingerprint.to_string(),
                "intermediate_not_after": isoformat(created.intermediate_not_after),
            }))
        }),
    );
    let service = ca.ca.clone();
    handler.register(
        "ca.replace.begin",
        Box::new(move |args| {
            let proof = proof_arg(args)?;
            let ca_passphrase = Zeroizing::new(text_arg(args, "ca_passphrase")?);
            let begun = service.replace_begin(proof, &ca_passphrase)?;
            Ok(json!({
                "ceremony_id": begun.ceremony_id,
                "intermediate_csr": begun.intermediate_csr,
                "expires_at": isoformat(begun.expires_at),
            }))
        }),
    );
    let (service, changed) = (ca.ca.clone(), ca.changed.clone());
    handler.register(
        "ca.replace.confirm",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let root_certificate = text_arg(args, "root_certificate")?;
            let intermediate_certificate = text_arg(args, "intermediate_certificate")?;
            let replaced = service.replace_confirm(
                &ceremony_id,
                &root_certificate,
                &intermediate_certificate,
            )?;
            // The API presents the certificate of the new CA and trusts only its root.
            changed();
            Ok(json!({
                "trust_domain": replaced.trust_domain.as_str(),
                "root_fingerprint": replaced.root_fingerprint.to_string(),
                "generation": replaced.generation,
            }))
        }),
    );
    let service = ca.ca.clone();
    handler.register(
        "ca.replace.abort",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let reason = text_arg(args, "reason")?;
            service.replace_abort(&ceremony_id, &reason)?;
            Ok(json!({}))
        }),
    );
    let service = ca.ca.clone();
    handler.register(
        "ca.intermediate.begin",
        Box::new(move |_| {
            let begun = service.intermediate_begin()?;
            Ok(json!({
                "ceremony_id": begun.ceremony_id,
                "intermediate_csr": begun.intermediate_csr,
                "root_certificate": begun.root_certificate,
                "expires_at": isoformat(begun.expires_at),
            }))
        }),
    );
    let (service, changed) = (ca.ca.clone(), ca.changed.clone());
    handler.register(
        "ca.intermediate.confirm",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let intermediate_certificate = text_arg(args, "intermediate_certificate")?;
            let renewed = service.intermediate_confirm(&ceremony_id, &intermediate_certificate)?;
            // With the intermediate expired, Hécate's certificate renews now and the API listens.
            changed();
            Ok(json!({
                "intermediate_version": renewed.intermediate_version,
                "intermediate_not_after": isoformat(renewed.intermediate_not_after),
            }))
        }),
    );
    let service = ca.ca.clone();
    handler.register(
        "ca.intermediate.abort",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let reason = text_arg(args, "reason")?;
            service.intermediate_abort(&ceremony_id, &reason)?;
            Ok(json!({}))
        }),
    );
    let service = ca.ca.clone();
    handler.register(
        "ca.create.abort",
        Box::new(move |args| {
            let ceremony_id = text_arg(args, "ceremony_id")?;
            let reason = text_arg(args, "reason")?;
            service.create_abort(&ceremony_id, &reason)?;
            Ok(json!({}))
        }),
    );
}

/// The operation of the row that a refused request on secrets leaves (rows with subject `admin`).
fn secret_audited(op: &str) -> Option<Operation> {
    Some(match op {
        "secrets.create" => Operation::SecretCreate,
        "secrets.add_version" => Operation::SecretAddVersion,
        "secrets.list" => Operation::SecretList,
        "secrets.get" => Operation::SecretGet,
        "secrets.revoke" => Operation::SecretRevoke,
        "secrets.restore" => Operation::SecretRestore,
        "secrets.promote" => Operation::SecretPromote,
        "secrets.destroy" => Operation::SecretDestroy,
        _ => return None,
    })
}

fn raw(args: &Map<String, Value>, key: &str) -> Value {
    args.get(key).cloned().unwrap_or(Value::Null)
}

fn created_data(created: &Created) -> Value {
    json!({
        "path": created.path,
        "version": created.version,
        "fingerprint": created.fingerprint,
        "stream_version": created.stream_version,
    })
}

fn summary_data(summary: &SecretSummary) -> Value {
    json!({
        "path": summary.path,
        "value_type": summary.value_type.as_str(),
        "active_version": summary.active_version,
        "version_count": summary.version_count,
        "rotate_every": summary.rotate_every,
        "expires_at": summary.expires_at.map(isoformat),
    })
}

fn details_data(details: &SecretDetails) -> Value {
    let versions: Vec<Value> = details
        .versions
        .iter()
        .map(|version| {
            json!({
                "version": version.version,
                "state": version.state.as_str(),
                "created_at": isoformat(version.created_at),
                "deprecated_at": version.deprecated_at.map(isoformat),
                "fingerprint": version.fingerprint,
            })
        })
        .collect();
    json!({
        "path": details.path,
        "owner": details.owner,
        "value_type": details.value_type.as_str(),
        "max_versions": details.max_versions,
        "rotate_every": details.rotate_every,
        "expires_at": details.expires_at.map(isoformat),
        "stream_version": details.stream_version,
        "versions": versions,
    })
}

fn added_data(added: &Added) -> Value {
    json!({
        "version": added.version,
        "fingerprint": added.fingerprint,
        "stream_version": added.stream_version,
    })
}

type Transition = fn(
    &SecretsService,
    &str,
    &Value,
    &Value,
) -> Result<u64, hecate_application::errors::ServiceError>;

fn register_secrets(handler: &mut RequestHandler, secrets: &Arc<SecretsService>) {
    let bound = |run: fn(&SecretsService, &Map<String, Value>) -> OpResult| -> server::Operation {
        let secrets = secrets.clone();
        Box::new(move |args| run(&secrets, args))
    };
    handler.register(
        "secrets.create",
        bound(|s, args| {
            let path = text_arg(args, "path")?;
            let value = Zeroizing::new(text_arg(args, "value")?);
            let value_type = text_arg(args, "value_type")?;
            let options = CreateOptions {
                max_versions: raw(args, "max_versions"),
                rotate_every: raw(args, "rotate_every"),
                expires_at: raw(args, "expires_at"),
            };
            Ok(created_data(&s.create(
                &path,
                &value,
                &value_type,
                &options,
            )?))
        }),
    );
    handler.register(
        "secrets.list",
        bound(|s, args| {
            let items: Vec<Value> = s
                .list_secrets(&raw(args, "identity"))?
                .iter()
                .map(summary_data)
                .collect();
            Ok(json!({"secrets": items}))
        }),
    );
    handler.register(
        "secrets.get",
        bound(|s, args| Ok(details_data(&s.get(&text_arg(args, "path")?)?))),
    );
    handler.register(
        "secrets.add_version",
        bound(|s, args| {
            let path = text_arg(args, "path")?;
            let value = Zeroizing::new(text_arg(args, "value")?);
            Ok(added_data(&s.add_version(
                &path,
                &value,
                &raw(args, "expected_version"),
            )?))
        }),
    );
    let transitions: [(&'static str, Transition); 3] = [
        ("secrets.revoke", SecretsService::revoke),
        ("secrets.restore", SecretsService::restore),
        ("secrets.promote", SecretsService::promote),
    ];
    for (name, transition) in transitions {
        let secrets = secrets.clone();
        handler.register(
            name,
            Box::new(move |args| {
                let path = text_arg(args, "path")?;
                let stream_version = transition(
                    &secrets,
                    &path,
                    &raw(args, "version"),
                    &raw(args, "expected_version"),
                )?;
                Ok(json!({"stream_version": stream_version}))
            }),
        );
    }
    handler.register(
        "secrets.destroy",
        bound(|s, args| {
            let proof = proof_arg(args)?;
            let path = text_arg(args, "path")?;
            let stream_version = s.destroy(
                &path,
                &raw(args, "version"),
                &raw(args, "expected_version"),
                proof,
            )?;
            Ok(json!({"stream_version": stream_version}))
        }),
    );
}

fn record_refused(
    sealing: &SealingService,
    secrets: Option<&SecretsService>,
    identities: Option<&IdentityOperations>,
    op: &str,
    args: &Map<String, Value>,
) {
    if let Some((operation, detail)) = policy_audited(op) {
        if let Some(ops) = identities
            && let Err(error) = ops.policies.record_invalid_request(operation, detail)
        {
            tracing::error!(target: "hecate.control", op, exception = error.kind(), "unexpected error");
        }
        return;
    }
    if let Some((operation, detail)) = identity_audited(op) {
        let recorded = match identities {
            Some(ops)
                if matches!(
                    operation,
                    Operation::CaCreate | Operation::CaIntermediateRenew | Operation::CaReplace
                ) =>
            {
                ops.ca.record_invalid_request(operation, detail)
            }
            Some(ops) => ops.identities.record_invalid_request(operation, detail),
            None => Ok(()),
        };
        if let Err(error) = recorded {
            tracing::error!(target: "hecate.control", op, exception = error.kind(), "unexpected error");
        }
        return;
    }
    if let Some(operation) = secret_audited(op) {
        if let Some(secrets) = secrets
            && let Err(error) =
                secrets.record_invalid_request(operation, op.trim_start_matches("secrets."))
        {
            tracing::error!(target: "hecate.control", op, exception = error.kind(), "unexpected error");
        }
        return;
    }
    let Some((operation, detail)) = audited(op) else {
        return;
    };
    let fields = if matches!(op, "passphrase.change" | "shares.rekey.begin") {
        args.get("proof").and_then(Value::as_object)
    } else {
        Some(args)
    };
    let method = fields.map_or(Method::Local, SealingService::fields_method);
    if let Err(error) = sealing.record_invalid_request(operation, method, detail) {
        tracing::error!(target: "hecate.control", op, exception = error.kind(), "unexpected error");
    }
}

/// The channel's operations on the sealing service and, when given, on the secrets service.
pub fn request_handler(
    sealing: Arc<SealingService>,
    secrets: Option<Arc<SecretsService>>,
) -> RequestHandler {
    build(sealing, secrets, None)
}

/// The channel's operations with those of the CA and the identities too, as `hecate serve`
/// builds it.
pub fn request_handler_with_identities(
    sealing: Arc<SealingService>,
    secrets: Option<Arc<SecretsService>>,
    identities: IdentityOperations,
) -> RequestHandler {
    build(sealing, secrets, Some(identities))
}

fn build(
    sealing: Arc<SealingService>,
    secrets: Option<Arc<SecretsService>>,
    ca: Option<IdentityOperations>,
) -> RequestHandler {
    let mut handler = RequestHandler::new();
    let (status_sealing, status_secrets) = (sealing.clone(), secrets.clone());
    let status_ca = ca
        .as_ref()
        .map(|ops| (ops.ca.clone(), ops.identities.clone()));
    handler.register(
        "status",
        Box::new(move |_| {
            let mut data = status_data(&status_sealing.status()?);
            if let Some(secrets) = &status_secrets {
                let counts = secrets.expiry_counts(None)?;
                data["secrets_expired"] = json!(counts.secrets_expired);
                data["secrets_expiring_soon"] = json!(counts.secrets_expiring_soon);
                data["secrets_due_rotation"] = json!(counts.secrets_due_rotation);
            }
            if let Some((ca, identities)) = &status_ca {
                data["ca"] = ca_status(
                    ca.status()?,
                    identities.counts()?,
                    identities.enrollment_throttled()?,
                );
            }
            Ok(data)
        }),
    );
    handler.register(
        "init.begin",
        bound(&sealing, |s, args| {
            let passphrase = Zeroizing::new(text_arg(args, "passphrase")?);
            let begun = s.init_begin(&passphrase)?;
            Ok(json!({
                "init_id": begun.init_id.as_str(),
                "shares": share_texts(&begun.shares),
                "expires_at": isoformat(begun.expires_at),
                "warning": begun.warning,
            }))
        }),
    );
    handler.register(
        "init.confirm",
        bound(&sealing, |s, args| {
            let init_id = text_arg(args, "init_id")?;
            let shares = Zeroizing::new(texts_arg(args, "shares")?);
            Ok(state_data(s.init_confirm(&init_id, &shares[..])?))
        }),
    );
    handler.register(
        "init.abort",
        bound(&sealing, |s, args| {
            s.init_abort(&text_arg(args, "init_id")?)?;
            Ok(json!({}))
        }),
    );
    let unseal_changed = ca.as_ref().map(|ca| ca.changed.clone());
    let unseal_sealing = sealing.clone();
    handler.register(
        "unseal",
        Box::new(move |args| {
            let s = &unseal_sealing;
            let unsealed = match (args.contains_key("passphrase"), args.contains_key("shares")) {
                (true, false) => {
                    s.unseal_with_passphrase(&Zeroizing::new(text_arg(args, "passphrase")?))?
                }
                (false, true) => {
                    s.unseal_with_shares(&Zeroizing::new(texts_arg(args, "shares")?)[..])?
                }
                _ => {
                    return Err(OpError::Arg(
                        "'unseal' needs either 'passphrase' or 'shares'".into(),
                    ));
                }
            };
            if let Some(changed) = &unseal_changed {
                changed();
            }
            Ok(unsealed_data(&unsealed))
        }),
    );
    let seal_ca = ca.as_ref().map(|ops| ops.ca.clone());
    let seal_sealing = sealing.clone();
    handler.register(
        "seal",
        Box::new(move |_| {
            let state = seal_sealing.seal()?;
            if let Some(ca) = &seal_ca {
                ca.forget_lost_ceremony();
            }
            Ok(state_data(state))
        }),
    );
    handler.register(
        "passphrase.change",
        bound(&sealing, |s, args| {
            let proof = proof_arg(args)?;
            let new_passphrase = Zeroizing::new(text_arg(args, "new_passphrase")?);
            Ok(json!({"config_version": s.change_passphrase(proof, &new_passphrase)?}))
        }),
    );
    handler.register(
        "shares.rekey.begin",
        bound(&sealing, |s, args| {
            let begun = s.rekey_begin(proof_arg(args)?)?;
            Ok(json!({
                "rekey_id": begun.rekey_id.as_str(),
                "shares": share_texts(&begun.shares),
                "expires_at": isoformat(begun.expires_at),
            }))
        }),
    );
    handler.register(
        "shares.rekey.confirm",
        bound(&sealing, |s, args| {
            let rekey_id = text_arg(args, "rekey_id")?;
            let shares = Zeroizing::new(texts_arg(args, "shares")?);
            Ok(json!({"config_version": s.rekey_confirm(&rekey_id, &shares[..])?}))
        }),
    );
    handler.register(
        "shares.rekey.abort",
        bound(&sealing, |s, args| {
            s.rekey_abort(&text_arg(args, "rekey_id")?)?;
            Ok(json!({}))
        }),
    );
    if let Some(secrets) = &secrets {
        register_secrets(&mut handler, secrets);
    }
    if let Some(ops) = &ca {
        register_ca(&mut handler, ops);
        register_tokens(&mut handler, &ops.identities);
        register_identities(&mut handler, ops);
        register_policies(&mut handler, &ops.policies);
    }
    handler.on_refused(Box::new(move |op, args| {
        record_refused(&sealing, secrets.as_deref(), ca.as_ref(), op, args)
    }));
    handler
}
