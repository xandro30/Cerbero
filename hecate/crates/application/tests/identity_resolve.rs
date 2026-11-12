//! T014: the resolution of the API as a use case (US1): a component authenticated by its
//! certificate and a short call token resolves its own secrets, all or nothing, with the rows of
//! the resolution; the refusals before it leave theirs (FR-001 to FR-005, FR-015 to FR-017).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    Clock, EventRecord, EventStore, KeyMaterial, MaterialStore, SecretMaterial, SecretStore,
    SecretWork,
};
use hecate_application::sealing::service::SealingService;
use hecate_application::secrets::service::{CreateOptions, Resolved};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE, InMemorySecretStore};
use hecate_domain::identity::names::IdentityName;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use hecate_domain::secrets::path::SecretPath;
use hecate_infrastructure::api::envelope::ApiError;
use hecate_infrastructure::call_token::JwtCallTokenVerifier;
use hecate_infrastructure::pki::RcgenPki;
use serde_json::{Value, json};
use time::Duration;

const TOKEN: &str = "secret://hermes/telegram/bot_token";
const PROXY: &str = "secret://hermes/certs/proxy";
const HADES: &str = "secret://hades/smtp/password";
const MISSING: &str = "secret://hermes/does/not/exist";
const UNRESOLVABLE: &str = "secret://hermes/old/key";
const TOKEN_VALUE: &str = "hermes-token-value";

fn create(rig: &CaRig, path: &str, value: &str) {
    rig.secrets
        .create(path, value, "string", &CreateOptions::default())
        .unwrap();
}

fn expected(rig: &CaRig, path: &str) -> Value {
    json!(rig.secrets.get(path).unwrap().stream_version)
}

fn revoke(rig: &CaRig, path: &str, version: u64) {
    rig.secrets
        .revoke(path, &json!(version), &expected(rig, path))
        .unwrap();
}

/// The secrets of the tests: two of `hermes`, one of `hades`, and one of `hermes` whose only
/// version is revoked, so it has no resolvable version.
fn store(rig: &CaRig) {
    create(rig, TOKEN, TOKEN_VALUE);
    create(rig, PROXY, "proxy-value");
    create(rig, HADES, "hades-value");
    create(rig, UNRESOLVABLE, "old-value");
    revoke(rig, UNRESOLVABLE, 1);
}

/// A rig with a CA, the secrets of the tests and `hermes` enrolled, with an empty access log.
fn resolving() -> (CaRig, Component) {
    let (rig, _) = created_ca();
    store(&rig);
    let hermes = Component::enrolled(&rig, "hermes");
    rig.access_log.clear();
    (rig, hermes)
}

fn body(references: &[&str]) -> Value {
    json!({ "references": references })
}

fn resolve(
    rig: &CaRig,
    component: &Component,
    references: &[&str],
) -> Result<Vec<Resolved>, ServiceError> {
    rig.identities
        .resolve(&component.credentials(None), &body(references))
}

fn resolution_rows(rig: &CaRig) -> Vec<AccessLogEntry> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::SecretResolve)
        .collect()
}

fn triples(resolved: &[Resolved]) -> Vec<(String, u64, String)> {
    resolved
        .iter()
        .map(|item| (item.reference.clone(), item.version, item.value.to_string()))
        .collect()
}

/// The secret store of a test that seals Hécate the first time the resolution reads a material:
/// after the authentication and before the decryption.
struct SealsOnMaterial {
    inner: Arc<InMemorySecretStore>,
    sealing: Arc<SealingService>,
    armed: Arc<AtomicBool>,
}

impl EventStore for SealsOnMaterial {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        self.inner.load(stream)
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        self.inner.streams(prefix)
    }
}

impl MaterialStore for SealsOnMaterial {
    fn load_secret_material(
        &self,
        path: &str,
        version: u64,
    ) -> Result<Option<SecretMaterial>, ServiceError> {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.sealing.seal()?;
        }
        self.inner.load_secret_material(path, version)
    }

    fn load_key_material(
        &self,
        identity: &str,
        key_version: u64,
    ) -> Result<Option<KeyMaterial>, ServiceError> {
        self.inner.load_key_material(identity, key_version)
    }
}

impl SecretStore for SealsOnMaterial {
    fn transaction(&self, work: &mut SecretWork<'_>) -> Result<(), ServiceError> {
        self.inner.transaction(work)
    }
}

