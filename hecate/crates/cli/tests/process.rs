//! The `hecate` binary as a process (new tests): exit codes, messages and an orderly stop.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command as Hecate;
use hecate_application::testing::MINIMAL_KDF;
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::testing::short_dir;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use predicates::prelude::*;

fn hecate() -> Hecate {
    let mut command = Hecate::new(env!("CARGO_BIN_EXE_hecate"));
    command
        .env_remove("HECATE_DATA_DIR")
        .env_remove("HECATE_SOCKET_PATH")
        .env_remove("HECATE_KDF_PROFILE");
    command
}

#[test]
fn without_hecate_data_dir_it_exits_with_2_and_an_error() {
    hecate()
        .arg("status")
        .assert()
        .code(2)
        .stderr(predicate::str::starts_with(
            "error: HECATE_DATA_DIR is required",
        ));
}

#[test]
fn an_unknown_command_or_option_exits_with_2() {
    hecate().arg("nope").assert().code(2);
    hecate().args(["status", "--nope"]).assert().code(2);
}

#[test]
fn without_the_channel_it_exits_with_1_and_an_error() {
    let dir = tempfile::tempdir().unwrap();
    hecate()
        .env("HECATE_DATA_DIR", dir.path())
        .arg("status")
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with("error: cannot reach Hécate"));
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(Instant::now() < deadline, "the socket did not appear");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn serve_answers_status_and_stops_in_order_with_sigterm() {
    let base = short_dir();
    let data_dir = base.path().join("data");
    let config = HecateConfig::for_dir(&data_dir, MINIMAL_KDF);
    let mut serve = Command::new(env!("CARGO_BIN_EXE_hecate"))
        .arg("serve")
        .env("HECATE_DATA_DIR", &data_dir)
        .env("HECATE_KDF_PROFILE", "moderate")
        .env_remove("HECATE_SOCKET_PATH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&config.socket_path);

    hecate()
        .env("HECATE_DATA_DIR", &data_dir)
        .arg("status")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("state: uninitialized"));

    kill(
        Pid::from_raw(i32::try_from(serve.id()).unwrap()),
        Signal::SIGTERM,
    )
    .unwrap();
    let status = serve.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(
        !config.socket_path.exists(),
        "the socket is removed on the way out"
    );
}

#[test]
fn serve_that_cannot_prepare_its_data_directory_exits_with_1_and_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, b"").unwrap();
    let data_dir = file.join("data");
    hecate()
        .env("HECATE_DATA_DIR", &data_dir)
        .arg("serve")
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with(format!(
            "error: cannot prepare {}: ",
            data_dir.display()
        )));
}
