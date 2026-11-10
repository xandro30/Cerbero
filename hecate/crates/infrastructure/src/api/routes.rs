//! The operations of the API (contracts/api.md): each one runs its use case on a blocking thread,
//! since the services are synchronous, and answers with the common envelope. A request refused
//! before its operation runs leaves the row of that operation (FR-024).

use std::sync::Arc;

use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method};
use axum::response::Response;
use axum::routing::{get, post};
use hecate_application::errors::ServiceError;
use hecate_application::identity::service::{CallRefusal, Enrolled, IdentityService, PublishedKey};
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{Method as AccessMethod, Operation};
use hecate_domain::sealing::errors::HecateError;
use serde_json::{Value, json};

use crate::api::auth::CallCredentials;
use crate::api::envelope::{ApiError, RequestContext};
use crate::api::instant;
use crate::api::json::JsonBody;
use crate::api::server::RefusalRecorder;
use crate::control::protocol::erase;

pub const IDEMPOTENCY_KEY: &str = "idempotency-key";
const CERTIFICATE: &str = "CERTIFICATE";
const CRL: &str = "X509 CRL";
/// The operations of this version of the API, as `GET /v1/capabilities` lists them.
pub const OPERATIONS: [&str; 6] = [
    "identities:enroll",
    "certificates:renew",
    "revocations",
    "keys",
    "keys/{kid}",
    "capabilities",
];

/// The routes of the API, which the operations of the components fill.
pub fn routes(identities: Arc<IdentityService>) -> Router {
    Router::new()
        .route("/v1/identities:enroll", post(enroll))
        .route("/v1/certificates:renew", post(renew))
        .route("/v1/revocations", get(revocations))
        .route("/v1/keys", get(keys))
        .route("/v1/keys/{kid}", get(key))
        .route("/v1/capabilities", get(capabilities))
        .with_state(identities)
}

/// The operation of a request and how its caller authenticates, after its method and its path:
/// the routes of [`routes`], with `HEAD` as the `GET` route, since axum serves it with the same
/// operation. `None` for a request that is not an operation.
pub fn operation_of(method: &Method, path: &str) -> Option<(Operation, AccessMethod)> {
    let verb = if *method == Method::HEAD {
        "GET"
    } else {
        method.as_str()
    };
    let operation = match (verb, path) {
        ("POST", "/v1/identities:enroll") => {
            return Some((Operation::IdentityEnroll, AccessMethod::EnrollmentToken));
        }
        ("POST", "/v1/certificates:renew") => Operation::CertificateRenew,
        ("GET", "/v1/revocations") => Operation::RevocationList,
        ("GET", "/v1/keys") => Operation::KeyList,
        ("GET", "/v1/capabilities") => Operation::CapabilitiesGet,
        ("GET", path)
            if path
                .strip_prefix("/v1/keys/")
                .is_some_and(|kid| !kid.is_empty() && !kid.contains('/')) =>
        {
            Operation::KeyGet
        }
        _ => return None,
    };
    Some((operation, AccessMethod::Identity))
}

/// What leaves the row of a request refused before its operation runs (FR-024), when the request
/// is an operation of the API. While the enrollments are throttled, an enrollment refused for its
/// request id is answered `rate_limited` instead (FR-034).
pub fn refusals(identities: Arc<IdentityService>) -> RefusalRecorder {
    Arc::new(move |method: &Method, path: &str, refusal: CallRefusal| {
        let (operation, how) = operation_of(method, path)?;
        match identities.record_refused_call(operation, how, refusal) {
            Ok(()) => None,
            Err(error @ ServiceError::Hecate(HecateError::RateLimited { .. })) => {
                Some(ApiError::from(error))
            }
            Err(error) => {
                tracing::error!(target: "hecate.api", reason = %error, "cannot leave the row of a refused request");
                None
            }
        }
    })
}

fn enrolled_data(enrolled: &Enrolled) -> Value {
    json!({
        "identity": enrolled.identity.to_string(),
        "serial": enrolled.serial.as_str(),
        "certificate": pem::encode(CERTIFICATE, &enrolled.certificate),
        "chain": enrolled
            .chain
            .iter()
            .map(|der| pem::encode(CERTIFICATE, der))
            .collect::<Vec<_>>(),
        "not_after": instant(enrolled.not_after),
        "call_key_id": enrolled.call_key_id,
    })
}

/// An Idempotency-Key that is not visible ASCII reaches the service as an empty one, which it
/// refuses with `validation`.
fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get(IDEMPOTENCY_KEY)
        .map(|value| value.to_str().unwrap_or_default().to_string())
}

/// `POST /v1/identities:enroll`: the token authenticates the request, so a client certificate,
/// if one arrives, is not used. The body carries the token and is erased after the use case.
async fn enroll(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    headers: HeaderMap,
    JsonBody(mut body): JsonBody,
) -> Response {
    let key = idempotency_key(&headers);
    let outcome = tokio::task::spawn_blocking(move || {
        let outcome = identities.enroll(key.as_deref(), &body);
        erase(&mut body);
        outcome
    })
    .await;
    answer(&context, outcome)
}

fn answer(
    context: &RequestContext,
    outcome: Result<Result<Enrolled, ServiceError>, tokio::task::JoinError>,
) -> Response {
    reply(context, outcome, |enrolled| enrolled_data(&enrolled))
}

/// The envelope of what a use case run on a blocking thread answered.
fn reply<T>(
    context: &RequestContext,
    outcome: Result<Result<T, ServiceError>, tokio::task::JoinError>,
    data: impl FnOnce(T) -> Value,
) -> Response {
    match outcome {
        Ok(Ok(value)) => context.completed(data(value)),
        Ok(Err(error)) => context.failed(&ApiError::from(error)),
        Err(_) => context.failed(&ApiError::Internal),
    }
}

