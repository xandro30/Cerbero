//! T034: the lines of the `ca` block in `hecate status`, after the ones it already prints.

mod common;

use common::*;

#[test]
fn without_a_ca_status_says_ca_none() {
    let env = ca_running();
    let mut terminal = Scripted::default();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert_eq!(terminal.take_out(), "state: unsealed\nca: none");
}

#[test]
fn with_a_ca_status_shows_it_and_hecate_certificate_also_sealed() {
    let env = ca_running();
    create_ca(&env);
    let summary = env.rig.service.status().unwrap().unwrap();
    let own = summary.hecate_certificate.unwrap();
    let expected = |state: &str| {
        format!(
            "state: {state}\nca: {TRUST_DOMAIN} {}\nhecate certificate {} until {}\nidentities expiring within 24h: 0\nsuspicious renewals within 24h: 0\nenrollment throttled: no",
            summary.root_fingerprint,
            own.serial,
            hecate_domain::instant::isoformat(own.not_after)
        )
    };
    let mut terminal = Scripted::default();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert_eq!(terminal.take_out(), expected("unsealed"));
    env.rig.sealing.seal().unwrap();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert_eq!(terminal.take_out(), expected("sealed"));
}

#[test]
fn status_says_when_enrollments_are_throttled() {
    let env = ca_running();
    create_ca(&env);
    for _ in 0..10 {
        let refused = env.rig.identities.enroll(
            Some("k1"),
            &serde_json::json!({"token": "x", "csr": "x", "call_key": "x"}),
        );
        assert!(refused.is_err());
    }
    let mut terminal = Scripted::default();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    assert!(
        terminal.take_out().ends_with("\nenrollment throttled: yes"),
        "the throttle is the last line"
    );
}

#[test]
fn status_shows_the_days_left_of_the_intermediate_when_fewer_than_30() {
    let env = ca_running();
    create_ca(&env);
    env.rig.clock.advance(time::Duration::days(340));
    let days = env
        .rig
        .service
        .status()
        .unwrap()
        .unwrap()
        .intermediate_days_left
        .unwrap();
    let mut terminal = Scripted::default();
    assert_eq!(hecate(&env.config, &mut terminal, &["status"]), 0);
    let out = terminal.take_out();
    let lines: Vec<&str> = out.lines().collect();
    let own = lines
        .iter()
        .position(|line| line.starts_with("hecate certificate "))
        .unwrap();
    assert_eq!(lines[own + 1], format!("intermediate days left: {days}"));
    assert!(lines[own + 2].starts_with("identities expiring within 24h: "));
}
