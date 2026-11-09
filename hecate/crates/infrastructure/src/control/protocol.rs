//! The local channel's protocol: one JSON request per line and the common envelope (research R9).
//! Requests must be standard JSON in UTF-8 (FR-018); replies are written with serde_json.

use std::io;
use std::time::Instant;

use hecate_application::errors::ServiceError;
use hecate_domain::sealing::errors::HecateError;
use serde::Serialize;
use serde_json::{Map, Value};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub request_id: Option<String>,
    pub op: String,
    pub args: Map<String, Value>,
}

/// Parses one line; anything malformed is a `validation` error with the Python messages.
pub fn parse_request(raw: &[u8]) -> Result<Request, HecateError> {
    let document: Value = serde_json::from_slice(raw)
        .map_err(|_| HecateError::invalid("request is not valid JSON"))?;
    let Value::Object(mut document) = document else {
        return Err(HecateError::invalid(
            "request must be an object with a string 'op'",
        ));
    };
    let Some(Value::String(op)) = document.remove("op") else {
        return Err(HecateError::invalid(
            "request must be an object with a string 'op'",
        ));
    };
    let args = match document.remove("args") {
        None => Map::new(),
        Some(Value::Object(args)) => args,
        Some(_) => return Err(HecateError::invalid("'args' must be an object")),
    };
    let request_id = match document.remove("request_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(request_id)) => Some(request_id),
        Some(_) => return Err(HecateError::invalid("'request_id' must be a string")),
    };
    Ok(Request {
        request_id,
        op,
        args,
    })
}

#[derive(Debug, Serialize)]
pub struct Envelope {
    pub status: &'static str,
    pub data: Value,
    pub error: Option<ErrorBody>,
    pub meta: Meta,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_s: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Meta {
    pub request_id: Option<String>,
    pub duration_ms: u64,
}

fn meta(request_id: Option<String>, started: Instant) -> Meta {
    Meta {
        request_id,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }
}

pub fn completed(request_id: Option<String>, data: Value, started: Instant) -> Envelope {
    Envelope {
        status: "completed",
        data,
        error: None,
        meta: meta(request_id, started),
    }
}

/// Translates an error into the envelope; anything but a Hécate error becomes `internal`.
pub fn failed(request_id: Option<String>, error: &ServiceError, started: Instant) -> Envelope {
    let mut data = Map::new();
    let error = match error {
        ServiceError::Hecate(error) => {
            if let HecateError::StateConflict {
                state,
                current_version,
                ..
            } = error
            {
                if let Some(state) = state {
                    data.insert("state".into(), Value::from(state.as_str()));
                }
                if let Some(current_version) = current_version {
                    data.insert("current_version".into(), Value::from(*current_version));
                }
            }
            ErrorBody {
                code: error.code(),
                message: error.to_string(),
                retryable: error.retryable(),
                retry_after_s: match error {
                    HecateError::RateLimited { retry_after_s } => Some(*retry_after_s),
                    _ => None,
                },
            }
        }
        _ => ErrorBody {
            code: "internal",
            message: "internal error".into(),
            retryable: true,
            retry_after_s: None,
        },
    };
    Envelope {
        status: "failed",
        data: Value::Object(data),
        error: Some(error),
        meta: meta(request_id, started),
    }
}

/// One compact JSON line, in a buffer that is zeroed when dropped: replies carry shares.
pub fn encode(envelope: &Envelope) -> Zeroizing<Vec<u8>> {
    let mut line = Zeroizing::new(Vec::new());
    if serde_json::to_writer(ErasingWriter(&mut line), envelope).is_err() {
        line = Zeroizing::new(b"{}".to_vec());
    }
    append_erasing(&mut line, b"\n");
    line
}

/// Zeroes every text of a JSON value in place: passphrases, shares and secret values travel as
/// JSON texts, and serde_json does not erase them when it drops them.
pub fn erase(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(erase),
        Value::Object(fields) => fields.values_mut().for_each(erase),
        _ => {}
    }
}

/// Appends to a buffer that is zeroed when dropped; when it has to grow, the old allocation is
/// zeroed too instead of being freed with the bytes it held.
pub fn append_erasing(buffer: &mut Zeroizing<Vec<u8>>, bytes: &[u8]) {
    if buffer.capacity() - buffer.len() < bytes.len() {
        let wanted = (buffer.len() + bytes.len()).max(2 * buffer.capacity());
        let mut grown = Zeroizing::new(Vec::with_capacity(wanted));
        grown.extend_from_slice(buffer);
        *buffer = grown;
    }
    buffer.extend_from_slice(bytes);
}

/// A writer into a buffer that is zeroed when dropped, also when it grows.
pub struct ErasingWriter<'a>(pub &'a mut Zeroizing<Vec<u8>>);

impl io::Write for ErasingWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        append_erasing(self.0, bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;

    use super::*;

    #[test]
    fn erase_zeroes_every_text_of_a_nested_value_and_keeps_the_rest() {
        let mut value = json!({
            "passphrase": "a secret phrase",
            "proof": {"shares": ["HEC2-AAAA-BBBB", "HEC2-CCCC-DDDD"]},
            "version": 3,
            "flag": true,
            "missing": null,
        });
        erase(&mut value);
        assert_eq!(
            value,
            json!({
                "passphrase": "",
                "proof": {"shares": ["", ""]},
                "version": 3,
                "flag": true,
                "missing": null,
            })
        );
    }

    #[test]
    fn append_erasing_keeps_the_content_when_the_buffer_grows() {
        let mut buffer = Zeroizing::new(Vec::with_capacity(2));
        for chunk in [b"ab".as_slice(), b"cdef", b"", b"ghijklmnop"] {
            append_erasing(&mut buffer, chunk);
        }
        assert_eq!(buffer.as_slice(), b"abcdefghijklmnop");
    }

    #[test]
    fn the_erasing_writer_keeps_what_serde_json_writes() {
        let mut line = Zeroizing::new(Vec::new());
        serde_json::to_writer(ErasingWriter(&mut line), &json!({"op": "status"})).unwrap();
        let mut writer = ErasingWriter(&mut line);
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
        assert_eq!(line.as_slice(), b"{\"op\":\"status\"}\n");
    }
}
