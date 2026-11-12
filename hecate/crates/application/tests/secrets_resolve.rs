mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{Clock, CryptoError, CryptoProvider, MaterialStore, SecretStore};
use hecate_application::secrets::envelope::Envelope;
use hecate_application::secrets::service::{CallContext, Resolved};
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::SecretsRig;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sealing::state::SealState;
use hecate_domain::secrets::path::SecretPath;
use hecate_domain::secrets::secret::{Secret, SecretEvent, SecretOptions};
use hecate_domain::secrets::value_type::ValueType;
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use serde_json::{Value, json};

const TOKEN: &str = "secret://hermes/telegram/bot_token";
const PROXY: &str = "secret://hermes/certs/proxy";
const OTHER: &str = "secret://echidna/api_key";
const TOKEN_VALUE: &str = "hermes-token-value";
const PROXY_VALUE: &str = "{\"cert\": \"value\"}";
const OTHER_VALUE: &str = "echidna-key-value";
const MISSING: &str = "secret://hermes/does/not/exist";

fn rig() -> SecretsRig {
    let rig = secrets();
    create(&rig, TOKEN, TOKEN_VALUE, "string");
    create(&rig, PROXY, PROXY_VALUE, "json");
    create(&rig, OTHER, OTHER_VALUE, "string");
    rig.access_log.clear();
    rig
}

fn triples(rig: &SecretsRig, references: &[&str]) -> Vec<(String, u64, String)> {
    resolve(rig, "hermes", references)
        .unwrap()
        .into_iter()
        .map(|item| (item.path.clone(), item.version, item.value.to_string()))
        .collect()
}

// py: unit/application/test_secret_resolve.py::test_resolves_the_active_version_with_the_cache_times
#[test]
fn resolves_the_active_version_with_the_cache_times() {
    let rig = rig();
    let out = resolve(&rig, "hermes", &[TOKEN]).unwrap();
    let [item] = out.as_slice() else {
        panic!("one secret")
    };
    assert_eq!(
        (item.path.as_str(), item.version, item.value.as_str()),
        (TOKEN, 1, TOKEN_VALUE)
    );
    assert_eq!((item.cache_ttl_soft_s, item.cache_ttl_hard_s), (300, 3600));
}

// py: unit/application/test_secret_resolve.py::test_resolves_a_concrete_version
#[test]
fn resolves_a_concrete_version() {
    let rig = rig();
    assert_eq!(
        triples(&rig, &[&format!("{TOKEN}@1")]),
        [(TOKEN.into(), 1, TOKEN_VALUE.into())]
    );
}

// py: unit/application/test_secret_resolve.py::test_a_version_that_does_not_exist_is_not_found
#[test]
fn a_version_that_does_not_exist_is_not_found() {
    let rig = rig();
    let error = resolve(&rig, "hermes", &[&format!("{TOKEN}@2")])
        .err()
        .unwrap();
    assert!(is_not_found(&error));
}

// py: unit/application/test_secret_resolve.py::test_resolves_several_references_in_one_request_in_order
#[test]
fn resolves_several_references_in_one_request_in_order() {
    let rig = rig();
    assert_eq!(
        triples(&rig, &[PROXY, TOKEN, &format!("{PROXY}@1")]),
        [
            (PROXY.into(), 1, PROXY_VALUE.into()),
            (TOKEN.into(), 1, TOKEN_VALUE.into()),
            (PROXY.into(), 1, PROXY_VALUE.into()),
        ]
    );
}

// py: unit/application/test_secret_resolve.py::test_all_or_nothing
#[test]
fn all_or_nothing() {
    let rig = rig();
    let error = resolve(&rig, "hermes", &[TOKEN, MISSING]).err().unwrap();
    assert!(is_not_found(&error) && !error.to_string().contains(TOKEN_VALUE));
}

