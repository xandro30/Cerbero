use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hecate_infrastructure::control::server::{LineHandler, peer_is_same_user, serve_connection};
use hecate_infrastructure::process::{Os, ProcessControls, harden_process_with};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn own_uid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

// py: unit/test_hardening.py::test_connections_from_the_same_user_are_accepted_on_linux
#[test]
fn connections_from_the_same_user_are_accepted_on_linux() {
    assert!(peer_is_same_user(Os::Linux, || Ok(own_uid()), own_uid()));
}

// py: unit/test_hardening.py::test_connections_from_another_user_are_rejected_on_linux
#[test]
fn connections_from_another_user_are_rejected_on_linux() {
    assert!(!peer_is_same_user(
        Os::Linux,
        || Ok(own_uid() + 1),
        own_uid()
    ));
}

// py: unit/test_hardening.py::test_other_platforms_rely_on_the_private_socket_permissions
#[test]
fn other_platforms_rely_on_the_private_socket_permissions() {
    assert!(peer_is_same_user(
        Os::Other,
        || Ok(own_uid() + 1),
        own_uid()
    ));
}

struct MustNotHandle(AtomicBool);

impl LineHandler for MustNotHandle {
    fn handle(&self, _raw: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
        self.0.store(true, Ordering::SeqCst);
        zeroize::Zeroizing::new(Vec::new())
    }
}

// py: unit/test_hardening.py::test_a_rejected_peer_is_closed_without_reading_or_handling
#[tokio::test]
async fn a_rejected_peer_is_closed_without_reading_or_handling() {
    let (server_end, mut client_end) = tokio::net::UnixStream::pair().unwrap();
    client_end
        .write_all(b"{\"op\":\"status\"}\n")
        .await
        .unwrap();
    let handler = Arc::new(MustNotHandle(AtomicBool::new(false)));
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    serve_connection(server_end, false, handler.clone(), lock).await;
    let mut answer = Vec::new();
    match client_end.read_to_end(&mut answer).await {
        Ok(_) => {}
        // Linux resets a Unix socket closed with unread data: the request was never read.
        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
        Err(error) => panic!("unexpected error: {error}"),
    }
    assert!(answer.is_empty());
    assert!(!handler.0.load(Ordering::SeqCst));
}

#[derive(Default)]
struct FakeControls {
    calls: Vec<&'static str>,
    lock_fails: bool,
}

impl ProcessControls for FakeControls {
    fn disable_core_dumps(&mut self) -> io::Result<()> {
        self.calls.push("core");
        Ok(())
    }

    fn lock_memory(&mut self) -> io::Result<()> {
        self.calls.push("lock");
        if self.lock_fails {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    }
}

// py: unit/test_hardening.py::test_core_dumps_are_disabled
#[test]
fn core_dumps_are_disabled() {
    let mut controls = FakeControls::default();
    harden_process_with(Os::Other, &mut controls);
    assert_eq!(controls.calls, vec!["core"]);
}

// py: unit/test_hardening.py::test_linux_locks_memory_and_warns_when_it_cannot
#[test]
fn linux_locks_memory_and_warns_when_it_cannot() {
    let mut locking = FakeControls::default();
    assert!(harden_process_with(Os::Linux, &mut locking).is_empty());
    assert_eq!(locking.calls, vec!["core", "lock"]);
    let mut failing = FakeControls {
        lock_fails: true,
        ..FakeControls::default()
    };
    let warnings = harden_process_with(Os::Linux, &mut failing);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("mlockall failed"))
    );
}

// py: unit/test_hardening.py::test_macos_warns_that_memory_is_not_locked
#[test]
fn macos_warns_that_memory_is_not_locked() {
    let warnings = harden_process_with(Os::Other, &mut FakeControls::default());
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("only available on Linux"))
    );
}
