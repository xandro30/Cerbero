mod common;

use std::path::Path;
use std::sync::Arc;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::Clock;
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE, initialize};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::testing::SqliteStack;
use time::Duration;

fn boot(path: &Path, clock: &Arc<FakeClock>) -> SqliteStack {
    SqliteStack::open(path, clock.clone())
}

// py: integration/test_restart.py::test_after_unsealing_a_restart_leaves_hecate_sealed
#[test]
fn after_unsealing_a_restart_leaves_hecate_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let clock = Arc::new(FakeClock::new());
    let stack = boot(&db, &clock);
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    assert_eq!(stack.sealing.status().unwrap().state, SealState::Unsealed);
    stack.close();

    let restarted = boot(&db, &clock);
    assert_eq!(restarted.sealing.status().unwrap().state, SealState::Sealed);
    restarted
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    assert_eq!(
        restarted.sealing.status().unwrap().state,
        SealState::Unsealed
    );
    restarted.close();
}

// py: integration/test_restart.py::test_failed_attempts_and_pending_wait_survive_a_restart
#[test]
fn failed_attempts_and_pending_wait_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let clock = Arc::new(FakeClock::new());
    let stack = boot(&db, &clock);
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    for _ in 0..3 {
        let error = stack
            .sealing
            .unseal_with_passphrase("wrong wrong wrong")
            .unwrap_err();
        assert!(matches!(
            error,
            ServiceError::Hecate(HecateError::AuthenticationFailed(_))
        ));
    }
    stack.close();

    let restarted = boot(&db, &clock);
    let status = restarted.sealing.status().unwrap();
    assert_eq!((status.consecutive_failures, status.retry_after_s), (3, 60));
    let error = restarted
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap_err();
    assert!(matches!(
        error,
        ServiceError::Hecate(HecateError::RateLimited { .. })
    ));
    restarted.close();
}

// py: integration/test_restart.py::test_sc007_no_more_than_ten_evaluated_attempts_per_hour_even_restarting_and_alternating
#[test]
fn sc007_no_more_than_ten_evaluated_attempts_per_hour_even_restarting_and_alternating() {
    let rig = rig();
    initialize(&rig.service, GOOD_PASSPHRASE);
    let foreign = initialize(&common::rig().service, GOOD_PASSPHRASE);
    let start = rig.clock.now();
    let (mut evaluated, mut turn) = (0, 0);
    while rig.clock.now() - start <= Duration::hours(1) {
        let service = rig.restart();
        let outcome = if turn % 2 == 0 {
            service
                .unseal_with_passphrase("wrong wrong wrong")
                .map(|_| ())
        } else {
            service.unseal_with_shares(&foreign[..2]).map(|_| ())
        };
        match outcome {
            Ok(()) => panic!("a wrong proof must not unseal"),
            Err(ServiceError::Hecate(HecateError::AuthenticationFailed(_))) => evaluated += 1,
            Err(ServiceError::Hecate(HecateError::RateLimited { retry_after_s })) => {
                rig.clock
                    .advance(Duration::seconds(i64::try_from(retry_after_s).unwrap()));
                continue;
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
        turn += 1;
    }
    assert!(evaluated <= 10, "{evaluated}");
    assert!(evaluated >= 9, "{evaluated}");
}
