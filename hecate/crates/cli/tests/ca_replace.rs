//! T107: `hecate ca replace --out <path> [--with-shares]` (contracts/cli.md): a proof and a new CA
//! passphrase, then the same exchange as `ca create`, copy check included; the errors and
//! cancelling.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use common::*;
use hecate_application::testing::GOOD_PASSPHRASE;
use hecate_domain::sealing::access_log::{AccessResult, Operation};

const NEW_CA_PASSPHRASE: &str = "harbor violet engine 77";

fn out_path(env: &CaRunning) -> PathBuf {
    env.base.path().join("root-ca-2.pem")
}

fn replace(env: &CaRunning, terminal: &mut Scripted, with_shares: bool) -> i32 {
    let out = out_path(env);
    let mut args = vec!["ca", "replace", "--out", out.to_str().unwrap()];
    if with_shares {
        args.push("--with-shares");
    }
    hecate(&env.config, terminal, &args)
}

fn rows(env: &CaRunning) -> Vec<(AccessResult, String)> {
    env.rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaReplace)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn row(result: AccessResult, detail: &str) -> (AccessResult, String) {
    (result, detail.to_string())
}

fn generation(env: &CaRunning) -> u64 {
    env.rig.service.authority().unwrap().unwrap().generation
}

/// The answers of a replacement that goes well, after the proof.
fn after_the_proof(env: &CaRunning) -> Vec<Answer> {
    vec![
        line(NEW_CA_PASSPHRASE),
        line(out_path(env).to_str().unwrap()),
        line(NEW_CA_PASSPHRASE),
    ]
}

#[test]
fn the_replacement_checks_the_copy_and_prints_the_new_generation() {
    let env = ca_running();
    create_ca(&env);
    let mut answers = vec![line(GOOD_PASSPHRASE)];
    answers.extend(after_the_proof(&env));
    let mut terminal = Scripted::new(answers);
    assert_eq!(
        replace(&env, &mut terminal, false),
        0,
        "{}",
        terminal.take_err()
    );
    let summary = env.rig.service.status().unwrap().unwrap();
    assert_eq!(
        terminal.take_out(),
        format!(
            "root key saved to {}; store it on two separate media\nreplaced ca for {TRUST_DOMAIN} generation 2\nroot fingerprint {}",
            out_path(&env).display(),
            summary.root_fingerprint
        )
    );
    assert_eq!(generation(&env), 2);
    let mode = fs::metadata(out_path(&env)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        rows(&env),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "confirm"),
        ]
    );
}

#[test]
fn two_recovery_shares_are_the_proof_with_with_shares() {
    let env = ca_running();
    create_ca(&env);
    let mut answers = vec![line(&env.rig.shares[0]), line(&env.rig.shares[2])];
    answers.extend(after_the_proof(&env));
    let mut terminal = Scripted::new(answers);
    assert_eq!(
        replace(&env, &mut terminal, true),
        0,
        "{}",
        terminal.take_err()
    );
    assert_eq!(generation(&env), 2);
}

#[test]
fn a_wrong_proof_or_a_short_ca_passphrase_change_nothing() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![line("not the passphrase"), line(NEW_CA_PASSPHRASE)]);
    assert_eq!(replace(&env, &mut terminal, false), 1);
    assert!(terminal.take_err().starts_with("error (auth): "));
    let mut terminal = Scripted::new(vec![line(GOOD_PASSPHRASE), line("too short")]);
    assert_eq!(replace(&env, &mut terminal, false), 1);
    assert!(terminal.take_err().starts_with("error (validation): "));
    assert!(!out_path(&env).exists());
    assert_eq!(generation(&env), 1);
    fs::write(out_path(&env), "keep me").unwrap();
    let mut terminal = Scripted::default();
    assert_eq!(replace(&env, &mut terminal, false), 1);
    assert!(terminal.take_err().contains("already exists"));
    assert_eq!(fs::read_to_string(out_path(&env)).unwrap(), "keep me");
}

#[test]
fn a_copy_that_does_not_open_aborts_and_removes_it() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![
        line(GOOD_PASSPHRASE),
        line(NEW_CA_PASSPHRASE),
        line(out_path(&env).to_str().unwrap()),
        line("another CA passphrase"),
    ]);
    assert_eq!(replace(&env, &mut terminal, false), 1);
    assert_eq!(
        terminal.take_err(),
        "error: the saved copy does not open with that passphrase"
    );
    assert!(!out_path(&env).exists());
    assert_eq!(generation(&env), 1);
    assert_eq!(
        rows(&env),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: backup_check_failed"),
        ]
    );
}

#[test]
fn cancelling_aborts_with_cancelled() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![
        line(GOOD_PASSPHRASE),
        line(NEW_CA_PASSPHRASE),
        Answer::Cancel,
    ]);
    assert_eq!(replace(&env, &mut terminal, false), 1);
    assert_eq!(terminal.take_out().lines().last(), Some("cancelled"));
    assert!(!out_path(&env).exists());
    assert_eq!(generation(&env), 1);
    assert_eq!(
        rows(&env),
        vec![
            row(AccessResult::Success, "begin"),
            row(AccessResult::Success, "abort: cancelled"),
        ]
    );
}
