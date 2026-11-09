//! The sealing operations of the local channel: the argument checks of the Python version, the
//! data of each reply and the access log row that a refused request leaves (research R8, R10).

use std::sync::Arc;

use hecate_application::sealing::service::{SealingService, Status, Unsealed};
use hecate_application::secrets::service::{
    Added, CreateOptions, Created, SecretDetails, SecretSummary, SecretsService,
};
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
    op: &str,
    args: &Map<String, Value>,
) {
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
    let mut handler = RequestHandler::new();
    let (status_sealing, status_secrets) = (sealing.clone(), secrets.clone());
    handler.register(
        "status",
        Box::new(move |_| {
            let mut data = status_data(&status_sealing.status()?);
            if let Some(secrets) = &status_secrets {
                let counts = secrets.expiry_counts(None)?;
                data["secrets_expired"] = json!(counts.secrets_expired);
                data["secrets_expiring_soon"] = json!(counts.secrets_expiring_soon);
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
    handler.register(
        "unseal",
        bound(&sealing, |s, args| {
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
            Ok(unsealed_data(&unsealed))
        }),
    );
    handler.register("seal", bound(&sealing, |s, _| Ok(state_data(s.seal()?))));
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
    handler.on_refused(Box::new(move |op, args| {
        record_refused(&sealing, secrets.as_deref(), op, args)
    }));
    handler
}