#[test]
fn a_component_resolves_its_active_and_concrete_versions_with_the_cache_times() {
    let (rig, hermes) = resolving();
    let concrete = format!("{PROXY}@1");
    let resolved = resolve(&rig, &hermes, &[TOKEN, &concrete]).unwrap();
    assert_eq!(
        triples(&resolved),
        [
            (TOKEN.to_string(), 1, TOKEN_VALUE.to_string()),
            (concrete, 1, "proxy-value".to_string()),
        ]
    );
    assert!(
        resolved
            .iter()
            .all(|item| (item.cache_ttl_soft_s, item.cache_ttl_hard_s) == (300, 3600))
    );
}

#[test]
fn a_deprecated_version_resolves_only_by_its_number_and_within_its_grace() {
    let (rig, hermes) = resolving();
    rig.secrets
        .add_version(TOKEN, "new-token", &expected(&rig, TOKEN))
        .unwrap();
    let first = format!("{TOKEN}@1");
    assert_eq!(
        triples(&resolve(&rig, &hermes, &[TOKEN, &first]).unwrap()),
        [
            (TOKEN.to_string(), 2, "new-token".to_string()),
            (first.clone(), 1, TOKEN_VALUE.to_string()),
        ]
    );
    rig.clock.advance(Duration::hours(25));
    assert!(is_not_found(
        &resolve(&rig, &hermes, &[&first]).err().unwrap()
    ));
    assert_eq!(resolve(&rig, &hermes, &[TOKEN]).unwrap()[0].version, 2);
}

#[test]
fn an_expired_secret_still_resolves() {
    let (rig, hermes) = resolving();
    let options = CreateOptions {
        expires_at: json!(isoformat(rig.clock.now() + Duration::hours(1))),
        ..CreateOptions::default()
    };
    rig.secrets
        .create("secret://hermes/expiring", "soon", "string", &options)
        .unwrap();
    rig.clock.advance(Duration::hours(2));
    let resolved = resolve(&rig, &hermes, &["secret://hermes/expiring"]).unwrap();
    assert_eq!(resolved[0].value.as_str(), "soon");
}

#[test]
fn revoked_and_destroyed_versions_are_not_found() {
    let (rig, hermes) = resolving();
    rig.secrets
        .add_version(PROXY, "proxy-2", &expected(&rig, PROXY))
        .unwrap();
    revoke(&rig, PROXY, 1);
    let first = format!("{PROXY}@1");
    assert!(is_not_found(
        &resolve(&rig, &hermes, &[&first]).err().unwrap()
    ));
    rig.secrets
        .destroy(
            PROXY,
            &json!(1),
            &expected(&rig, PROXY),
            &json!({"passphrase": GOOD_PASSPHRASE}),
        )
        .unwrap();
    assert!(is_not_found(
        &resolve(&rig, &hermes, &[&first]).err().unwrap()
    ));
}

#[test]
fn foreign_missing_and_unresolvable_secrets_are_the_same_refusal_all_or_nothing() {
    let (rig, hermes) = resolving();
    let refusals: Vec<ServiceError> = [HADES, MISSING, UNRESOLVABLE]
        .iter()
        .map(|reference| resolve(&rig, &hermes, &[reference]).err().unwrap())
        .collect();
    assert!(refusals.iter().all(is_not_found), "{refusals:?}");
    let messages: Vec<String> = refusals.iter().map(ToString::to_string).collect();
    assert!(
        messages.windows(2).all(|pair| pair[0] == pair[1]),
        "{messages:?}"
    );
    let mixed = resolve(&rig, &hermes, &[TOKEN, PROXY, MISSING])
        .err()
        .unwrap();
    assert!(is_not_found(&mixed) && !mixed.to_string().contains(TOKEN_VALUE));
}

#[test]
fn the_rows_name_the_caller_by_its_spiffe_id_identity_and_serial() {
    let (rig, hermes) = resolving();
    let serial = hermes.serial().to_string();
    let subject = spiffe("hermes").to_string();
    resolve(&rig, &hermes, &[TOKEN, PROXY]).unwrap();
    resolve(&rig, &hermes, &[TOKEN, HADES, MISSING])
        .err()
        .unwrap();
    let rows = resolution_rows(&rig);
    let summary: Vec<_> = rows
        .iter()
        .map(|row| (row.result, row.path.as_deref(), row.version))
        .collect();
    assert_eq!(
        summary,
        [
            (AccessResult::Success, Some(TOKEN), Some(1)),
            (AccessResult::Success, Some(PROXY), Some(1)),
            (AccessResult::NotFound, Some(HADES), None),
        ]
    );
    for row in &rows {
        assert_eq!(
            (
                row.subject.as_deref(),
                row.identity.as_deref(),
                row.certificate.as_deref(),
                row.method
            ),
            (
                Some(subject.as_str()),
                Some("hermes"),
                Some(serial.as_str()),
                Method::Identity
            )
        );
    }
}