// py: unit/application/test_secret_resolve.py::test_another_identitys_secret_and_a_missing_one_are_indistinguishable
#[test]
fn another_identitys_secret_and_a_missing_one_are_indistinguishable() {
    let rig = rig();
    let refusals: Vec<ServiceError> = [
        OTHER.to_string(),
        MISSING.to_string(),
        format!("{OTHER}@1"),
        "secret://hermes/does/not/exist@1".to_string(),
    ]
    .iter()
    .map(|reference| resolve(&rig, "hermes", &[reference]).err().unwrap())
    .collect();
    let mut messages: Vec<String> = refusals.iter().map(ToString::to_string).collect();
    messages.dedup();
    assert_eq!(messages.len(), 1);
    for error in &refusals {
        let ServiceError::Hecate(error @ HecateError::NotFound(_)) = error else {
            panic!("{error:?}")
        };
        assert!(error.code() == "not_found" && !error.retryable());
    }
    assert!(
        ![OTHER, MISSING, OTHER_VALUE]
            .iter()
            .any(|text| messages[0].contains(text))
    );
}

// py: unit/application/test_secret_resolve.py::test_an_identity_never_resolves_outside_its_own_space
#[test]
fn an_identity_never_resolves_outside_its_own_space() {
    let rig = rig();
    create(&rig, "secret://hermes-eu/x", "eu-value", "string");
    assert!(is_not_found(
        &resolve(&rig, "hermes", &["secret://hermes-eu/x"])
            .err()
            .unwrap()
    ));
    assert!(is_not_found(
        &resolve(&rig, "hermes-eu", &[TOKEN]).err().unwrap()
    ));
    let out = resolve(&rig, "hermes-eu", &["secret://hermes-eu/x"]).unwrap();
    assert_eq!(out[0].value.as_str(), "eu-value");
}

// py: unit/application/test_secret_resolve.py::test_malformed_requests_are_invalid
#[test]
fn malformed_requests_are_invalid() {
    let rig = rig();
    let too_many: Vec<&str> = vec![TOKEN; 101];
    for references in [
        json!([]),
        json!("secret://hermes/x"),
        Value::Null,
        json!([5]),
        json!(["not a reference"]),
        json!([format!("{TOKEN}@0")]),
        json!(too_many),
    ] {
        let error = rig.service.resolve("hermes", &references).err().unwrap();
        assert!(is_invalid(&error), "{references}");
    }
}

// py: unit/application/test_secret_resolve.py::test_a_malformed_identity_is_invalid
#[test]
fn a_malformed_identity_is_invalid() {
    let rig = rig();
    for identity in ["", "Hermes", "he/rmes"] {
        assert!(
            is_invalid(&resolve(&rig, identity, &[TOKEN]).err().unwrap()),
            "{identity:?}"
        );
    }
}

// py: unit/application/test_secret_resolve.py::test_resolving_while_sealed_conflicts
#[test]
fn resolving_while_sealed_conflicts() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    assert!(is_conflict(
        &resolve(&rig, "hermes", &[TOKEN]).err().unwrap()
    ));
}

// py: unit/application/test_secret_resolve.py::test_every_resolution_is_logged_with_the_identity_as_subject_and_no_value
#[test]
fn every_resolution_is_logged_with_the_identity_as_subject_and_no_value() {
    let rig = rig();
    resolve(&rig, "hermes", &[TOKEN, PROXY]).unwrap();
    let rows = rig.secret_rows();
    let [first, second] = rows.as_slice() else {
        panic!("two rows: {rows:?}")
    };
    for (row, path) in [(first, TOKEN), (second, PROXY)] {
        assert_eq!(
            (
                row.operation,
                row.subject.as_deref(),
                row.method,
                row.result
            ),
            (
                Operation::SecretResolve,
                Some("hermes"),
                Method::Identity,
                AccessResult::Success
            )
        );
        assert_eq!((row.path.as_deref(), row.version), (Some(path), Some(1)));
    }
    let shown = format!("{:?}", rig.access_log.entries());
    let escaped = format!("{PROXY_VALUE:?}");
    assert!(
        !shown.contains(TOKEN_VALUE)
            && !shown.contains(PROXY_VALUE)
            && !shown.contains(&escaped[1..escaped.len() - 1])
    );
}

