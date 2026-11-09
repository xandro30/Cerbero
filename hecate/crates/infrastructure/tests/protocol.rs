use std::time::Instant;

use hecate_application::errors::ServiceError;
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::control::protocol::{self, parse_request};
use serde_json::{Value, json};

fn rejection(raw: &[u8]) -> String {
    parse_request(raw).unwrap_err().to_string()
}

fn assert_in_order(text: &str, keys: &[&str]) {
    let positions: Vec<usize> = keys.iter().map(|key| text.find(key).unwrap()).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
}

#[test]
fn a_standard_json_request_is_read_with_its_op_args_and_request_id() {
    let request = parse_request(
        b"{\"request_id\":\"r1\",\"op\":\"secrets.get\",\"args\":{\"path\":\"secret://a/b\"}}\n",
    )
    .unwrap();
    assert_eq!(request.request_id.as_deref(), Some("r1"));
    assert_eq!(request.op, "secrets.get");
    assert_eq!(Value::Object(request.args), json!({"path": "secret://a/b"}));
    let bare = parse_request(b"{\"op\":\"status\",\"request_id\":null}").unwrap();
    assert_eq!(bare.request_id, None);
    assert!(bare.args.is_empty());
}

#[test]
fn input_that_is_not_standard_json_is_rejected() {
    let nested = format!(
        "{{\"op\":\"status\",\"args\":{{\"x\":{}{}}}}}",
        "[".repeat(128),
        "]".repeat(128)
    );
    let utf16: Vec<u8> = "{\"op\":\"status\"}"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let cases: Vec<Vec<u8>> = vec![
        b"{\"op\":\"status\",\"args\":{\"x\":NaN}}".to_vec(),
        b"{\"op\":\"status\",\"args\":{\"x\":Infinity}}".to_vec(),
        b"{\"op\":\"status\",\"args\":{\"x\":1e400}}".to_vec(),
        b"{\"op\":\"status\",\"args\":{\"x\":\"\\ud800\"}}".to_vec(),
        nested.into_bytes(),
        [b"\xef\xbb\xbf".as_slice(), b"{\"op\":\"status\"}"].concat(),
        utf16,
        b"{\"op\":\"\xff\"}".to_vec(),
        b"\n".to_vec(),
        Vec::new(),
    ];
    for raw in cases {
        assert_eq!(rejection(&raw), "request is not valid JSON", "{raw:?}");
    }
}

#[test]
fn the_other_shape_errors_keep_the_python_messages_and_order() {
    let op = "request must be an object with a string 'op'";
    assert_eq!(rejection(b"[]"), op);
    assert_eq!(rejection(b"{\"op\":1}"), op);
    assert_eq!(rejection(b"{\"op\":1,\"args\":[]}"), op);
    assert_eq!(
        rejection(b"{\"op\":\"x\",\"args\":[]}"),
        "'args' must be an object"
    );
    assert_eq!(
        rejection(b"{\"op\":\"x\",\"args\":null}"),
        "'args' must be an object"
    );
    assert_eq!(
        rejection(b"{\"op\":\"x\",\"args\":[],\"request_id\":5}"),
        "'args' must be an object"
    );
    assert_eq!(
        rejection(b"{\"op\":\"x\",\"request_id\":5}"),
        "'request_id' must be a string"
    );
}

#[test]
fn completed_and_failed_envelopes_keep_the_python_fields_in_order() {
    let started = Instant::now();
    let completed = protocol::completed(Some("r1".into()), json!({"state": "sealed"}), started);
    let text = String::from_utf8(protocol::encode(&completed).to_vec()).unwrap();
    assert!(text.ends_with('\n'));
    assert_in_order(&text, &["\"status\"", "\"data\"", "\"error\"", "\"meta\""]);
    assert_in_order(&text, &["\"request_id\"", "\"duration_ms\""]);
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["data"], json!({"state": "sealed"}));
    assert_eq!(value["error"], Value::Null);
    assert_eq!(value["meta"]["request_id"], "r1");
    assert!(value["meta"]["duration_ms"].is_u64());

    let limited = ServiceError::from(HecateError::RateLimited { retry_after_s: 60 });
    let text =
        String::from_utf8(protocol::encode(&protocol::failed(None, &limited, started)).to_vec())
            .unwrap();
    assert_in_order(
        &text,
        &[
            "\"code\"",
            "\"message\"",
            "\"retryable\"",
            "\"retry_after_s\"",
        ],
    );
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["status"], "failed");
    assert_eq!(value["data"], json!({}));
    assert_eq!(
        value["error"],
        json!({"code": "rate_limited", "message": "too many failed attempts; retry in 60s",
               "retryable": true, "retry_after_s": 60})
    );
    assert_eq!(value["meta"]["request_id"], Value::Null);

    let conflict = ServiceError::from(HecateError::state_conflict(
        Some(SealState::Sealed),
        None,
        Some(3),
    ));
    let value: Value = serde_json::from_slice(&protocol::encode(&protocol::failed(
        None, &conflict, started,
    )))
    .unwrap();
    assert_eq!(
        value["data"],
        json!({"state": "sealed", "current_version": 3})
    );
    assert_eq!(
        value["error"],
        json!({"code": "conflict", "message": "operation not allowed while sealed", "retryable": false})
    );
}

#[test]
fn an_internal_error_does_not_reveal_its_cause() {
    let failure = ServiceError::internal("SqliteFailure");
    let envelope = protocol::failed(Some("r".into()), &failure, Instant::now());
    let value: Value = serde_json::from_slice(&protocol::encode(&envelope)).unwrap();
    assert_eq!(
        value["error"],
        json!({"code": "internal", "message": "internal error", "retryable": true})
    );
    assert_eq!(value["data"], json!({}));
}
