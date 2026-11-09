use std::path::Path;
use std::sync::Arc;

use hecate_application::ports::SealRepository;
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE};
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::testing::SqliteStack;

fn build(path: &Path) -> SqliteStack {
    SqliteStack::open(path, Arc::new(FakeClock::new()))
}

// py: integration/test_init_crash.py::test_process_stopped_between_begin_and_confirm_stays_uninitialized
#[test]
fn process_stopped_between_begin_and_confirm_stays_uninitialized() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = build(&db);
    let begun = stack.sealing.init_begin(GOOD_PASSPHRASE).unwrap();
    assert!(!begun.init_id.is_empty());
    // The process dies here: nothing was confirmed and the pending init lived only in memory.
    stack.close();

    let restarted = build(&db);
    assert_eq!(
        restarted.sealing.status().unwrap().state,
        SealState::Uninitialized
    );
    assert_eq!(restarted.repo.load_config().unwrap(), None);
    let fresh = restarted.sealing.init_begin(GOOD_PASSPHRASE).unwrap();
    let shares: Vec<String> = fresh.shares.iter().map(|share| share.to_string()).collect();
    restarted
        .sealing
        .init_confirm(&fresh.init_id, &shares[..2])
        .unwrap();
    assert_eq!(restarted.sealing.status().unwrap().state, SealState::Sealed);
    restarted.close();
}

// py: integration/test_init_crash.py::test_confirmed_init_survives_a_restart_as_sealed
#[test]
fn confirmed_init_survives_a_restart_as_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = build(&db);
    let begun = stack.sealing.init_begin(GOOD_PASSPHRASE).unwrap();
    let shares: Vec<String> = begun.shares.iter().map(|share| share.to_string()).collect();
    stack
        .sealing
        .init_confirm(&begun.init_id, &shares[..2])
        .unwrap();
    stack.close();

    let restarted = build(&db);
    assert_eq!(restarted.sealing.status().unwrap().state, SealState::Sealed);
    restarted.close();
}
