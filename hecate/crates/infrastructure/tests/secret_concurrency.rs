mod common;

use std::sync::{Arc, Barrier};

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, MINIMAL_KDF};
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use serde_json::{Value, json};

const PATH: &str = "secret://hermes/telegram/bot_token";

/// Sends the same kind of request from two clients at once and returns both replies.
fn at_once(
    config: &HecateConfig,
    stack: &hecate_infrastructure::testing::SqliteStack,
    op: &'static str,
    args: [Value; 2],
) -> Vec<Value> {
    let handler = Arc::new(request_handler(
        stack.sealing.clone(),
        Some(stack.service.clone()),
    ));
    let _server = ServerThread::start(handler, &config.socket_path);
    let barrier = Arc::new(Barrier::new(2));
    let attempts: Vec<_> = args
        .into_iter()
        .map(|args| {
            let barrier = barrier.clone();
            let socket = config.socket_path.clone();
            std::thread::spawn(move || {
                let client = ControlClient::new(socket);
                barrier.wait();
                client.request(op, args).unwrap()
            })
        })
        .collect();
    attempts
        .into_iter()
        .map(|attempt| attempt.join().unwrap())
        .collect()
}

fn statuses(replies: &[Value]) -> Vec<&str> {
    let mut statuses: Vec<&str> = replies
        .iter()
        .map(|reply| reply["status"].as_str().unwrap())
        .collect();
    statuses.sort_unstable();
    statuses
}

// py: integration/test_secret_concurrency.py::test_two_new_versions_with_the_same_expected_version_one_wins_and_the_other_conflicts
#[test]
fn two_new_versions_with_the_same_expected_version_one_wins_and_the_other_conflicts() {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let stack = ready(&config.db_path());
    stack
        .service
        .create(PATH, "value-one", "string", &no_options())
        .unwrap();
    let replies = at_once(
        &config,
        &stack,
        "secrets.add_version",
        [
            json!({"path": PATH, "value": "value-a", "expected_version": 2}),
            json!({"path": PATH, "value": "value-b", "expected_version": 2}),
        ],
    );
    assert_eq!(statuses(&replies), ["completed", "failed"]);
    let loser = replies
        .iter()
        .find(|reply| reply["status"] == "failed")
        .unwrap();
    assert_eq!(loser["error"]["code"], "conflict");
    assert_eq!(loser["data"], json!({"current_version": 4}));
    let got = stack.service.get(PATH).unwrap();
    let versions: Vec<u64> = got.versions.iter().map(|version| version.version).collect();
    assert!(got.stream_version == 4 && versions == [1, 2]);
    let raw = rusqlite::Connection::open(config.db_path()).unwrap();
    let materials: i64 = raw
        .query_row("SELECT COUNT(*) FROM secret_material", [], |row| row.get(0))
        .unwrap();
    assert_eq!(materials, 2);
    stack.close();
}

// py: integration/test_secret_concurrency.py::test_two_destroys_of_the_same_version_one_wins
#[test]
fn two_destroys_of_the_same_version_one_wins() {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let stack = ready(&config.db_path());
    stack
        .service
        .create(PATH, "value-one", "string", &no_options())
        .unwrap();
    stack.service.revoke(PATH, &json!(1), &json!(2)).unwrap();
    let destroy = json!({"path": PATH, "version": 1, "expected_version": 3, "proof": {"passphrase": GOOD_PASSPHRASE}});
    let replies = at_once(
        &config,
        &stack,
        "secrets.destroy",
        [destroy.clone(), destroy],
    );
    assert_eq!(statuses(&replies), ["completed", "failed"]);
    let loser = replies
        .iter()
        .find(|reply| reply["status"] == "failed")
        .unwrap();
    assert_eq!(loser["error"]["code"], "conflict");
    stack.close();
}
