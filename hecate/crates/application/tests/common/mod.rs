#![allow(dead_code)]

use std::sync::Arc;

use hecate_application::errors::ServiceError;
use hecate_application::ports::CryptoProvider;
use hecate_application::sealing::service::SealingService;
use hecate_application::testing::{GOOD_PASSPHRASE, SealingRig, initialize};
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;

pub fn crypto() -> Arc<dyn CryptoProvider> {
    Arc::new(SoftwareCryptoProvider::new())
}

pub fn rig() -> SealingRig {
    SealingRig::new(crypto())
}

/// A rig already initialized: the service is sealed; returns the three shares too.
pub fn initialized() -> (SealingRig, Vec<String>) {
    let rig = rig();
    let shares = initialize(&rig.service, GOOD_PASSPHRASE);
    (rig, shares)
}

/// A rig initialized and unsealed with the passphrase.
pub fn unsealed() -> (SealingRig, Vec<String>) {
    let (rig, shares) = initialized();
    rig.service.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    (rig, shares)
}

pub fn fresh_service() -> Arc<SealingService> {
    rig().service
}

pub fn foreign_shares() -> Vec<String> {
    initialize(&fresh_service(), GOOD_PASSPHRASE)
}

pub fn is_invalid(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Hecate(HecateError::InvalidInput(_)))
}

pub fn is_auth(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    )
}

pub fn conflict_state(error: &ServiceError) -> Option<SealState> {
    match error {
        ServiceError::Hecate(HecateError::StateConflict { state, .. }) => *state,
        _ => None,
    }
}

pub fn retry_after(error: &ServiceError) -> Option<u64> {
    match error {
        ServiceError::Hecate(HecateError::RateLimited { retry_after_s }) => Some(*retry_after_s),
        _ => None,
    }
}

pub fn entries_of(rig: &SealingRig, operation: Operation) -> Vec<AccessLogEntry> {
    rig.log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .collect()
}

pub fn with_result(entries: &[AccessLogEntry], result: AccessResult) -> usize {
    entries
        .iter()
        .filter(|entry| entry.result == result)
        .count()
}

use hecate_application::ports::{EventStore, MaterialStore};
use hecate_application::secrets::service::{CreateOptions, Created, Resolved};
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::SecretsRig;
use hecate_domain::events::EventRecord;
use serde_json::{Value, json};

pub fn secrets() -> SecretsRig {
    SecretsRig::new(crypto(), true, SecretsSettings::default())
}

pub fn sealed_secrets() -> SecretsRig {
    SecretsRig::new(crypto(), false, SecretsSettings::default())
}

pub fn create(rig: &SecretsRig, path: &str, value: &str, value_type: &str) -> Created {
    rig.service
        .create(path, value, value_type, &CreateOptions::default())
        .unwrap()
}

pub fn stream_of(path: &str) -> String {
    format!("secret/{}", path.trim_start_matches("secret://"))
}

pub fn events(rig: &SecretsRig, stream: &str) -> Vec<EventRecord> {
    rig.store.load(stream).unwrap()
}

pub fn kinds(records: &[EventRecord]) -> Vec<String> {
    records.iter().map(|record| record.kind.clone()).collect()
}

/// The number of events of a secret, read from the store without leaving a row.
pub fn current(rig: &SecretsRig, path: &str) -> Value {
    json!(events(rig, &stream_of(path)).len())
}

pub fn has_material(rig: &SecretsRig, path: &str, version: u64) -> bool {
    rig.store
        .load_secret_material(path, version)
        .unwrap()
        .is_some()
}

pub fn states(rig: &SecretsRig, path: &str) -> Vec<(u64, &'static str)> {
    rig.service
        .get(path)
        .unwrap()
        .versions
        .iter()
        .map(|version| (version.version, version.state.as_str()))
        .collect()
}

pub fn state_of(rig: &SecretsRig, path: &str, version: u64) -> &'static str {
    states(rig, path)
        .into_iter()
        .find(|(number, _)| *number == version)
        .unwrap()
        .1
}

pub fn resolve(
    rig: &SecretsRig,
    identity: &str,
    references: &[&str],
) -> Result<Vec<Resolved>, ServiceError> {
    rig.service.resolve(identity, &json!(references))
}

pub fn resolve_one(rig: &SecretsRig, reference: &str) -> Resolved {
    resolve(rig, "hermes", &[reference]).unwrap().remove(0)
}

pub fn can_resolve(rig: &SecretsRig, reference: &str) -> bool {
    match resolve(rig, "hermes", &[reference]) {
        Ok(_) => true,
        Err(error) if is_not_found(&error) => false,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

pub fn is_not_found(error: &ServiceError) -> bool {
    matches!(error, ServiceError::Hecate(HecateError::NotFound(_)))
}

pub fn is_conflict(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::Hecate(HecateError::StateConflict { .. })
    )
}

pub fn current_version(error: &ServiceError) -> Option<u64> {
    match error {
        ServiceError::Hecate(HecateError::StateConflict {
            current_version, ..
        }) => *current_version,
        _ => None,
    }
}

pub fn is_fingerprint(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 4
        && groups.iter().all(|group| {
            group.len() == 4
                && group
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

pub fn failures(rig: &SecretsRig) -> u32 {
    rig.sealing.status().unwrap().consecutive_failures
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
