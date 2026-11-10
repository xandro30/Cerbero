//! T089: `hecate identities list`, `get`, `disable [--urgent]` and `enable`, and `hecate
//! certificates revoke <serial> [--urgent]` (contracts/cli.md), and their errors.

mod common;

use common::*;
use hecate_domain::instant::isoformat;
use hecate_infrastructure::testing::TestComponent;
use tempfile::TempDir;

fn run(env: &CaRunning, args: &[&str]) -> (i32, String, String) {
    let mut terminal = Scripted::default();
    let code = hecate(&env.config, &mut terminal, args);
    (code, terminal.take_out(), terminal.take_err())
}

/// A component enrolled through the service, with the directory that keeps its keys.
fn enrolled(env: &CaRunning, identity: &str) -> (TestComponent, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let component = TestComponent::open(dir.path(), env.rig.clock.clone()).unwrap();
    let token = env
        .rig
        .identities
        .tokens_create(identity)
        .unwrap()
        .token
        .to_string();
    let (body, key) = component.enrollment(&token).unwrap();
    let issued = env.rig.identities.enroll(Some(&key), &body).unwrap();
    component
        .accept_enrollment(&issued.certificate, &issued.chain)
        .unwrap();
    (component, dir)
}

fn own_serial(env: &CaRunning) -> String {
    env.rig
        .service
        .status()
        .unwrap()
        .unwrap()
        .hecate_certificate
        .unwrap()
        .serial
        .to_string()
}

#[test]
fn list_get_disable_and_enable_print_what_the_contract_says() {
    let env = ca_running();
    create_ca(&env);
    let (code, out, _) = run(&env, &["identities", "list"]);
    assert_eq!((code, out.as_str()), (0, "no identities"));
    let (hermes, _dir) = enrolled(&env, "hermes");
    let serial = hermes.serial().unwrap();
    let detail = env.rig.identities.identities_get("hermes").unwrap();
    let certificate = &detail.certificates[0];
    let (from, until) = (
        isoformat(certificate.not_before),
        isoformat(certificate.not_after),
    );
    let (code, out, _) = run(&env, &["identities", "list"]);
    assert_eq!(
        (code, out),
        (0, format!("hermes enrolled certificate until {until}"))
    );
    let (code, out, _) = run(&env, &["identities", "get", "hermes"]);
    assert_eq!(
        (code, out),
        (
            0,
            format!("identity hermes enrolled\n{serial} valid renewable yes {from} {until}")
        )
    );
    let (code, out, _) = run(&env, &["identities", "disable", "hermes"]);
    assert_eq!((code, out.as_str()), (0, "disabled hermes"));
    let (code, out, _) = run(&env, &["identities", "disable", "hermes", "--urgent"]);
    assert_eq!(
        (code, out.as_str()),
        (0, "disabled hermes\nrevoked 1 certificates")
    );
    let (code, out, _) = run(&env, &["identities", "get", "hermes"]);
    assert_eq!(
        (code, out),
        (
            0,
            format!("identity hermes disabled\n{serial} revoked renewable no {from} {until}")
        )
    );
    let (code, out, _) = run(&env, &["identities", "enable", "hermes"]);
    assert_eq!((code, out.as_str()), (0, "enabled hermes"));
}

#[test]
fn certificates_revoke_prints_the_serial_and_the_reissued_one() {
    let env = ca_running();
    create_ca(&env);
    let (hades, _dir) = enrolled(&env, "hades");
    let serial = hades.serial().unwrap();
    let (code, out, _) = run(&env, &["certificates", "revoke", serial.as_str()]);
    assert_eq!((code, out), (0, format!("revoked {serial}")));
    let (code, out, _) = run(
        &env,
        &["certificates", "revoke", serial.as_str(), "--urgent"],
    );
    assert_eq!((code, out), (0, format!("revoked {serial} urgent")));
    let own = own_serial(&env);
    let (code, out, _) = run(&env, &["certificates", "revoke", &own, "--urgent"]);
    let reissued = own_serial(&env);
    assert_ne!(reissued, own);
    assert_eq!(
        (code, out),
        (0, format!("revoked {own} urgent\nreissued {reissued}"))
    );
}

#[test]
fn errors_are_printed_with_their_code() {
    let env = ca_running();
    create_ca(&env);
    let (code, _, err) = run(&env, &["identities", "get", "nobody"]);
    assert_eq!(
        (code, err.as_str()),
        (1, "error (not_found): identity not found")
    );
    let (code, _, err) = run(&env, &["identities", "disable", "hecate", "--urgent"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (validation): "), "{err}");
    let own = own_serial(&env);
    let (code, _, err) = run(&env, &["certificates", "revoke", &own]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (validation): "), "{err}");
    let (code, _, err) = run(&env, &["certificates", "revoke", "not-a-serial"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (validation): "), "{err}");
    env.rig.sealing.seal().unwrap();
    let (code, _, err) = run(&env, &["identities", "list"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (conflict): "), "{err}");
}
