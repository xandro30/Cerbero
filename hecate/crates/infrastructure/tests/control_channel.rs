mod common;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use common::*;
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE, MINIMAL_KDF, initialize};
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::testing::{ServerThread, SqliteStack, short_dir};
use serde_json::{Value, json};

fn prepared(base: &tempfile::TempDir) -> HecateConfig {
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    config
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn raw_request(socket: &Path, raw: &[u8]) -> Value {
    let mut stream = UnixStream::connect(socket).unwrap();
    stream.write_all(raw).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

// py: integration/test_control_channel.py::test_socket_is_private_and_status_works_when_uninitialized
#[test]
fn socket_is_private_and_status_works_when_uninitialized() {
    let base = short_dir();
    let config = prepared(&base);
    let _server = ServerThread::start(Arc::new(handler_for(rig().service)), &config.socket_path);
    assert_eq!(mode(&config.data_dir), 0o700);
    assert_eq!(mode(&config.socket_path), 0o600);

    let reply = ControlClient::new(&config.socket_path)
        .request("status", json!({}))
        .unwrap();
    assert_eq!(reply["status"], "completed");
    assert_eq!(reply["data"]["state"], "uninitialized");
    let mut keys: Vec<&str> = reply
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "error", "meta", "status"]);
    assert!(reply["meta"]["duration_ms"].as_u64().is_some());
}

// py: integration/test_control_channel.py::test_malformed_request_gets_validation_error_in_common_envelope
#[test]
fn malformed_request_gets_validation_error_in_common_envelope() {
    let base = short_dir();
    let config = prepared(&base);
    let _server = ServerThread::start(Arc::new(handler_for(rig().service)), &config.socket_path);
    for raw in [
        b"not json\n".as_slice(),
        b"{\"op\": 7}\n",
        b"{\"op\": \"nope\", \"args\": {}}\n",
    ] {
        let reply = raw_request(&config.socket_path, raw);
        assert_eq!(reply["status"], "failed");
        assert_eq!(reply["error"]["code"], "validation");
        assert_eq!(reply["error"]["retryable"], false);
        assert!(reply["error"].get("retry_after_s").is_none());
    }
}

// py: integration/test_control_channel.py::test_status_works_with_real_sqlite_from_worker_threads
#[test]
fn status_works_with_real_sqlite_from_worker_threads() {
    let base = short_dir();
    let config = prepared(&base);
    let stack = SqliteStack::open(&config.db_path(), Arc::new(FakeClock::new()));
    {
        let _server = ServerThread::start(
            Arc::new(request_handler(stack.sealing.clone(), None)),
            &config.socket_path,
        );
        let client = ControlClient::new(&config.socket_path);
        assert_eq!(
            client.request("status", json!({})).unwrap()["data"]["state"],
            "uninitialized"
        );
        assert_eq!(
            client.request("status", json!({})).unwrap()["status"],
            "completed"
        );
    }
    stack.close();
}

/// New (FR-018): a request that is not standard JSON is refused as a whole, before any
/// operation, and leaves no row in the access log (Python would read `NaN` and record a refusal).
#[test]
fn a_request_that_is_not_standard_json_leaves_no_access_log_row() {
    let base = short_dir();
    let config = prepared(&base);
    let stack = SqliteStack::open(&config.db_path(), Arc::new(FakeClock::new()));
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    let before = stack.access_log.entries().unwrap().len();
    let deep = format!(
        "{{\"request_id\": \"r\", \"op\": \"unseal\", \"args\": {{\"passphrase\": {}\"x\"{}}}}}\n",
        "[".repeat(200),
        "]".repeat(200)
    );
    {
        let _server = ServerThread::start(
            Arc::new(request_handler(stack.sealing.clone(), None)),
            &config.socket_path,
        );
        for raw in [
            b"{\"request_id\": \"r\", \"op\": \"unseal\", \"args\": {\"passphrase\": NaN}}\n".to_vec(),
            b"{\"request_id\": \"r\", \"op\": \"unseal\", \"args\": {\"passphrase\": \"\\ud800\"}}\n".to_vec(),
            b"{\"request_id\": \"r\", \"op\": \"unseal\", \"args\": {\"passphrase\": 1e999}}\n".to_vec(),
            deep.into_bytes(),
        ] {
            let reply = raw_request(&config.socket_path, &raw);
            assert_eq!(reply["status"], "failed");
            assert_eq!(reply["error"]["code"], "validation");
            assert_eq!(reply["error"]["message"], "request is not valid JSON");
        }
    }
    assert_eq!(stack.access_log.entries().unwrap().len(), before);
    stack.close();
}