/// A key as RFC 8037 writes it, with the members of Cerbero (contracts/formats.md).
fn jwk(key: &PublishedKey) -> Value {
    json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": key.key.x(),
        "kid": key.key.kid(),
        "alg": "EdDSA",
        "use": "sig",
        "cerbero_identity": key.identity.to_string(),
        "cerbero_status": if key.active { "active" } else { "verify_only" },
        "cerbero_not_after": instant(key.not_after),
    })
}

/// `GET /v1/revocations`: the CRLs of the intermediates, valid 300 s; served, it leaves no row.
async fn revocations(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    CallCredentials(credentials): CallCredentials,
) -> Response {
    let outcome = tokio::task::spawn_blocking(move || identities.revocations(&credentials)).await;
    reply(&context, outcome, |revocations| {
        json!({
            "crls": revocations
                .crls
                .iter()
                .map(|der| pem::encode(CRL, der))
                .collect::<Vec<_>>(),
            "expires_at": instant(revocations.expires_at),
        })
    })
}

/// `GET /v1/keys`: the keys valid for new calls, valid 300 s; served, it leaves no row.
async fn keys(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    CallCredentials(credentials): CallCredentials,
) -> Response {
    let outcome = tokio::task::spawn_blocking(move || identities.keys(&credentials)).await;
    reply(&context, outcome, |published| {
        json!({
            "keys": published.keys.iter().map(jwk).collect::<Vec<_>>(),
            "expires_at": instant(published.expires_at),
        })
    })
}

/// `GET /v1/keys/{kid}`: any registered key, also one that only verifies past signatures. A kid
/// that is not valid UTF-8 once decoded reaches the use case as an empty one, which it refuses
/// with `validation` and its row after authenticating, like any malformed kid (FR-024).
async fn key(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    CallCredentials(credentials): CallCredentials,
    kid: Result<Path<String>, PathRejection>,
) -> Response {
    let kid = kid.map(|Path(kid)| kid).unwrap_or_default();
    let outcome = tokio::task::spawn_blocking(move || identities.key(&credentials, &kid)).await;
    reply(&context, outcome, |key| json!({"key": jwk(&key)}))
}

/// `GET /v1/capabilities`: what this version of the API offers.
async fn capabilities(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    CallCredentials(credentials): CallCredentials,
) -> Response {
    let outcome = tokio::task::spawn_blocking(move || identities.capabilities(&credentials)).await;
    reply(
        &context,
        outcome,
        |()| json!({"component": "hecate", "api": "v1", "operations": OPERATIONS}),
    )
}

/// `POST /v1/certificates:renew`: the certificate of the connection and a short call token
/// authenticate the request, and it answers the same fields as an enrollment.
async fn renew(
    State(identities): State<Arc<IdentityService>>,
    context: RequestContext,
    CallCredentials(credentials): CallCredentials,
    headers: HeaderMap,
    JsonBody(body): JsonBody,
) -> Response {
    let key = idempotency_key(&headers);
    let outcome =
        tokio::task::spawn_blocking(move || identities.renew(&credentials, key.as_deref(), &body))
            .await;
    answer(&context, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_operation_of_the_api_is_known_by_its_method_and_path() {
        let kid = "A".repeat(43);
        let cases = [
            (
                Method::POST,
                "/v1/identities:enroll".to_string(),
                Some((Operation::IdentityEnroll, AccessMethod::EnrollmentToken)),
            ),
            (
                Method::POST,
                "/v1/certificates:renew".to_string(),
                Some((Operation::CertificateRenew, AccessMethod::Identity)),
            ),
            (
                Method::GET,
                "/v1/revocations".to_string(),
                Some((Operation::RevocationList, AccessMethod::Identity)),
            ),
            (
                Method::GET,
                "/v1/keys".to_string(),
                Some((Operation::KeyList, AccessMethod::Identity)),
            ),
            (
                Method::GET,
                format!("/v1/keys/{kid}"),
                Some((Operation::KeyGet, AccessMethod::Identity)),
            ),
            (
                Method::GET,
                "/v1/capabilities".to_string(),
                Some((Operation::CapabilitiesGet, AccessMethod::Identity)),
            ),
            // T130: axum serves `HEAD` with the operation of the `GET` route.
            (
                Method::HEAD,
                "/v1/keys".to_string(),
                Some((Operation::KeyList, AccessMethod::Identity)),
            ),
            (
                Method::HEAD,
                format!("/v1/keys/{kid}"),
                Some((Operation::KeyGet, AccessMethod::Identity)),
            ),
            (
                Method::HEAD,
                "/v1/revocations".to_string(),
                Some((Operation::RevocationList, AccessMethod::Identity)),
            ),
            (
                Method::HEAD,
                "/v1/capabilities".to_string(),
                Some((Operation::CapabilitiesGet, AccessMethod::Identity)),
            ),
            (Method::HEAD, "/v1/identities:enroll".to_string(), None),
            (Method::HEAD, "/v1/certificates:renew".to_string(), None),
            (Method::GET, "/v1/identities:enroll".to_string(), None),
            (Method::POST, "/v1/keys".to_string(), None),
            (Method::GET, "/v1/keys/".to_string(), None),
            (Method::GET, format!("/v1/keys/{kid}/more"), None),
            (Method::GET, "/v1/certificates/renew".to_string(), None),
            (Method::GET, "/anything".to_string(), None),
        ];
        for (method, path, expected) in cases {
            assert_eq!(operation_of(&method, &path), expected, "{method} {path}");
        }
    }
}
