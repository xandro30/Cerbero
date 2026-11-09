mod common;

use std::sync::{Arc, Barrier};

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, MINIMAL_KDF, initialize};
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use serde_json::{Value, json};

// py: integration/test_concurrent_unseal.py::test_two_simultaneous_unseals_one_wins_and_the_other_gets_conflict
#[test]
fn two_simultaneous_unseals_one_wins_and_the_other_gets_conflict() {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let rig = rig();
    initialize(&rig.service, GOOD_PASSPHRASE);
    let barrier = Arc::new(Barrier::new(2));
    let replies: Vec<Value> = {
        let _server = ServerThread::start(
            Arc::new(request_handler(rig.service.clone(), None)),
            &config.socket_path,
        );
        let attempts: Vec<_> = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let socket = config.socket_path.clone();
                std::thread::spawn(move || {
                    let client = ControlClient::new(socket);
                    barrier.wait();
                    client
                        .request("unseal", json!({"passphrase": GOOD_PASSPHRASE}))
                        .unwrap()
                })
            })
            .collect();
        attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect()
    };

    let mut statuses: Vec<&str> = replies
        .iter()
        .map(|reply| reply["status"].as_str().unwrap())
        .collect();
    statuses.sort_unstable();
    assert_eq!(statuses, ["completed", "failed"]);
    let loser = replies
        .iter()
        .find(|reply| reply["status"] == "failed")
        .unwrap();
    assert_eq!(loser["error"]["code"], "conflict");
    assert_eq!(loser["data"], json!({"state": "unsealed"}));
    assert_eq!(rig.service.status().unwrap().state, SealState::Unsealed);
}
