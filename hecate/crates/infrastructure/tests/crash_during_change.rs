mod common;

use std::path::Path;
use std::sync::Arc;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{SealRepository, SealWork, SealWrites};
use hecate_application::sealing::service::SealingService;
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE, initialize, make_service};
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::protection::SealConfig;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use serde_json::{Value, json};

const NEW: &str = "a brand new passphrase";

fn proof() -> Value {
    json!({"passphrase": GOOD_PASSPHRASE})
}

/// Repository wrapper that writes the configuration and then fails inside the transaction.
struct PowerCutAfterSave(Arc<SqliteSealRepository>);

struct PowerCut<'a>(&'a mut dyn SealWrites);

impl SealWrites for PowerCut<'_> {
    fn load_config(&mut self) -> Result<Option<SealConfig>, ServiceError> {
        self.0.load_config()
    }

    fn save_config(&mut self, config: &SealConfig) -> Result<(), ServiceError> {
        self.0.save_config(config)?;
        Err(ServiceError::internal("PowerCut"))
    }

    fn load_attempts(&mut self) -> Result<AttemptPolicy, ServiceError> {
        self.0.load_attempts()
    }

    fn save_attempts(&mut self, attempts: &AttemptPolicy) -> Result<(), ServiceError> {
        self.0.save_attempts(attempts)
    }
}

impl SealRepository for PowerCutAfterSave {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError> {
        self.0.load_config()
    }

    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError> {
        self.0.load_attempts()
    }

    fn transaction(&self, work: &mut SealWork<'_>) -> Result<(), ServiceError> {
        self.0
            .transaction(&mut |writes| work(&mut PowerCut(writes)))
    }
}

fn boot(path: &Path, crashing: bool) -> (Arc<SqliteSealRepository>, SealingService) {
    let repo = Arc::new(SqliteSealRepository::open(path).unwrap());
    let log = Arc::new(SqliteAccessLog::open(path).unwrap());
    let used: Arc<dyn SealRepository> = if crashing {
        Arc::new(PowerCutAfterSave(repo.clone()))
    } else {
        repo.clone()
    };
    let service = make_service(crypto(), used, log, Arc::new(FakeClock::new()));
    (repo, service)
}

fn assert_old_protection_intact(path: &Path, shares: &[String]) {
    let (repo, service) = boot(path, false);
    assert_eq!(repo.load_config().unwrap().unwrap().config_version, 1);
    assert_eq!(
        service
            .unseal_with_passphrase(GOOD_PASSPHRASE)
            .unwrap()
            .state,
        SealState::Unsealed
    );
    service.seal().unwrap();
    assert_eq!(
        service.unseal_with_shares(&shares[..2]).unwrap().state,
        SealState::Unsealed
    );
}

fn power_cut(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Internal { kind: "PowerCut" })
}

// py: integration/test_crash_during_change.py::test_power_cut_inside_passphrase_change_keeps_the_previous_protection
#[test]
fn power_cut_inside_passphrase_change_keeps_the_previous_protection() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let shares = initialize(&boot(&db, false).1, GOOD_PASSPHRASE);

    let (_repo, crashing) = boot(&db, true);
    crashing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    assert!(power_cut(
        &crashing.change_passphrase(&proof(), NEW).unwrap_err()
    ));
    drop(crashing);

    assert_old_protection_intact(&db, &shares);
    let (_repo, service) = boot(&db, false);
    let error = service.unseal_with_passphrase(NEW).unwrap_err();
    assert!(matches!(
        error,
        ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    ));
}

// py: integration/test_crash_during_change.py::test_power_cut_inside_rekey_confirm_keeps_the_old_shares
#[test]
fn power_cut_inside_rekey_confirm_keeps_the_old_shares() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let shares = initialize(&boot(&db, false).1, GOOD_PASSPHRASE);

    let (_repo, crashing) = boot(&db, true);
    crashing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let begun = crashing.rekey_begin(&proof()).unwrap();
    let new_shares: Vec<String> = begun.shares.iter().map(|share| share.to_string()).collect();
    assert!(power_cut(
        &crashing
            .rekey_confirm(&begun.rekey_id, &new_shares[..2])
            .unwrap_err()
    ));
    drop(crashing);

    assert_old_protection_intact(&db, &shares);
}