#[test]
fn refusals_before_the_resolution_leave_their_rows() {
    let (rig, hermes) = resolving();
    let mut credentials = hermes.credentials(None);
    credentials.bearer = Some("not.a.token".into());
    let error = rig
        .identities
        .resolve(&credentials, &body(&[TOKEN]))
        .err()
        .unwrap();
    assert!(is_auth(&error), "{error:?}");
    let too_many = vec![TOKEN; 101];
    let (zero, leading_zero, ten_digits) = (
        format!("{TOKEN}@0"),
        format!("{TOKEN}@01"),
        format!("{TOKEN}@1000000000"),
    );
    for wrong in [
        json!({}),
        json!({"references": [TOKEN], "extra": 1}),
        json!([TOKEN]),
        body(&[]),
        body(&too_many),
        body(&["not a reference"]),
        body(&[&zero]),
        body(&[&leading_zero]),
        body(&[&ten_digits]),
    ] {
        let error = rig
            .identities
            .resolve(&hermes.credentials(None), &wrong)
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{wrong}");
    }
    let results: Vec<AccessResult> = resolution_rows(&rig).iter().map(|row| row.result).collect();
    assert_eq!(results.len(), 10, "{results:?}");
    assert_eq!(results[0], AccessResult::Failure);
    assert!(
        results[1..]
            .iter()
            .all(|result| *result == AccessResult::InvalidInput)
    );
    rig.sealing.seal().unwrap();
    let error = resolve(&rig, &hermes, &[TOKEN]).err().unwrap();
    assert!(matches!(ApiError::from(error), ApiError::Unavailable(_)));
    let last = resolution_rows(&rig).pop().unwrap();
    assert_eq!(
        (last.result, last.detail.as_str(), last.subject),
        (AccessResult::RejectedConflict, "sealed", None)
    );
}

#[test]
fn sealed_between_the_two_phases_is_unavailable_with_one_row() {
    let armed = Arc::new(AtomicBool::new(false));
    let trigger = armed.clone();
    let rig = CaRig::with_secret_store(
        crypto(),
        Arc::new(RcgenPki::new()),
        Arc::new(JwtCallTokenVerifier::new()),
        move |store, sealing| {
            Arc::new(SealsOnMaterial {
                inner: store,
                sealing,
                armed: trigger,
            })
        },
    );
    create_ca_on(&rig);
    store(&rig);
    let hermes = Component::enrolled(&rig, "hermes");
    rig.access_log.clear();
    armed.store(true, Ordering::SeqCst);
    let error = resolve(&rig, &hermes, &[TOKEN, PROXY]).err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    assert!(matches!(ApiError::from(error), ApiError::Unavailable(_)));
    let rows = resolution_rows(&rig);
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (row.result, row.subject.is_some()),
        (AccessResult::RejectedConflict, true)
    );
}

#[test]
fn the_policy_is_read_once_per_request() {
    let (rig, hermes) = resolving();
    let before = rig.store.loads_of("policy/hermes");
    resolve(&rig, &hermes, &[TOKEN, PROXY, TOKEN]).unwrap();
    assert_eq!(rig.store.loads_of("policy/hermes") - before, 1);
    resolve(&rig, &hermes, &[TOKEN, MISSING]).err().unwrap();
    assert_eq!(rig.store.loads_of("policy/hermes") - before, 2);
}

#[test]
fn the_values_are_wiped_when_dropped() {
    fn erased<T: zeroize::ZeroizeOnDrop>(_: &[T]) {}
    let (rig, hermes) = resolving();
    let resolved = resolve(&rig, &hermes, &[TOKEN]).unwrap();
    erased(&resolved);
}

// --- Feature 006, US2: the secrets that an exception shares (T024) ------------------------------

const NEW_CA_PASSPHRASE: &str = "harbor violet engine 77";

