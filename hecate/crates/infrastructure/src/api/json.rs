//! Bodies of standard JSON of up to 64 KiB (FR-029). Anything else is `validation`, without a row
//! in the access log and with a structured log line that does not carry the body.

use std::future::poll_fn;
use std::pin::Pin;

use axum::body::HttpBody;
use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::api::envelope::{ApiError, RequestContext};
use crate::control::protocol::append_erasing;

pub const MAX_BODY: usize = 64 * 1024;

/// A request body as a JSON value. The bytes are read into memory that is erased when dropped:
/// bodies carry enrollment tokens.
pub struct JsonBody(pub Value);

fn rejected(context: Option<&RequestContext>, reason: &'static str) -> Response {
    let request_id = context
        .map(|context| context.request_id.as_str())
        .unwrap_or("");
    tracing::warn!(target: "hecate.api", request_id, reason, "request rejected");
    match context {
        Some(context) => context.failed(&ApiError::Validation(reason.to_string())),
        None => axum::http::StatusCode::BAD_REQUEST.into_response(),
    }
}

impl<S: Send + Sync> FromRequest<S> for JsonBody {
    type Rejection = Response;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let context = request.extensions().get::<RequestContext>().cloned();
        let mut body = request.into_body();
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(frame) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
            let Ok(frame) = frame else {
                return Err(rejected(context.as_ref(), "the body could not be read"));
            };
            if let Ok(data) = frame.into_data() {
                if bytes.len() + data.len() > MAX_BODY {
                    return Err(rejected(context.as_ref(), "the body is larger than 64 KiB"));
                }
                append_erasing(&mut bytes, &data);
            }
        }
        serde_json::from_slice(&bytes)
            .map(JsonBody)
            .map_err(|_| rejected(context.as_ref(), "the body is not standard JSON"))
    }
}