// py: unit/application/test_secret_resolve.py::test_a_rejection_is_logged_with_the_requested_path_and_logs_no_success
#[test]
fn a_rejection_is_logged_with_the_requested_path_and_logs_no_success() {
    let rig = rig();
    assert!(is_not_found(
        &resolve(&rig, "hermes", &[TOKEN, OTHER]).err().unwrap()
    ));
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (
            row.operation,
            row.subject.as_deref(),
            row.result,
            row.path.as_deref()
        ),
        (
            Operation::SecretResolve,
            Some("hermes"),
            AccessResult::NotFound,
            Some(OTHER)
        )
    );
}

// py: unit/application/test_secret_resolve.py::test_rejections_are_logged_the_same_for_foreign_and_missing_secrets
#[test]
fn rejections_are_logged_the_same_for_foreign_and_missing_secrets() {
    let rig = rig();
    for path in [OTHER, MISSING] {
        assert!(is_not_found(
            &resolve(&rig, "hermes", &[path]).err().unwrap()
        ));
    }
    let rows = rig.secret_rows();
    let [foreign, missing] = rows.as_slice() else {
        panic!("two rows: {rows:?}")
    };
    assert_eq!(
        (foreign.result, foreign.version),
        (missing.result, missing.version)
    );
}

// py: unit/application/test_secret_resolve.py::test_a_sealed_refusal_is_logged_too
#[test]
fn a_sealed_refusal_is_logged_too() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    assert!(is_conflict(
        &resolve(&rig, "hermes", &[TOKEN]).err().unwrap()
    ));
    assert_eq!(
        rig.secret_rows().last().unwrap().result,
        AccessResult::RejectedConflict
    );
}

// --- Feature 006: the resolution in two phases (T013) -------------------------------------------

const SPIFFE: &str = "spiffe://cerbero.test/hermes";

/// The crypto of the tests, counting what it decrypts: a scope key or a value.
struct Counting {
    inner: Arc<dyn CryptoProvider>,
    decrypted: AtomicUsize,
}

impl CryptoProvider for Counting {
    fn random_bytes(&self, n: usize) -> Vec<u8> {
        self.inner.random_bytes(n)
    }

    fn random_key(&self) -> SecretKey {
        self.inner.random_key()
    }

    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        opslimit: u64,
        memlimit: usize,
    ) -> Result<SecretKey, CryptoError> {
        self.inner.derive_key(passphrase, salt, opslimit, memlimit)
    }

    fn aead_encrypt(
        &self,
        key: &SecretKey,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        self.inner.aead_encrypt(key, plaintext, aad)
    }

    fn aead_decrypt(
        &self,
        key: &SecretKey,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, CryptoError> {
        self.decrypted.fetch_add(1, Ordering::SeqCst);
        self.inner.aead_decrypt(key, nonce, ciphertext, aad)
    }

    fn hmac_sha256(&self, key: &SecretKey, data: &[u8]) -> [u8; 32] {
        self.inner.hmac_sha256(key, data)
    }

    fn hkdf_sha256(&self, key: &SecretKey, info: &[u8]) -> SecretKey {
        self.inner.hkdf_sha256(key, info)
    }

    fn shamir_split(&self, secret: &SecretKey) -> Result<Vec<(u8, SecretBytes)>, CryptoError> {
        self.inner.shamir_split(secret)
    }

    fn shamir_combine(&self, shares: &[(u8, &[u8])]) -> Result<SecretKey, CryptoError> {
        self.inner.shamir_combine(shares)
    }
}