#[test]
fn a_shared_secret_resolves_with_its_owners_rules_and_nothing_else_of_the_owner() {
    let (rig, hermes) = resolving();
    create(&rig, "secret://hades/other/key", "other-value");
    rig.policies.grant_read("hermes", HADES).unwrap();
    assert_eq!(
        triples(&resolve(&rig, &hermes, &[HADES, TOKEN]).unwrap()),
        [
            (HADES.to_string(), 1, "hades-value".to_string()),
            (TOKEN.to_string(), 1, TOKEN_VALUE.to_string()),
        ]
    );
    let other = resolve(&rig, &hermes, &["secret://hades/other/key"]);
    assert!(is_not_found(&other.err().unwrap()));
    rig.secrets
        .add_version(HADES, "hades-2", &expected(&rig, HADES))
        .unwrap();
    let first = format!("{HADES}@1");
    assert_eq!(
        triples(&resolve(&rig, &hermes, &[HADES, &first]).unwrap()),
        [
            (HADES.to_string(), 2, "hades-2".to_string()),
            (first.clone(), 1, "hades-value".to_string()),
        ]
    );
    rig.clock.advance(Duration::hours(25));
    assert!(is_not_found(
        &resolve(&rig, &hermes, &[&first]).err().unwrap()
    ));
}

#[test]
fn revoking_an_exception_refuses_the_next_request() {
    let (rig, hermes) = resolving();
    rig.policies.grant_read("hermes", HADES).unwrap();
    assert!(resolve(&rig, &hermes, &[HADES]).is_ok());
    rig.policies.revoke_read("hermes", HADES).unwrap();
    assert!(is_not_found(
        &resolve(&rig, &hermes, &[HADES]).err().unwrap()
    ));
}

#[test]
fn a_shared_secret_without_resolvable_versions_is_not_found_and_its_exception_stays() {
    let (rig, hermes) = resolving();
    let old = "secret://hades/old/key";
    create(&rig, old, "old-hades-value");
    rig.policies.grant_read("hermes", old).unwrap();
    revoke(&rig, old, 1);
    assert!(is_not_found(&resolve(&rig, &hermes, &[old]).err().unwrap()));
    let policy = rig
        .policies
        .policy_of(&IdentityName::parse("hermes").unwrap())
        .unwrap();
    assert!(policy.can_read(&SecretPath::parse(old).unwrap()));
}

#[test]
fn disabling_the_owner_keeps_the_exception() {
    let (rig, hermes) = resolving();
    let _hades = Component::enrolled(&rig, "hades");
    rig.policies.grant_read("hermes", HADES).unwrap();
    rig.identities.identities_disable("hades", true).unwrap();
    let resolved = resolve(&rig, &hermes, &[HADES]).unwrap();
    assert_eq!(resolved[0].value.as_str(), "hades-value");
}

#[test]
fn the_identity_that_received_it_keeps_its_policy_when_disabled_or_after_a_new_ca() {
    let (rig, hermes) = resolving();
    rig.policies.grant_read("hermes", HADES).unwrap();
    rig.identities.identities_disable("hermes", true).unwrap();
    assert!(is_auth(&resolve(&rig, &hermes, &[HADES]).err().unwrap()));
    rig.identities.identities_enable("hermes").unwrap();
    let again = Component::enrolled(&rig, "hermes");
    let resolved = resolve(&rig, &again, &[HADES]).unwrap();
    assert_eq!(resolved[0].value.as_str(), "hades-value");
    // A new CA: the certificate of before no longer authenticates, and the policy stays.
    let begun = rig
        .service
        .replace_begin(&json!({"passphrase": GOOD_PASSPHRASE}), NEW_CA_PASSPHRASE)
        .unwrap();
    let signed = sign(&rig, &begun);
    rig.service
        .replace_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    assert!(is_auth(&resolve(&rig, &again, &[HADES]).err().unwrap()));
    let renewed = Component::enrolled(&rig, "hermes");
    let resolved = resolve(&rig, &renewed, &[HADES]).unwrap();
    assert_eq!(resolved[0].value.as_str(), "hades-value");
}

#[test]
fn a_request_with_own_and_shared_secrets_is_all_or_nothing() {
    let (rig, hermes) = resolving();
    rig.policies.grant_read("hermes", HADES).unwrap();
    let mixed = resolve(&rig, &hermes, &[TOKEN, HADES, MISSING]);
    assert!(is_not_found(&mixed.err().unwrap()));
    assert_eq!(resolve(&rig, &hermes, &[TOKEN, HADES]).unwrap().len(), 2);
}
