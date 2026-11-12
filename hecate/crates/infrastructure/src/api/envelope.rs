//! The common envelope of the API (contracts/api.md): the same shape as the local channel, the
//! closed codes with the HTTP status that Hécate fixes, `Retry-After` and `X-Request-Id`.

use std::io;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use data_encoding::HEXLOWER;
use hecate_application::errors::ServiceError;
use hecate_application::identity::service::CallRefusal;
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use serde::Serialize;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use crate::api::server::{RefusalRecorder, record_refusal};
use crate::control::protocol::{Envelope, ErrorBody, Meta};

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const SEALED: &str = "Hécate is sealed; retry later";

/// The errors of the API: the common codes that Hécate uses, each with its HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Validation(String),
    Auth(String),
    NotFound(String),
    Conflict(String),
    RateLimited { message: String, retry_after_s: u64 },
    Unavailable(String),
    Internal,
}

impl ApiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::Auth(_) => "auth",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unavailable(_) => "unavailable",
            Self::Internal => "internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Unavailable(_) | Self::Internal
        )
    }

    fn message(&self) -> String {
        match self {
            Self::Validation(message)
            | Self::Auth(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Unavailable(message)
            | Self::RateLimited { message, .. } => message.clone(),
            Self::Internal => "internal error".to_string(),
        }
    }

    fn retry_after_s(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_s, .. } => Some(*retry_after_s),
            _ => None,
        }
    }
}

impl From<ServiceError> for ApiError {
    /// A use case's error; sealed is `unavailable` here, while the local channel says `conflict`.
    fn from(error: ServiceError) -> Self {
        let ServiceError::Hecate(error) = error else {
            return Self::Internal;
        };
        let message = error.to_string();
        match error {
            HecateError::InvalidInput(_) => Self::Validation(message),
            HecateError::AuthenticationFailed(_) => Self::Auth(message),
            HecateError::RateLimited { retry_after_s } => Self::RateLimited {
                message,
                retry_after_s,
            },
            HecateError::NotFound(_) => Self::NotFound(message),
            HecateError::StateConflict {
                state: Some(SealState::Sealed),
                ..
            } => Self::Unavailable(SEALED.to_string()),
            HecateError::StateConflict { .. } => Self::Conflict(message),
        }
    }
}

/// The request id and the start of a request, set by [`request_context`] for every handler.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
    started: Instant,
}

impl RequestContext {
    fn new(request_id: String, started: Instant) -> Self {
        Self {
            request_id,
            started,
        }
    }

    fn meta(&self) -> Meta {
        Meta {
            request_id: Some(self.request_id.clone()),
            duration_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }

    pub fn completed(&self, data: Value) -> Response {
        let envelope = Envelope {
            status: "completed",
            data,
            error: None,
            meta: self.meta(),
        };
        json_response(StatusCode::OK, &envelope, None)
    }

    pub fn failed(&self, error: &ApiError) -> Response {
        let envelope = Envelope {
            status: "failed",
            data: Value::Object(Map::new()),
            error: Some(ErrorBody {
                code: error.code(),
                message: error.message(),
                retryable: error.retryable(),
                retry_after_s: error.retry_after_s(),
            }),
            meta: self.meta(),
        };
        json_response(error.status(), &envelope, error.retry_after_s())
    }

    /// A completed answer whose data carries secrets: the envelope of [`Self::completed`], with
    /// `duration_ms` taken once when it is built, in a buffer of exactly its size that is wiped
    /// when hyper drops the body (FR-016, research R6).
    pub fn completed_with_secrets(&self, data: &impl Serialize) -> Response {
        self.completed_owned(data, |bytes| bytes)
    }

    /// The same, with the buffer in an owner of the caller's, which the body keeps until hyper
    /// drops it.
    pub fn completed_owned<O>(
        &self,
        data: &impl Serialize,
        owner: impl FnOnce(Zeroizing<Vec<u8>>) -> O,
    ) -> Response
    where
        O: AsRef<[u8]> + Send + 'static,
    {
        let envelope = SecretEnvelope {
            status: "completed",
            data,
            error: None,
            meta: self.meta(),
        };
        let Ok(bytes) = wiped_json(&envelope) else {
            return self.failed(&ApiError::Internal);
        };
        let mut response = Response::new(Body::from(Bytes::from_owner(owner(bytes))));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    }
}

/// The envelope of an answer whose data carries secrets: the members of `Envelope`, with the
/// data serialized where it is instead of copied into a `Value`.
#[derive(Serialize)]
struct SecretEnvelope<'a, D> {
    status: &'static str,
    data: &'a D,
    error: Option<ErrorBody>,
    meta: Meta,
}

/// Counts the bytes written to it, to size a buffer before writing into it.
struct Counter(usize);

impl io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The JSON of a value that carries secrets (research R6) in a buffer of exactly its size: a first
/// pass counts the bytes and a second writes them, so the buffer never grows and leaves no copies
/// behind. It is wiped when dropped.
pub fn wiped_json(value: &impl Serialize) -> serde_json::Result<Zeroizing<Vec<u8>>> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value)?;
    let mut buffer = Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *buffer, value)?;
    Ok(buffer)
}

fn json_response(status: StatusCode, envelope: &Envelope, retry_after_s: Option<u64>) -> Response {
    let body = serde_json::to_vec(envelope).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if let Some(seconds) = retry_after_s {
        headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    }
    response
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Self>()
            .cloned()
            .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// From 1 to 128 printable ASCII characters, as `Idempotency-Key`.
fn valid_request_id(text: &str) -> bool {
    (1..=128).contains(&text.len()) && text.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn generated_request_id() -> String {
    let mut bytes = [0; 16];
    aws_lc_rs::rand::fill(&mut bytes).expect("the system random source works");
    HEXLOWER.encode(&bytes)
}

/// Sets the context of the request: the `X-Request-Id` it brings, or a new one. One that does
/// not follow its rule is `validation`, unless `refused`, which leaves its row, answers instead.
pub async fn request_context(
    State(refused): State<RefusalRecorder>,
    mut request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let provided = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .map(|value| value.to_str().ok().filter(|text| valid_request_id(text)));
    let context = match provided {
        None => RequestContext::new(generated_request_id(), started),
        Some(Some(text)) => RequestContext::new(text.to_string(), started),
        Some(None) => {
            let (method, path) = (request.method().clone(), request.uri().path().to_string());
            let answer = record_refusal(refused, method, path, CallRefusal::RequestId).await;
            return RequestContext::new(generated_request_id(), started).failed(
                &answer.unwrap_or_else(|| {
                    ApiError::Validation(
                        "X-Request-Id must be 1 to 128 printable ASCII characters".into(),
                    )
                }),
            );
        }
    };
    request.extensions_mut().insert(context);
    next.run(request).await
}
