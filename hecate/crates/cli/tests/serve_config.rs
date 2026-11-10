//! `hecate serve` with an invalid value of the variables of the identities (research R15): a
//! configuration error, as in the change 005.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn serve_with_an_invalid_variable_prints_the_error_and_exits_with_2() {
    for (name, value) in [
        ("HECATE_API_ADDR", "nowhere"),
        ("HECATE_CERT_LIFETIME_HOURS", "73"),
        ("HECATE_ENROLL_TOKEN_MINUTES", "0"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        Command::new(env!("CARGO_BIN_EXE_hecate"))
            .env_remove("HECATE_SOCKET_PATH")
            .env_remove("HECATE_KDF_PROFILE")
            .env_remove("HECATE_API_ADDR")
            .env_remove("HECATE_CERT_LIFETIME_HOURS")
            .env_remove("HECATE_ENROLL_TOKEN_MINUTES")
            .env("HECATE_DATA_DIR", dir.path().join("data"))
            .env(name, value)
            .arg("serve")
            .assert()
            .code(2)
            .stderr(predicate::str::starts_with(format!("error: {name} must")));
        assert!(!dir.path().join("data").exists(), "{name}");
    }
}
