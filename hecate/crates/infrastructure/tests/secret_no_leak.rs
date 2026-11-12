mod common;

use std::os::unix::fs::FileTypeExt;
use std::sync::{Arc, Mutex};

use common::*;
use data_encoding::{HEXLOWER, HEXUPPER};
use hecate_application::ports::EventStore;
use hecate_application::testing::{GOOD_PASSPHRASE, MINIMAL_KDF};
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::logs::render;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use serde_json::{Value, json};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const PATH: &str = "secret://hermes/telegram/bot_token";
const PEM_BODY: &str = "QkVHSU4tU0VOVElORUwtUEVNLUJPRFk=";

fn values() -> Vec<String> {
    (1..=3)
        .map(|n| format!("LEAK-SENTINEL-{n}-0123456789abcdef"))
        .collect()
}

fn pem() -> String {
    format!("-----BEGIN CERTIFICATE-----\n{PEM_BODY}\n-----END CERTIFICATE-----\n")
}

fn variants(value: &str) -> Vec<Vec<u8>> {
    let raw = value.as_bytes();
    vec![
        raw.to_vec(),
        HEXLOWER.encode(raw).into_bytes(),
        HEXUPPER.encode(raw).into_bytes(),
    ]
}

fn leaked(blob: &[u8], values: &[String]) -> Vec<Vec<u8>> {
    values
        .iter()
        .flat_map(|value| variants(value))
        .filter(|variant| contains(blob, variant))
        .collect()
}

// py: integration/test_secret_no_leak.py::test_the_check_itself_detects_a_planted_value
#[test]
fn the_check_itself_detects_a_planted_value() {
    let values = values();
    assert!(!leaked(format!("...{}...", values[0]).as_bytes(), &values).is_empty());
    assert!(!leaked(HEXLOWER.encode(values[1].as_bytes()).as_bytes(), &values).is_empty());
    assert!(leaked(b"nothing to see", &values).is_empty());
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        self.0.lock().unwrap().push(render(event));
    }
}

// py: integration/test_secret_no_leak.py::test_no_value_reaches_the_database_the_events_the_log_or_the_replies
#[test]
fn no_value_reaches_the_database_the_events_the_log_or_the_replies() {
    let capture = Capture::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(capture.clone())
            .with(LevelFilter::INFO),
    )
    .unwrap();
    let values = values();
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let stack = stack(&config.db_path());
    let mut replies = Vec::new();
    {
        let handler = Arc::new(request_handler(
            stack.sealing.clone(),
            Some(stack.service.clone()),
        ));
        let _server = ServerThread::start(handler, &config.socket_path);
        let client = ControlClient::new(&config.socket_path);
        let mut send = |op: &str, args: Value| {
            let reply = client.request(op, args).unwrap();
            replies.push(reply.to_string());
            reply
        };
        let begun = send("init.begin", json!({"passphrase": GOOD_PASSPHRASE}))["data"].clone();
        let shares = &begun["shares"].as_array().unwrap()[..2];
        send(
            "init.confirm",
            json!({"init_id": begun["init_id"], "shares": shares}),
        );
        send("unseal", json!({"passphrase": GOOD_PASSPHRASE}));
        send(
            "secrets.create",
            json!({"path": PATH, "value": values[0], "value_type": "string"}),
        );
        send(
            "secrets.create",
            json!({"path": "secret://hermes/certs/proxy", "value": pem(), "value_type": "pem"}),
        );
        send(
            "secrets.add_version",
            json!({"path": PATH, "value": values[1], "expected_version": 2}),
        );
        send(
            "secrets.add_version",
            json!({"path": PATH, "value": values[2], "expected_version": 4}),
        );
        send("secrets.list", json!({}));
        send("secrets.get", json!({"path": PATH}));
        send(
            "secrets.revoke",
            json!({"path": PATH, "version": 1, "expected_version": 6}),
        );
        send(
            "secrets.destroy",
            json!({"path": PATH, "version": 1, "expected_version": 7, "proof": {"passphrase": GOOD_PASSPHRASE}}),
        );
        send(
            "secrets.destroy",
            json!({"path": PATH, "version": 2, "expected_version": 8, "proof": {"passphrase": values[0]}}),
        );
        send(
            "secrets.create",
            json!({"path": PATH, "value": values[0], "value_type": "string"}),
        );
        send(
            "secrets.add_version",
            json!({"path": PATH, "value": "", "expected_version": values[0]}),
        );
        send("secrets.create", json!({"path": values[0], "value": 5}));
        send("status", json!({}));
    }
    // The resolution through its use case; the API that serves it has its own leak test,
    // `identity_no_leak.rs`.
    let resolved = stack
        .service
        .resolve("hermes", &json!([PATH, "secret://hermes/certs/proxy"]))
        .unwrap();
    assert_eq!(resolved[0].value.as_str(), values[2]);
    assert_eq!(resolved[1].value.as_str(), pem());
    stack.service.revoke(PATH, &json!(3), &json!(8)).unwrap();
    let mut events = Vec::new();
    for stream in stack.store.streams("").unwrap() {
        events.extend(stack.store.load(&stream).unwrap());
    }
    let log_rows = stack.access_log.entries().unwrap();
    stack.close();

    let mut database = Vec::new();
    for item in std::fs::read_dir(&config.data_dir).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_socket() {
            continue;
        }
        database.extend(std::fs::read(item.path()).unwrap());
    }
    let mut all = values.clone();
    all.push(pem());
    all.push(PEM_BODY.to_string());
    assert!(
        leaked(&database, &all).is_empty(),
        "a value is in the database files"
    );
    assert!(
        leaked(format!("{events:?}").as_bytes(), &all).is_empty(),
        "a value is in an event"
    );
    assert!(
        leaked(format!("{log_rows:?}").as_bytes(), &all).is_empty(),
        "a value is in the access log"
    );
    assert!(
        leaked(replies.join("\n").as_bytes(), &all).is_empty(),
        "a value is in a reply"
    );
    let lines = capture.0.lock().unwrap().clone();
    assert!(
        leaked(lines.join("\n").as_bytes(), &all).is_empty(),
        "a value is in the logs"
    );
    assert!(log_rows.len() > 10 && !lines.is_empty());
}

// py: integration/test_secret_no_leak.py::test_fingerprints_stay_out_of_the_log_and_events_carry_no_value_or_key_field
#[test]
fn fingerprints_stay_out_of_the_log_and_events_carry_no_value_or_key_field() {
    let base = short_dir();
    let stack = ready(&base.path().join("hecate.db"));
    let fingerprint = stack
        .service
        .create(PATH, &values()[0], "string", &no_options())
        .unwrap()
        .fingerprint;
    stack.service.get(PATH).unwrap();
    let rows = format!("{:?}", stack.access_log.entries().unwrap());
    assert!(!rows.contains(&fingerprint) && !rows.contains(&fingerprint.replace('-', "")));
    let mut events = Vec::new();
    for stream in stack.store.streams("").unwrap() {
        events.extend(stack.store.load(&stream).unwrap());
    }
    let forbidden = [
        "value",
        "key",
        "dek",
        "secret",
        "material",
        "ciphertext",
        "nonce",
    ];
    assert!(events.iter().all(|record| {
        record
            .data
            .keys()
            .all(|key| !forbidden.contains(&key.as_str()))
    }));
    assert!(events.iter().all(|record| {
        record
            .data
            .values()
            .all(|value| value.is_string() || value.is_u64() || value.is_i64() || value.is_null())
    }));
    stack.close();
}
