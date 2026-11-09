//! The `hecate secrets` commands (new tests: the Python version did not test them), inside the
//! test process with a scripted terminal and the server in a thread.

mod common;

use std::sync::Arc;

use common::*;
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::{GOOD_PASSPHRASE, MINIMAL_KDF, SecretsRig};
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use serde_json::json;
use tempfile::TempDir;

const PATH: &str = "secret://hermes/token";

/// A running, unsealed Hécate with secrets on in-memory storage; fields drop in order.
struct Running {
    _server: ServerThread,
    config: HecateConfig,
    rig: SecretsRig,
    base: TempDir,
}

fn running() -> Running {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let rig = SecretsRig::new(
        Arc::new(SoftwareCryptoProvider::new()),
        true,
        SecretsSettings::default(),
    );
    let handler = request_handler(rig.sealing.clone(), Some(rig.service.clone()));
    let server = ServerThread::start(Arc::new(handler), &config.socket_path);
    Running {
        _server: server,
        config,
        rig,
        base,
    }
}

fn resolved(env: &Running, path: &str) -> String {
    env.rig.service.resolve("hermes", &json!([path])).unwrap()[0]
        .value
        .to_string()
}

fn fingerprint_of(out: &str) -> String {
    out.rsplit("fingerprint ")
        .next()
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn create_list_and_get_print_what_the_python_cli_printed() {
    let env = running();
    let mut terminal = Scripted::new(vec![line("the-value")]);
    let code = hecate(
        &env.config,
        &mut terminal,
        &["secrets", "create", PATH, "--type", "string"],
    );
    assert_eq!(code, 0);
    let out = terminal.take_out();
    assert!(
        out.starts_with(&format!("created {PATH} version 1 fingerprint ")),
        "{out}"
    );
    let fingerprint = fingerprint_of(&out);
    assert_eq!(resolved(&env, PATH), "the-value");

    assert_eq!(hecate(&env.config, &mut terminal, &["secrets", "list"]), 0);
    assert_eq!(
        terminal.take_out(),
        format!("{PATH}  string  active 1  versions 1")
    );

    assert_eq!(
        hecate(&env.config, &mut terminal, &["secrets", "get", PATH]),
        0
    );
    let lines: Vec<String> = terminal.take_out().lines().map(str::to_string).collect();
    assert_eq!(
        lines[0],
        format!("{PATH}  (string, owner hermes, max versions 10)")
    );
    assert_eq!(
        lines[1],
        format!("  version 1  active  2026-11-07T12:00:00+00:00  fingerprint {fingerprint}")
    );
    assert_eq!(lines.len(), 2);
}

#[test]
fn a_value_from_a_file_has_its_line_breaks_converted_and_the_options_reach_hecate() {
    let env = running();
    let file = env.base.path().join("value.txt");
    std::fs::write(&file, "first\r\nsecond\rthird\n").unwrap();
    let mut terminal = Scripted::new(vec![]);
    let args = [
        "secrets",
        "create",
        PATH,
        "--type",
        "string",
        "--from-file",
        file.to_str().unwrap(),
        "--max-versions",
        "3",
        "--rotate-every",
        "30d",
        "--expires-at",
        "2027-01-01T00:00:00+00:00",
    ];
    assert_eq!(hecate(&env.config, &mut terminal, &args), 0);
    assert_eq!(resolved(&env, PATH), "first\nsecond\nthird\n");

    terminal.take_out();
    assert_eq!(
        hecate(
            &env.config,
            &mut terminal,
            &["secrets", "list", "--identity", "hermes"]
        ),
        0
    );
    assert_eq!(
        terminal.take_out(),
        format!("{PATH}  string  active 1  versions 1 expires 2027-01-01T00:00:00+00:00")
    );
    assert_eq!(
        hecate(&env.config, &mut terminal, &["secrets", "get", PATH]),
        0
    );
    let out = terminal.take_out();
    assert!(out.contains("max versions 3") && out.contains("rotate every: 30d"));
    assert!(out.contains("expires at: 2027-01-01T00:00:00+00:00"));
}

#[test]
fn a_file_that_cannot_be_read_is_reported_with_its_reason() {
    let env = running();
    let missing = env.base.path().join("missing.txt");
    let not_utf8 = env.base.path().join("latin1.txt");
    std::fs::write(&not_utf8, b"contrase\xf1a").unwrap();
    let directory = env.base.path().to_path_buf();
    for (file, reason) in [
        (&missing, "not found"),
        (&directory, "is a directory"),
        (&not_utf8, "not valid UTF-8"),
    ] {
        let mut terminal = Scripted::new(vec![]);
        let path = file.to_str().unwrap();
        let code = hecate(
            &env.config,
            &mut terminal,
            &[
                "secrets",
                "create",
                PATH,
                "--type",
                "string",
                "--from-file",
                path,
            ],
        );
        assert_eq!(code, 1);
        assert_eq!(
            terminal.take_err(),
            format!("error: cannot read {path}: {reason}")
        );
    }
    assert!(env.rig.store.is_empty());
}

#[test]
fn put_revoke_restore_promote_and_destroy_print_what_was_done() {
    let env = running();
    let mut terminal = Scripted::new(vec![line("first-value")]);
    hecate(
        &env.config,
        &mut terminal,
        &["secrets", "create", PATH, "--type", "string"],
    );
    terminal.take_out();

    terminal.script(vec![line("second-value")]);
    assert_eq!(
        hecate(&env.config, &mut terminal, &["secrets", "put", PATH]),
        0
    );
    let out = terminal.take_out();
    assert!(
        out.starts_with(&format!("added version 2 to {PATH} fingerprint ")),
        "{out}"
    );
    assert_eq!(resolved(&env, PATH), "second-value");

    for (command, version, done) in [
        ("revoke", "1", "revoked"),
        ("restore", "1", "restored"),
        ("promote", "1", "promoted"),
        ("revoke", "2", "revoked"),
    ] {
        assert_eq!(
            hecate(
                &env.config,
                &mut terminal,
                &["secrets", command, PATH, "--version", version]
            ),
            0
        );
        assert_eq!(
            terminal.take_out(),
            format!("version {version} of {PATH} {done}")
        );
    }

    terminal.script(vec![line(GOOD_PASSPHRASE)]);
    assert_eq!(
        hecate(
            &env.config,
            &mut terminal,
            &["secrets", "destroy", PATH, "--version", "2"]
        ),
        0
    );
    assert_eq!(
        terminal.take_out(),
        format!(
            "Destroying version 2 of {PATH} erases its value for good.\nversion 2 of {PATH} destroyed"
        )
    );
}

#[test]
fn destroy_with_shares_asks_for_two_recovery_shares() {
    let env = running();
    let mut terminal = Scripted::new(vec![line("first-value")]);
    hecate(
        &env.config,
        &mut terminal,
        &["secrets", "create", PATH, "--type", "string"],
    );
    hecate(
        &env.config,
        &mut terminal,
        &["secrets", "revoke", PATH, "--version", "1"],
    );
    terminal.take_out();
    terminal.script(vec![line(&env.rig.shares[0]), line(&env.rig.shares[2])]);
    let code = hecate(
        &env.config,
        &mut terminal,
        &[
            "secrets",
            "destroy",
            PATH,
            "--version",
            "1",
            "--with-shares",
        ],
    );
    assert_eq!(code, 0);
    assert!(
        terminal
            .take_out()
            .ends_with(&format!("version 1 of {PATH} destroyed"))
    );
}

#[test]
fn errors_exit_with_1_and_the_python_message() {
    let env = running();
    let mut terminal = Scripted::new(vec![line("value")]);
    assert_eq!(
        hecate(
            &env.config,
            &mut terminal,
            &["secrets", "get", "secret://hermes/missing"]
        ),
        1
    );
    assert_eq!(terminal.take_err(), "error (not_found): secret not found");
    let code = hecate(
        &env.config,
        &mut terminal,
        &["secrets", "create", "hermes/token", "--type", "string"],
    );
    assert_eq!(code, 1);
    assert_eq!(
        terminal.take_err(),
        "error (validation): a secret path starts with secret://"
    );
    assert_eq!(
        hecate(
            &env.config,
            &mut terminal,
            &["secrets", "revoke", PATH, "--version", "1"]
        ),
        1
    );
    assert_eq!(terminal.take_err(), "error (not_found): secret not found");
    env.rig.sealing.seal().unwrap();
    assert_eq!(hecate(&env.config, &mut terminal, &["secrets", "list"]), 1);
    assert_eq!(
        terminal.take_err(),
        "error (conflict): operation not allowed while sealed [state: sealed]"
    );
}

#[test]
fn cancelling_the_value_prompt_creates_nothing() {
    let env = running();
    let mut terminal = Scripted::new(vec![Answer::Cancel]);
    let code = hecate(
        &env.config,
        &mut terminal,
        &["secrets", "create", PATH, "--type", "string"],
    );
    assert_eq!(code, 1);
    assert_eq!(terminal.take_out(), "cancelled");
    assert!(env.rig.store.is_empty());
}

#[test]
fn list_without_secrets_and_status_with_the_expiry_counts() {
    let env = running();
    let mut terminal = Scripted::new(vec![line("value")]);
    assert_eq!(hecate(&env.config, &mut terminal, &["secrets", "list"]), 0);
    assert_eq!(terminal.take_out(), "no secrets");
    hecate(
        &env.config,
        &mut terminal,
        &[
            "secrets",
            "create",
            PATH,
            "--type",
            "string",
            "--expires-at",
            "2026-11-09T12:00:00+00:00",
        ],
    );
    terminal.take_out();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert_eq!(
        terminal.take_out(),
        "state: unsealed\nsecrets expired: 0\nsecrets expiring in the next 7 days: 1"
    );
}
