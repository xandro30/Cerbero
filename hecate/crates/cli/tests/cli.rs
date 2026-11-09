//! The CLI commands called inside the test process, as in Python: a scripted terminal answers
//! the prompts and the server runs in a thread.

mod common;

use std::sync::Arc;

use common::*;
use hecate_application::testing::{GOOD_PASSPHRASE, MINIMAL_KDF, SealingRig, initialize};
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use tempfile::TempDir;

/// A running Hécate on in-memory storage; the fields drop in order: server, then directory.
struct Running {
    _server: ServerThread,
    config: HecateConfig,
    shares: Vec<String>,
    _base: TempDir,
}

fn running(initialized: bool) -> Running {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let rig = SealingRig::new(Arc::new(SoftwareCryptoProvider::new()));
    let shares = if initialized {
        initialize(&rig.service, GOOD_PASSPHRASE)
    } else {
        Vec::new()
    };
    let server = ServerThread::start(
        Arc::new(request_handler(rig.service.clone(), None)),
        &config.socket_path,
    );
    Running {
        _server: server,
        config,
        shares,
        _base: base,
    }
}

// py: integration/test_cli.py::test_init_flow_confirms_with_two_shares
#[test]
fn init_flow_confirms_with_two_shares() {
    let env = running(false);
    let mut terminal = Scripted::echoing_shares(vec![
        line("yes"),
        line(GOOD_PASSPHRASE),
        line(GOOD_PASSPHRASE),
    ]);
    assert_eq!(hecate(&env.config, &mut terminal, &["init"]), 0);
    let out = terminal.take_out();
    assert_eq!(shares_printed(&out).len(), 3);
    assert!(out.contains("initialized and sealed"));
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert!(terminal.take_out().contains("state: sealed"));
}

// py: integration/test_cli.py::test_init_requires_consent_and_matching_passphrases
#[test]
fn init_requires_consent_and_matching_passphrases() {
    let env = running(false);
    let mut terminal = Scripted::new(vec![line("no")]);
    assert_eq!(hecate(&env.config, &mut terminal, &["init"]), 1);
    terminal.script(vec![
        line("yes"),
        line(GOOD_PASSPHRASE),
        line("another passphrase"),
    ]);
    assert_eq!(hecate(&env.config, &mut terminal, &["init"]), 1);
    assert!(terminal.take_err().contains("do not match"));
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert!(terminal.take_out().contains("state: uninitialized"));
}

// py: integration/test_cli.py::test_interrupting_the_confirmation_aborts_the_initialization
#[test]
fn interrupting_the_confirmation_aborts_the_initialization() {
    let env = running(false);
    let mut terminal = Scripted::new(vec![
        line("yes"),
        line(GOOD_PASSPHRASE),
        line(GOOD_PASSPHRASE),
        Answer::Cancel,
    ]);
    assert_eq!(hecate(&env.config, &mut terminal, &["init"]), 1);
    assert!(terminal.take_err().contains("aborted"));
    terminal.take_out();
    hecate(&env.config, &mut terminal, &["status"]);
    assert!(terminal.take_out().contains("state: uninitialized"));
}

// py: integration/test_cli.py::test_cli_reports_when_hecate_is_not_running
#[test]
fn cli_reports_when_hecate_is_not_running() {
    let dir = short_dir();
    let config = HecateConfig::for_dir(dir.path(), MINIMAL_KDF);
    let mut terminal = Scripted::default();
    assert_eq!(hecate(&config, &mut terminal, &["status"]), 1);
    assert!(terminal.take_err().contains("cannot reach"));
}

// py: integration/test_cli.py::test_unseal_with_the_right_passphrase
#[test]
fn unseal_with_the_right_passphrase() {
    let env = running(true);
    let mut terminal = Scripted::new(vec![line(GOOD_PASSPHRASE)]);
    assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 0);
    assert!(terminal.take_out().contains("state: unsealed"));
}

// py: integration/test_cli.py::test_wrong_passphrases_then_the_wait_is_reported
#[test]
fn wrong_passphrases_then_the_wait_is_reported() {
    let env = running(true);
    let mut answers: Vec<Answer> = (0..3).map(|_| line("wrong wrong wrong")).collect();
    answers.push(line(GOOD_PASSPHRASE));
    let mut terminal = Scripted::new(answers);
    for _ in 0..3 {
        assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 1);
    }
    assert!(terminal.take_err().contains("error (auth)"));
    assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 1);
    let err = terminal.take_err();
    assert!(
        err.contains("error (rate_limited)") && err.contains("retry in 60s"),
        "{err}"
    );
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert!(terminal.take_out().contains("next attempt allowed in"));
}

