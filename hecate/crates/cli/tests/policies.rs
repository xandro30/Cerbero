//! T026: `hecate policies grant-read|revoke-read|grant-status|revoke-status|list|get`
//! (contracts/cli.md), with `unchanged`, `no policies` and the errors.

mod common;

use common::*;
use hecate_application::ports::Clock;
use hecate_application::secrets::service::CreateOptions;
use hecate_domain::instant::isoformat;

const SMTP: &str = "secret://hades/smtp/password";

fn run(env: &CaRunning, args: &[&str]) -> (i32, String, String) {
    let mut terminal = Scripted::default();
    let code = hecate(&env.config, &mut terminal, args);
    (code, terminal.take_out(), terminal.take_err())
}

fn with_secret() -> CaRunning {
    let env = ca_running();
    env.rig
        .secrets
        .create(SMTP, "smtp-value", "string", &CreateOptions::default())
        .unwrap();
    env
}

#[test]
fn the_orders_print_what_the_contract_says() {
    let env = with_secret();
    let cases: Vec<(Vec<&str>, String)> = vec![
        (
            vec!["grant-read", "hermes", SMTP],
            format!("granted read {SMTP} to hermes (policy version 1)"),
        ),
        (
            vec!["grant-read", "hermes", SMTP],
            format!("unchanged: hermes already reads {SMTP}"),
        ),
        (
            vec!["grant-status", "hermes"],
            "granted status to hermes (policy version 2)".to_string(),
        ),
        (
            vec!["grant-status", "hermes"],
            "unchanged: hermes already has status".to_string(),
        ),
        (
            vec!["revoke-status", "hermes"],
            "revoked status from hermes (policy version 3)".to_string(),
        ),
        (
            vec!["revoke-status", "hermes"],
            "unchanged: hermes has no status".to_string(),
        ),
        (
            vec!["revoke-read", "hermes", SMTP],
            format!("revoked read {SMTP} from hermes (policy version 4)"),
        ),
        (
            vec!["revoke-read", "hermes", SMTP],
            format!("unchanged: hermes does not read {SMTP}"),
        ),
    ];
    for (args, expected) in cases {
        let full: Vec<&str> = std::iter::once("policies").chain(args).collect();
        let (code, out, err) = run(&env, &full);
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (0, expected.as_str(), ""),
            "{full:?}"
        );
    }
}

#[test]
fn list_and_get_print_what_the_contract_says() {
    let env = with_secret();
    let (code, out, _) = run(&env, &["policies", "list"]);
    assert_eq!((code, out.as_str()), (0, "no policies"));
    run(&env, &["policies", "grant-read", "hermes", SMTP]);
    run(&env, &["policies", "grant-status", "zeus"]);
    let at = isoformat(env.rig.clock.now());
    let (code, out, _) = run(&env, &["policies", "list"]);
    assert_eq!(
        (code, out.as_str()),
        (
            0,
            "hermes version 1 status no reads 1\nzeus version 1 status yes reads 0"
        )
    );
    let (code, out, _) = run(&env, &["policies", "get", "hermes"]);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!(
            "policy hermes version 1 status no\nread {SMTP}\nhistory\n1 {at} read_granted {SMTP} by admin"
        )
    );
    let (_, out, _) = run(&env, &["policies", "get", "zeus"]);
    assert_eq!(
        out,
        format!("policy zeus version 1 status yes\nhistory\n1 {at} status_granted by admin")
    );
    let (_, out, _) = run(&env, &["policies", "get", "hades"]);
    assert_eq!(out, "policy hades version 0 status no");
}

#[test]
fn errors_are_printed_with_their_code() {
    let env = with_secret();
    let (code, _, err) = run(&env, &["policies", "grant-read", "admin", SMTP]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (validation): "), "{err}");
    let (code, _, err) = run(
        &env,
        &["policies", "grant-read", "hermes", "secret://hades/nothing"],
    );
    assert_eq!(
        (code, err.as_str()),
        (1, "error (not_found): secret not found")
    );
    env.rig.sealing.seal().unwrap();
    let (code, _, err) = run(&env, &["policies", "list"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (conflict): "), "{err}");
}