fn counting_rig() -> (SecretsRig, Arc<Counting>) {
    let counting = Arc::new(Counting {
        inner: crypto(),
        decrypted: AtomicUsize::new(0),
    });
    let rig = SecretsRig::new(counting.clone(), true, SecretsSettings::default());
    create(&rig, TOKEN, TOKEN_VALUE, "string");
    create(&rig, PROXY, PROXY_VALUE, "json");
    create(&rig, OTHER, OTHER_VALUE, "string");
    rig.access_log.clear();
    (rig, counting)
}

/// The caller of the API that the rows name: its SPIFFE ID, its identity and its serial.
fn caller() -> CallContext<'static> {
    CallContext {
        subject: SPIFFE,
        identity: Some("hermes"),
        certificate: Some("5f1c"),
    }
}

fn own(path: &SecretPath) -> bool {
    path.owner() == "hermes"
}

fn resolve_as_caller(rig: &SecretsRig, references: &[&str]) -> Result<Vec<Resolved>, ServiceError> {
    rig.service.resolve_for(&caller(), &own, &json!(references))
}

#[test]
fn the_resolution_of_a_caller_names_it_in_every_row() {
    let rig = rig();
    let concrete = format!("{PROXY}@1");
    let resolved = resolve_as_caller(&rig, &[TOKEN, &concrete]).unwrap();
    let references: Vec<String> = resolved.iter().map(|item| item.reference.clone()).collect();
    assert_eq!(references, [TOKEN.to_string(), concrete]);
    let rows = rig.secret_rows();
    assert_eq!(rows.len(), 2, "{rows:?}");
    for (row, path) in rows.iter().zip([TOKEN, PROXY]) {
        assert_eq!(
            (
                row.subject.as_deref(),
                row.identity.as_deref(),
                row.certificate.as_deref()
            ),
            (Some(SPIFFE), Some("hermes"), Some("5f1c"))
        );
        assert_eq!(
            (row.operation, row.method, row.result),
            (
                Operation::SecretResolve,
                Method::Identity,
                AccessResult::Success
            )
        );
        assert_eq!((row.path.as_deref(), row.version), (Some(path), Some(1)));
    }
}

#[test]
fn every_reference_is_checked_before_any_is_decrypted() {
    let (rig, counting) = counting_rig();
    let missing_version = format!("{PROXY}@2");
    let before = counting.decrypted.load(Ordering::SeqCst);
    for references in [
        [TOKEN, PROXY, MISSING],
        [TOKEN, PROXY, OTHER],
        [TOKEN, PROXY, missing_version.as_str()],
    ] {
        let error = resolve_as_caller(&rig, &references).err().unwrap();
        assert!(is_not_found(&error), "{references:?}");
    }
    assert_eq!(counting.decrypted.load(Ordering::SeqCst), before);
    resolve_as_caller(&rig, &[TOKEN, PROXY]).unwrap();
    assert!(counting.decrypted.load(Ordering::SeqCst) > before);
}

#[test]
fn a_refusal_leaves_one_row_with_the_first_reference_that_fails() {
    let rig = rig();
    let error = resolve_as_caller(&rig, &[TOKEN, OTHER, MISSING])
        .err()
        .unwrap();
    assert!(is_not_found(&error));
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (
            row.result,
            row.path.as_deref(),
            row.version,
            row.subject.as_deref()
        ),
        (AccessResult::NotFound, Some(OTHER), None, Some(SPIFFE))
    );
}

#[test]
fn a_secret_that_the_permission_shares_is_opened_with_its_owners_key() {
    let rig = rig();
    create(&rig, "secret://echidna/not_shared", "kept", "string");
    let shared = |path: &SecretPath| own(path) || path.to_string() == OTHER;
    let resolved = rig
        .service
        .resolve_for(&caller(), &shared, &json!([OTHER, TOKEN]))
        .unwrap();
    let values: Vec<&str> = resolved.iter().map(|item| item.value.as_str()).collect();
    assert_eq!(values, [OTHER_VALUE, TOKEN_VALUE]);
    let refused = rig
        .service
        .resolve_for(&caller(), &shared, &json!(["secret://echidna/not_shared"]))
        .err()
        .unwrap();
    assert!(is_not_found(&refused));
}

