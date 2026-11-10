//! T032: `hecate ca create` (contracts/cli.md): the CLI keeps the root CA key, writes the copy
//! protected with the CA passphrase, checks a saved copy before confirming, and aborts with its
//! reason when the copy does not open or the admin cancels.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use common::*;
use hecate_domain::sealing::access_log::{AccessResult, Operation};

fn out_path(env: &CaRunning) -> PathBuf {
    env.base.path().join("root-ca.pem")
}

fn rows(env: &CaRunning) -> Vec<(AccessResult, String)> {
    env.rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaCreate)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn create(env: &CaRunning, terminal: &mut Scripted) -> i32 {
    let out = out_path(env);
    hecate(
        &env.config,
        terminal,
        &[
            "ca",
            "create",
            "--trust-domain",
            TRUST_DOMAIN,
            "--out",
            out.to_str().unwrap(),
        ],
    )
}

#[test]
fn the_ceremony_writes_a_private_copy_checks_it_and_creates_the_ca() {
    let env = ca_running();
    let out = out_path(&env);
    let mut terminal = Scripted::new(vec![
        line(CA_PASSPHRASE),
        line(out.to_str().unwrap()),
        line(CA_PASSPHRASE),
    ]);
    assert_eq!(create(&env, &mut terminal), 0, "{}", terminal.take_err());
    let printed = terminal.take_out();
    let summary = env.rig.service.status().unwrap().unwrap();
    assert_eq!(
        printed,
        format!(
            "root key saved to {}; store it on two separate media\ncreated ca for {TRUST_DOMAIN}\nroot fingerprint {}",
            out.display(),
            summary.root_fingerprint
        )
    );
    let mode = fs::metadata(&out).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let copy = fs::read_to_string(&out).unwrap();
    assert!(
        copy.starts_with("-----BEGIN HECATE ROOT CA KEY-----\n"),
        "{copy}"
    );
    assert_eq!(
        rows(&env),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "confirm".to_string()),
        ]
    );
}

#[test]
fn a_copy_that_does_not_open_aborts_and_creates_nothing() {
    let env = ca_running();
    let out = out_path(&env);
    let mut terminal = Scripted::new(vec![
        line(CA_PASSPHRASE),
        line(out.to_str().unwrap()),
        line("another CA passphrase"),
    ]);
    assert_eq!(create(&env, &mut terminal), 1);
    assert_eq!(
        terminal.take_err(),
        "error: the saved copy does not open with that passphrase"
    );
    assert!(env.rig.service.status().unwrap().is_none());
    assert!(
        !out.exists(),
        "the copy of a CA that was not created is removed"
    );
    assert_eq!(
        rows(&env),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (
                AccessResult::Success,
                "abort: backup_check_failed".to_string()
            ),
        ]
    );
}

#[test]
fn an_existing_out_file_is_never_overwritten() {
    let env = ca_running();
    let out = out_path(&env);
    fs::write(&out, "keep me").unwrap();
    let mut terminal = Scripted::new(vec![line(CA_PASSPHRASE)]);
    assert_eq!(create(&env, &mut terminal), 1);
    assert!(terminal.take_err().contains("already exists"));
    assert_eq!(fs::read_to_string(&out).unwrap(), "keep me");
    assert!(env.rig.service.status().unwrap().is_none());
    assert!(rows(&env).is_empty());
}

#[test]
fn cancelling_aborts_with_cancelled_and_exits_with_1() {
    for answers in [
        vec![Answer::Cancel],
        vec![line(CA_PASSPHRASE), Answer::Cancel],
        vec![line(CA_PASSPHRASE), line("/nowhere"), Answer::Cancel],
    ] {
        let env = ca_running();
        let begun = answers.len() > 1;
        let mut terminal = Scripted::new(answers);
        assert_eq!(create(&env, &mut terminal), 1);
        assert_eq!(terminal.take_out().lines().last(), Some("cancelled"));
        assert!(env.rig.service.status().unwrap().is_none());
        assert!(!out_path(&env).exists());
        let expected = if begun {
            vec![
                (AccessResult::Success, "begin".to_string()),
                (AccessResult::Success, "abort: cancelled".to_string()),
            ]
        } else {
            Vec::new()
        };
        assert_eq!(rows(&env), expected);
    }
}

#[test]
fn no_request_carries_a_private_key_or_the_copy() {
    let env = ca_running();
    let out = out_path(&env);
    let mut terminal = Scripted::new(vec![
        line(CA_PASSPHRASE),
        line(out.to_str().unwrap()),
        line(CA_PASSPHRASE),
    ]);
    assert_eq!(create(&env, &mut terminal), 0);
    let copy = fs::read_to_string(&out).unwrap();
    let body: String = copy
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let lines = env.recording.lines.lock().unwrap();
    assert_eq!(lines.len(), 2);
    for line in lines.iter() {
        let text = String::from_utf8_lossy(line);
        assert!(!text.contains("PRIVATE KEY") && !text.contains("HECATE ROOT CA KEY"));
        assert!(!text.contains(&body[..40]));
    }
}
