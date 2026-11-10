//! T098: `hecate ca renew-intermediate --root-key <path>` (contracts/cli.md): the copy of the root
//! CA key opens only in the CLI with the CA passphrase and signs the new intermediate; a copy
//! that does not open or holds another key aborts with its reason.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::backup::seal_backup;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::pki::RootCaKey;

const DOES_NOT_OPEN: &str =
    "error: the copy does not open: wrong passphrase or the copy of another CA";

fn copy_path(env: &CaRunning) -> PathBuf {
    env.base.path().join("root-ca.pem")
}

fn renew(env: &CaRunning, terminal: &mut Scripted, root_key: &Path) -> i32 {
    hecate(
        &env.config,
        terminal,
        &[
            "ca",
            "renew-intermediate",
            "--root-key",
            root_key.to_str().unwrap(),
        ],
    )
}

fn rows(env: &CaRunning) -> Vec<(AccessResult, String)> {
    env.rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaIntermediateRenew)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn row(detail: &str) -> (AccessResult, String) {
    (AccessResult::Success, detail.to_string())
}

fn active_version(env: &CaRunning) -> u64 {
    env.rig
        .service
        .authority()
        .unwrap()
        .unwrap()
        .active()
        .unwrap()
        .version
}

#[test]
fn the_renewal_signs_with_the_copy_and_prints_the_new_version() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![line(CA_PASSPHRASE)]);
    assert_eq!(
        renew(&env, &mut terminal, &copy_path(&env)),
        0,
        "{}",
        terminal.take_err()
    );
    let ca = env.rig.service.authority().unwrap().unwrap();
    let active = ca.active().unwrap();
    assert_eq!(active.version, 2);
    assert_eq!(
        terminal.take_out(),
        format!(
            "renewed intermediate version 2 until {}",
            isoformat(active.not_after)
        )
    );
    assert_eq!(rows(&env), vec![row("begin"), row("confirm")]);
}

#[test]
fn a_copy_that_does_not_open_aborts_with_wrong_passphrase_without_saying_why() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![line("another CA passphrase")]);
    assert_eq!(renew(&env, &mut terminal, &copy_path(&env)), 1);
    assert_eq!(terminal.take_err(), DOES_NOT_OPEN);
    let other = ca_running();
    create_ca(&other);
    let mut terminal = Scripted::new(vec![line(CA_PASSPHRASE)]);
    assert_eq!(renew(&env, &mut terminal, &copy_path(&other)), 1);
    assert_eq!(terminal.take_err(), DOES_NOT_OPEN);
    assert_eq!(active_version(&env), 1);
    assert_eq!(
        rows(&env),
        vec![
            row("begin"),
            row("abort: wrong_passphrase"),
            row("begin"),
            row("abort: wrong_passphrase"),
        ]
    );
}

#[test]
fn a_copy_that_opens_with_another_key_aborts_with_wrong_root_key() {
    let env = ca_running();
    create_ca(&env);
    let fingerprint = env.rig.service.status().unwrap().unwrap().root_fingerprint;
    let another = RootCaKey::generate().unwrap();
    let copy = seal_backup(
        &SoftwareCryptoProvider::new(),
        another.pkcs8(),
        CA_PASSPHRASE,
        env.config.kdf,
        &fingerprint,
    )
    .unwrap();
    let path = env.base.path().join("another-key.pem");
    fs::write(&path, copy).unwrap();
    let mut terminal = Scripted::new(vec![line(CA_PASSPHRASE)]);
    assert_eq!(renew(&env, &mut terminal, &path), 1);
    assert_eq!(
        terminal.take_err(),
        "error: the copy does not hold the key of the root CA of Hécate"
    );
    assert_eq!(active_version(&env), 1);
    assert_eq!(rows(&env), vec![row("begin"), row("abort: wrong_root_key")]);
}

#[test]
fn a_copy_that_cannot_be_read_ends_before_the_ceremony() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::default();
    assert_eq!(renew(&env, &mut terminal, Path::new("/nowhere")), 1);
    assert_eq!(
        terminal.take_err(),
        "error: cannot read /nowhere: not found"
    );
    assert!(rows(&env).is_empty());
}

#[test]
fn cancelling_aborts_with_cancelled_and_exits_with_1() {
    let env = ca_running();
    create_ca(&env);
    let mut terminal = Scripted::new(vec![Answer::Cancel]);
    assert_eq!(renew(&env, &mut terminal, &copy_path(&env)), 1);
    assert_eq!(terminal.take_out().lines().last(), Some("cancelled"));
    assert_eq!(active_version(&env), 1);
    assert_eq!(rows(&env), vec![row("begin"), row("abort: cancelled")]);
}

#[test]
fn neither_the_key_nor_the_passphrase_cross_the_channel_and_both_are_erased() {
    let env = ca_running();
    create_ca(&env);
    let created = env.recording.lines.lock().unwrap().len();
    let mut terminal = Scripted::new(vec![line(CA_PASSPHRASE)]);
    assert_eq!(renew(&env, &mut terminal, &copy_path(&env)), 0);
    let copy = fs::read_to_string(copy_path(&env)).unwrap();
    let body: String = copy
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let lines = env.recording.lines.lock().unwrap();
    assert_eq!(lines.len(), created + 2, "begin and confirm");
    for line in &lines[created..] {
        let text = String::from_utf8_lossy(line);
        assert!(!text.contains("PRIVATE KEY") && !text.contains("HECATE ROOT CA KEY"));
        assert!(!text.contains(&body[..40]));
        assert!(!text.contains(CA_PASSPHRASE));
    }
    // The opened copy lives in these types, which are erased when dropped.
    fn erased<T: zeroize::ZeroizeOnDrop>() {}
    erased::<RootCaKey>();
    erased::<SecretBytes>();
}
