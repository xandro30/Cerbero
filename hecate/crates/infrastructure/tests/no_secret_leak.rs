use std::os::unix::fs::FileTypeExt;
use std::sync::{Arc, Mutex};

use data_encoding::{HEXLOWER, HEXUPPER};
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE, MINIMAL_KDF};
use hecate_domain::sealing::share::RecoveryShare;
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::logs::render;
use hecate_infrastructure::testing::{ServerThread, SqliteStack, short_dir};
use serde_json::{Value, json};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const NEW: &str = "the second passphrase";
const WRONG: &str = "a wrong passphrase!!";

enum Secret {
    Text(String),
    Bytes(Vec<u8>),
}

fn variants(secret: &Secret) -> Vec<Vec<u8>> {
    let raw = match secret {
        Secret::Text(text) => text.as_bytes().to_vec(),
        Secret::Bytes(bytes) => bytes.clone(),
    };
    let mut variants = vec![
        HEXLOWER.encode(&raw).into_bytes(),
        HEXUPPER.encode(&raw).into_bytes(),
        raw,
    ];
    if let Secret::Text(text) = secret {
        variants.push(text.replace('-', "").into_bytes());
    }
    variants
}

fn contains(blob: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && blob.windows(needle.len()).any(|window| window == needle)
}

fn leaks(blob: &[u8], secrets: &[Secret]) -> Vec<Vec<u8>> {
    secrets
        .iter()
        .flat_map(variants)
        .filter(|variant| contains(blob, variant))
        .collect()
}

fn text(secret: &str) -> Secret {
    Secret::Text(secret.to_string())
}

// py: integration/test_no_secret_leak.py::test_the_check_itself_detects_a_planted_secret
#[test]
fn the_check_itself_detects_a_planted_secret() {
    let planted = [b"xx".as_slice(), GOOD_PASSPHRASE.as_bytes(), b"yy"].concat();
    assert!(!leaks(&planted, &[text(GOOD_PASSPHRASE)]).is_empty());
    let fragment = [
        b"fragment ".as_slice(),
        HEXLOWER.encode(&[0xab; 32]).as_bytes(),
    ]
    .concat();
    assert!(!leaks(&fragment, &[Secret::Bytes(vec![0xab; 32])]).is_empty());
    assert!(leaks(b"nothing here", &[text(GOOD_PASSPHRASE)]).is_empty());
}

/// Keeps every log line as the JSON lines logger writes it.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        self.0.lock().unwrap().push(render(event));
    }
}

fn shares(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|share| share.as_str().unwrap().to_string())
        .collect()
}

// py: integration/test_no_secret_leak.py::test_no_passphrase_share_or_root_reaches_the_database_the_log_or_the_replies
#[test]
fn no_passphrase_share_or_root_reaches_the_database_the_log_or_the_replies() {
    let capture = Capture::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(capture.clone())
            .with(LevelFilter::INFO),
    )
    .unwrap();
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let stack = SqliteStack::open(&config.db_path(), Arc::new(FakeClock::new()));
    let mut replies: Vec<String> = Vec::new();
    let mut shown: Vec<String> = Vec::new();
    let root: Vec<u8>;
    {
        let _server = ServerThread::start(
            Arc::new(request_handler(stack.sealing.clone(), None)),
            &config.socket_path,
        );
        let client = ControlClient::new(&config.socket_path);
        let mut ask = |op: &str, args: Value| {
            let reply = client.request(op, args).unwrap();
            replies.push(reply.to_string());
            reply
        };
        let begun = ask("init.begin", json!({"passphrase": GOOD_PASSPHRASE}))["data"].clone();
        let first = shares(&begun["shares"]);
        shown.extend(first.iter().cloned());
        ask(
            "init.confirm",
            json!({"init_id": begun["init_id"], "shares": &first[..2]}),
        );
        ask("unseal", json!({"passphrase": WRONG}));
        ask("unseal", json!({"passphrase": GOOD_PASSPHRASE}));
        // White-box: the one place the root legitimately lives.
        root = stack.sealing.root().unwrap().expose().to_vec();
        ask(
            "passphrase.change",
            json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW}),
        );
        let rekey =
            ask("shares.rekey.begin", json!({"proof": {"passphrase": NEW}}))["data"].clone();
        let second = shares(&rekey["shares"]);
        shown.extend(second.iter().cloned());
        ask(
            "shares.rekey.confirm",
            json!({"rekey_id": rekey["rekey_id"], "shares": &second[..2]}),
        );
        ask("seal", json!({}));
        let reply = ask("unseal", json!({"shares": &second[1..]}));
        assert_eq!(reply["data"]["state"], "unsealed");
    }
    stack.close();

    let mut secrets = vec![
        text(GOOD_PASSPHRASE),
        text(NEW),
        text(WRONG),
        Secret::Bytes(root.clone()),
    ];
    for share in &shown {
        secrets.push(Secret::Bytes(
            RecoveryShare::parse(share).unwrap().values().to_vec(),
        ));
        secrets.push(text(share));
    }
    for item in std::fs::read_dir(&config.data_dir).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_socket() {
            continue;
        }
        let blob = std::fs::read(item.path()).unwrap();
        assert!(
            leaks(&blob, &secrets).is_empty(),
            "secret found in {:?}",
            item.file_name()
        );
    }

    let lines = capture.0.lock().unwrap().clone();
    assert!(
        !lines.is_empty(),
        "the server should have logged its requests"
    );
    assert!(
        leaks(lines.join("\n").as_bytes(), &secrets).is_empty(),
        "secret found in the logs"
    );
    // The replies carry the shares to the admin on purpose; never the passphrase or the root.
    let never = [
        text(GOOD_PASSPHRASE),
        text(NEW),
        text(WRONG),
        Secret::Bytes(root),
    ];
    assert!(leaks(replies.concat().as_bytes(), &never).is_empty());
}
