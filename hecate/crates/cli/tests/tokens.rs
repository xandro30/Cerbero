//! T054: `hecate tokens create`, `list` and `annul` (contracts/cli.md), and their errors.

mod common;

use common::*;

fn run(env: &CaRunning, args: &[&str]) -> (i32, String, String) {
    let mut terminal = Scripted::default();
    let code = hecate(&env.config, &mut terminal, args);
    (code, terminal.take_out(), terminal.take_err())
}

#[test]
fn create_list_and_annul_print_what_the_contract_says() {
    let env = ca_running();
    create_ca(&env);
    let (code, out, _) = run(&env, &["tokens", "create", "hermes"]);
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    let token = lines[0].strip_prefix("token ").unwrap();
    assert!(token.starts_with("HET1.hermes."), "{token}");
    let id = token.split('.').nth(2).unwrap();
    let pending = env.rig.identities.tokens_list().unwrap();
    let expires = hecate_domain::instant::isoformat(pending[0].expires_at);
    assert_eq!(
        lines[1],
        format!("id {id} identity hermes expires {expires}")
    );
    assert_eq!(lines[2], "this token is shown only once");

    let (code, out, _) = run(&env, &["tokens", "list"]);
    assert_eq!((code, out), (0, format!("{id} hermes expires {expires}")));
    let (code, out, _) = run(&env, &["tokens", "annul", id]);
    assert_eq!((code, out), (0, format!("annulled {id}")));
    let (code, out, _) = run(&env, &["tokens", "list"]);
    assert_eq!((code, out.as_str()), (0, "no pending tokens"));
}

#[test]
fn errors_are_printed_with_their_code() {
    let env = ca_running();
    let (code, _, err) = run(&env, &["tokens", "create", "hermes"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (conflict): "), "{err}");
    create_ca(&env);
    let (code, _, err) = run(&env, &["tokens", "create", "hecate"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("error (validation): "), "{err}");
    let (code, _, err) = run(&env, &["tokens", "annul", "0123456789abcdef"]);
    assert_eq!(
        (code, err.as_str()),
        (1, "error (not_found): token not found")
    );
}