#[test]
fn inconsistent_data_is_not_found_and_a_repeated_reference_is_resolved_again() {
    let rig = rig();
    let repeated = resolve_as_caller(&rig, &[TOKEN, TOKEN]).unwrap();
    assert_eq!(repeated.len(), 2);
    assert_eq!(rig.secret_rows().len(), 2);
    rig.store
        .transaction(&mut |writes| writes.delete_secret_material(TOKEN, 1))
        .unwrap();
    assert!(is_not_found(
        &resolve_as_caller(&rig, &[TOKEN]).err().unwrap()
    ));
}

#[test]
fn sealed_the_resolution_of_a_caller_is_refused_with_its_row() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    let error = resolve_as_caller(&rig, &[TOKEN]).err().unwrap();
    assert_eq!(conflict_state(&error), Some(SealState::Sealed));
    let rows = rig.secret_rows();
    let [row] = rows.as_slice() else {
        panic!("one row: {rows:?}")
    };
    assert_eq!(
        (row.result, row.subject.as_deref()),
        (AccessResult::RejectedConflict, Some(SPIFFE))
    );
}

// py: unit/application/test_secret_resolve.py::test_what_is_resolved_is_exactly_what_was_written
#[test]
fn what_is_resolved_is_exactly_what_was_written() {
    let rig = rig();
    create(
        &rig,
        "secret://hermes/unicode",
        "pässwörd €\n\ttabs",
        "string",
    );
    assert_eq!(
        resolve_one(&rig, "secret://hermes/unicode").value.as_str(),
        "pässwörd €\n\ttabs"
    );
}

// --- Feature 006: a rotate_every saved before it was a duration (T049, FR-023) --------------------

const LEGACY: &str = "secret://hermes/legacy/password";
const LEGACY_VALUE: &str = "legacy-value";

/// A secret saved before `rotate_every` was a duration: its events carry the free text `90d`, and
/// its material is sealed with the scope key of hermes, as `create` stored it then.
fn store_legacy(rig: &SecretsRig) {
    let envelope = Envelope::new(crypto());
    let root = rig.sealing.root().unwrap();
    let wrapped = rig.store.load_key_material("hermes", 1).unwrap().unwrap();
    let scope_key = envelope
        .open_scope_key(&root, "hermes", 1, &wrapped)
        .unwrap();
    let material = envelope
        .seal_value(&scope_key, LEGACY, 1, "hermes", 1, LEGACY_VALUE.as_bytes())
        .unwrap();
    let options = SecretOptions {
        rotate_every: Some("90d".to_string()),
        ..SecretOptions::default()
    };
    let events = Secret::create(
        &SecretPath::parse(LEGACY).unwrap(),
        ValueType::String,
        rig.clock.now(),
        &envelope.fingerprint(&scope_key, LEGACY_VALUE.as_bytes()),
        1,
        options,
    )
    .unwrap();
    let records: Vec<_> = events.iter().map(SecretEvent::encode).collect();
    rig.store
        .transaction(&mut |writes| {
            writes.append(&stream_of(LEGACY), &records, 0)?;
            writes.save_secret_material(LEGACY, 1, &material)
        })
        .unwrap();
}

#[test]
fn a_secret_whose_rotate_every_is_not_a_duration_still_resolves() {
    let rig = rig();
    store_legacy(&rig);
    assert_eq!(
        rig.service.get(LEGACY).unwrap().rotate_every.as_deref(),
        Some("90d")
    );
    let resolved = resolve_one(&rig, LEGACY);
    assert_eq!(
        (
            resolved.path.as_str(),
            resolved.version,
            resolved.value.as_str()
        ),
        (LEGACY, 1, LEGACY_VALUE)
    );
}
