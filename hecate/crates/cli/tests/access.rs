//! T012: `hecate access grant|revoke|list|get` (contracts/cli.md), with `unchanged`,
//! `no access lists` and the errors.

mod common;

use common::*;
use hecate_application::ports::Clock;
use hecate_domain::instant::isoformat;

fn run(env: &CaRunning, args: &[&str]) -> (i32, String, String) {
    let mut terminal = Scripted::default();
    let code = hecate(&env.config, &mut terminal, args);
    (code, terminal.take_out(), terminal.take_err())
}

#[test]
fn the_orders_print_what_the_contract_says() {
    let env = ca_running();
    let cases = [
        (
            ["grant", "hades", "hermes"],
            "granted hades access to hermes (access version 1)",
        ),
        (
            ["grant", "hades", "hermes"],
            "unchanged: hades already calls hermes",
        ),
        (
            ["revoke", "hades", "hermes"],
            "revoked hades access to hermes (access version 2)",
        ),
        (
            ["revoke", "hades", "hermes"],
            "unchanged: hades does not call hermes",
        ),
    ];
    for (args, expected) in cases {
        let full: Vec<&str> = std::iter::once("access").chain(args).collect();
        let (code, out, err) = run(&env, &full);
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (0, expected, ""),
            "{full:?}"
        );
    }
}

#[test]
fn list_and_get_print_what_the_contract_says() {
    let env = ca_running();
    let (code, out, _) = run(&env, &["access", "list"]);
    assert_eq!((code, out.as_str()), (0, "no access lists"));
    run(&env, &["access", "grant", "hades", "hermes"]);
    run(&env, &["access", "grant", "caller", "hermes"]);
    run(&env, &["access", "grant", "hermes", "estigia"]);
    run(&env, &["access", "revoke", "hermes", "estigia"]);
    let at = isoformat(env.rig.clock.now());
    let (code, out, _) = run(&env, &["access", "list"]);
    assert_eq!(
        (code, out.as_str()),
        (0, "estigia version 2 callers 0\nhermes version 2 callers 2")
    );
    let (code, out, _) = run(&env, &["access", "get", "hermes"]);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!(
            "access hermes version 2\ncaller caller\ncaller hades\nhistory\n\
             1 {at} caller_granted hades by admin\n2 {at} caller_granted caller by admin"
        )
    );
    let (_, out, _) = run(&env, &["access", "get", "estigia"]);
    assert_eq!(
        out,
        format!(
            "access estigia version 2\nhistory\n1 {at} caller_granted hermes by admin\n\
             2 {at} caller_revoked hermes by admin"
        )
    );
    let (_, out, _) = run(&env, &["access", "get", "hades"]);
    assert_eq!(out, "access hades version 0");
}

#[test]
fn errors_are_printed_with_their_code() {
    let env = ca_running();
    for args in [
        &["access", "grant", "admin", "hermes"][..],
        &["access", "grant", "hades", "hecate"],
        &["access", "grant", "hermes", "hermes"],
        &["access", "revoke", "Hades", "hermes"],
        &["access", "grant", "hades/integration/x", "hermes"],
        &["access", "get", "admin"],
    ] {
        let (code, out, err) = run(&env, args);
        assert_eq!((code, out.as_str()), (1, ""), "{args:?}");
        assert!(err.starts_with("error (validation): "), "{args:?}: {err}");
    }
    env.rig.sealing.seal().unwrap();
    for args in [
        &["access", "grant", "hades", "hermes"][..],
        &["access", "revoke", "hades", "hermes"],
        &["access", "list"],
        &["access", "get", "hermes"],
    ] {
        let (code, _, err) = run(&env, args);
        assert_eq!(code, 1, "{args:?}");
        assert!(err.starts_with("error (conflict): "), "{args:?}: {err}");
    }
}