// py: integration/test_cli.py::test_unseal_with_shares_recommends_changing_the_passphrase
#[test]
fn unseal_with_shares_recommends_changing_the_passphrase() {
    let env = running(true);
    let mut terminal = Scripted::new(vec![line(&env.shares[2]), line(&env.shares[0])]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["unseal", "--shares"]),
        0
    );
    let out = terminal.take_out();
    assert!(out.contains("state: unsealed") && out.contains("passphrase change --with-shares"));
}

// py: integration/test_cli.py::test_passphrase_change_with_shares_then_new_passphrase_unseals
#[test]
fn passphrase_change_with_shares_then_new_passphrase_unseals() {
    let env = running(true);
    let new = "a brand new passphrase";
    let mut terminal = Scripted::new(vec![line(&env.shares[0]), line(&env.shares[1])]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["unseal", "--shares"]),
        0
    );
    terminal.script(vec![
        line(&env.shares[1]),
        line(&env.shares[2]),
        line(new),
        line(new),
    ]);
    assert_eq!(
        hecate(
            &env.config,
            &mut terminal,
            &["passphrase", "change", "--with-shares"]
        ),
        0
    );
    assert!(terminal.take_out().contains("passphrase changed"));
    assert_eq!(hecate(&env.config, &mut terminal, &["seal"]), 0);
    terminal.script(vec![line(new)]);
    assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 0);
}

// py: integration/test_cli.py::test_passphrase_change_with_the_current_passphrase_and_mismatched_new_one
#[test]
fn passphrase_change_with_the_current_passphrase_and_mismatched_new_one() {
    let env = running(true);
    let mut terminal = Scripted::new(vec![line(GOOD_PASSPHRASE)]);
    hecate(&env.config, &mut terminal, &["unseal"]);
    terminal.script(vec![
        line(GOOD_PASSPHRASE),
        line("a brand new passphrase"),
        line("another new passphrase"),
    ]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["passphrase", "change"]),
        1
    );
    assert!(terminal.take_err().contains("do not match"));
}

// py: integration/test_cli.py::test_shares_rekey_flow_replaces_the_shares
#[test]
fn shares_rekey_flow_replaces_the_shares() {
    let env = running(true);
    let mut terminal = Scripted::echoing_shares(vec![line(GOOD_PASSPHRASE)]);
    // The prompt "Passphrase: " does not start with "Share", so it takes the scripted answer.
    assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 0);
    terminal.script(vec![line(GOOD_PASSPHRASE)]);
    assert_eq!(hecate(&env.config, &mut terminal, &["shares", "rekey"]), 0);
    let out = terminal.take_out();
    let new = shares_printed(&out);
    assert_eq!(new.len(), 3);
    assert!(new.iter().all(|share| !env.shares.contains(share)));
    assert!(out.contains("new recovery shares active"));
    assert_eq!(hecate(&env.config, &mut terminal, &["seal"]), 0);
    // The old ones no longer open it.
    let mut terminal = Scripted::new(vec![line(&env.shares[0]), line(&env.shares[1])]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["unseal", "--shares"]),
        1
    );
    terminal.script(vec![line(&new[0]), line(&new[2])]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["unseal", "--shares"]),
        0
    );
}

// py: integration/test_cli.py::test_interrupting_the_rekey_confirmation_keeps_the_old_shares
#[test]
fn interrupting_the_rekey_confirmation_keeps_the_old_shares() {
    let env = running(true);
    let mut terminal = Scripted::new(vec![line(GOOD_PASSPHRASE)]);
    hecate(&env.config, &mut terminal, &["unseal"]);
    terminal.script(vec![line(GOOD_PASSPHRASE), Answer::Cancel]);
    assert_eq!(hecate(&env.config, &mut terminal, &["shares", "rekey"]), 1);
    assert!(terminal.take_err().contains("old shares still work"));
    hecate(&env.config, &mut terminal, &["seal"]);
    terminal.script(vec![line(&env.shares[0]), line(&env.shares[1])]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["unseal", "--shares"]),
        0
    );
}

#[test]
fn cancelling_a_prompt_outside_the_share_confirmation_prints_cancelled_and_exits_with_1() {
    let env = running(true);
    let mut terminal = Scripted::new(vec![Answer::Cancel]);
    assert_eq!(hecate(&env.config, &mut terminal, &["unseal"]), 1);
    assert_eq!(terminal.take_out(), "cancelled");
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert!(terminal.take_out().contains("state: sealed"));
}
