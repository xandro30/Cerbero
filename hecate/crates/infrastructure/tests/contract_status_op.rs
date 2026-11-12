mod common;

use std::sync::Arc;

use common::*;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{SealRepository, SealWork};
use hecate_application::testing::{
    FakeClock, GOOD_PASSPHRASE, InMemoryAccessLog, initialize, make_service,
};
use hecate_domain::sealing::attempts::AttemptPolicy;
use hecate_domain::sealing::protection::SealConfig;
use serde_json::{Value, json};

const STATUS_FIELDS: [&str; 6] = [
    "state",
    "initialized_at",
    "unsealed_via",
    "recommend_passphrase_change",
    "consecutive_failures",
    "retry_after_s",
];

fn fields(data: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn status_fields() -> Vec<String> {
    let mut keys: Vec<String> = STATUS_FIELDS.iter().map(|key| key.to_string()).collect();
    keys.sort();
    keys
}

// py: contract/test_status_op.py::test_status_when_uninitialized
#[test]
fn status_when_uninitialized() {
    let reply = call(&handler_for(rig().service), "status", json!({}));
    assert!(reply["status"] == "completed" && reply["error"].is_null());
    assert_eq!(fields(&reply["data"]), status_fields());
    assert_eq!(
        reply["data"],
        json!({
            "state": "uninitialized", "initialized_at": null, "unsealed_via": null,
            "recommend_passphrase_change": false, "consecutive_failures": 0, "retry_after_s": 0,
        })
    );
}

// py: contract/test_status_op.py::test_status_when_sealed_unsealed_and_after_recovery
#[test]
fn status_when_sealed_unsealed_and_after_recovery() {
    let (_rig, handler, shares) = initialized_handler();
    let sealed = call(&handler, "status", json!({}))["data"].clone();
    assert_eq!(fields(&sealed), status_fields());
    assert_eq!(sealed["state"], "sealed");
    assert!(
        sealed["initialized_at"]
            .as_str()
            .is_some_and(|at| !at.is_empty())
    );
    assert!(sealed["unsealed_via"].is_null());

    call(&handler, "unseal", json!({"passphrase": GOOD_PASSPHRASE}));
    let unsealed = call(&handler, "status", json!({}))["data"].clone();
    assert_eq!(
        (
            &unsealed["state"],
            &unsealed["unsealed_via"],
            &unsealed["recommend_passphrase_change"]
        ),
        (&json!("unsealed"), &json!("passphrase"), &json!(false))
    );

    call(&handler, "seal", json!({}));
    call(&handler, "unseal", json!({"shares": &shares[..2]}));
    let recovered = call(&handler, "status", json!({}))["data"].clone();
    assert_eq!(
        (
            &recovered["unsealed_via"],
            &recovered["recommend_passphrase_change"]
        ),
        (&json!("recovery"), &json!(true))
    );
}

// py: contract/test_status_op.py::test_status_reports_failures_and_the_pending_wait
#[test]
fn status_reports_failures_and_the_pending_wait() {
    let (rig, handler, _) = initialized_handler();
    for _ in 0..3 {
        call(
            &handler,
            "unseal",
            json!({"passphrase": "wrong wrong wrong"}),
        );
    }
    let data = call(&handler, "status", json!({}))["data"].clone();
    assert_eq!(
        (
            data["consecutive_failures"].as_u64(),
            data["retry_after_s"].as_u64()
        ),
        (Some(3), Some(60))
    );
    rig.clock.advance(time::Duration::seconds(30));
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["retry_after_s"],
        30
    );
}

/// Storage that fails as a locked database would; the cause goes to the logs only by its kind.
struct BrokenRepository;

const CAUSE: &str = "secret detail: /var/lib/hecate/hecate.db is locked";

impl SealRepository for BrokenRepository {
    fn load_config(&self) -> Result<Option<SealConfig>, ServiceError> {
        Err(ServiceError::internal(CAUSE))
    }

    fn load_attempts(&self) -> Result<AttemptPolicy, ServiceError> {
        Err(ServiceError::internal(CAUSE))
    }

    fn transaction(&self, _work: &mut SealWork<'_>) -> Result<(), ServiceError> {
        Err(ServiceError::internal(CAUSE))
    }
}

// py: contract/test_status_op.py::test_internal_error_hides_the_cause_and_is_retryable
#[test]
fn internal_error_hides_the_cause_and_is_retryable() {
    for op in ["status", "seal"] {
        let service = make_service(
            crypto(),
            Arc::new(BrokenRepository),
            Arc::new(InMemoryAccessLog::new()),
            Arc::new(FakeClock::new()),
        );
        let reply = call(&handler_for(Arc::new(service)), op, json!({}));
        assert_eq!(reply["status"], "failed", "{op}");
        assert_eq!(
            reply["error"],
            json!({"code": "internal", "message": "internal error", "retryable": true}),
            "{op}"
        );
        assert_eq!(reply["data"], json!({}), "{op}");
        assert!(!reply.to_string().contains("secret detail"), "{op}");
        assert_eq!(reply["meta"]["request_id"], "r", "{op}");
    }
    let _ = initialize;
}

// -- expiry counts ---------------------------------------------------------------------------------

use hecate_application::ports::Clock;
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::SecretsRig;
use hecate_domain::instant::isoformat;
use hecate_infrastructure::control::server::RequestHandler;

fn secrets_env() -> (SecretsRig, RequestHandler) {
    let rig = secrets_rig(true);
    let handler = secrets_handler(&rig);
    (rig, handler)
}

fn create_expiring(rig: &SecretsRig, name: &str, delta: Option<time::Duration>) {
    let expires_at = delta.map_or(serde_json::Value::Null, |delta| {
        json!(isoformat(rig.clock.now() + delta))
    });
    let options = CreateOptions {
        expires_at,
        ..CreateOptions::default()
    };
    rig.service
        .create(
            &format!("secret://hermes/{name}"),
            &format!("value-of-{name}"),
            "string",
            &options,
        )
        .unwrap();
}

fn counts(data: &serde_json::Value) -> (u64, u64) {
    (
        data["secrets_expired"].as_u64().unwrap(),
        data["secrets_expiring_soon"].as_u64().unwrap(),
    )
}

// py: contract/test_status_op.py::test_status_counts_expired_and_expiring_soon_secrets_without_paths
#[test]
fn status_counts_expired_and_expiring_soon_secrets_without_paths() {
    let (rig, handler) = secrets_env();
    create_expiring(&rig, "expired", Some(-time::Duration::days(1)));
    create_expiring(&rig, "soon", Some(time::Duration::days(3)));
    create_expiring(&rig, "later", Some(time::Duration::days(30)));
    create_expiring(&rig, "never", None);
    let reply = call(&handler, "status", json!({}));
    assert_eq!(reply["status"], "completed");
    let mut expected = status_fields();
    expected.extend([
        "secrets_expired".to_string(),
        "secrets_expiring_soon".to_string(),
        "secrets_due_rotation".to_string(),
    ]);
    expected.sort();
    assert_eq!(fields(&reply["data"]), expected);
    assert_eq!(counts(&reply["data"]), (1, 1));
    assert_eq!(reply["data"]["secrets_due_rotation"], 0);
    let shown = reply.to_string();
    assert!(
        !shown.contains("secret://")
            && !reply["data"]
                .to_string()
                .replace("secrets_expired", "")
                .contains("expired")
    );
}

// py: contract/test_status_op.py::test_the_limits_are_inclusive_on_the_side_of_the_nearest_date
#[test]
fn the_limits_are_inclusive_on_the_side_of_the_nearest_date() {
    let (rig, handler) = secrets_env();
    create_expiring(&rig, "now", Some(time::Duration::ZERO));
    create_expiring(&rig, "exactly-seven-days", Some(time::Duration::days(7)));
    create_expiring(
        &rig,
        "just-beyond",
        Some(time::Duration::days(7) + time::Duration::seconds(1)),
    );
    assert_eq!(counts(&call(&handler, "status", json!({}))["data"]), (1, 1));
}

// py: contract/test_status_op.py::test_the_counts_move_with_the_clock
#[test]
fn the_counts_move_with_the_clock() {
    let (rig, handler) = secrets_env();
    create_expiring(&rig, "soon", Some(time::Duration::days(3)));
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["secrets_expiring_soon"],
        1
    );
    rig.clock.advance(time::Duration::days(4));
    assert_eq!(counts(&call(&handler, "status", json!({}))["data"]), (1, 0));
}

// py: contract/test_status_op.py::test_the_counts_are_available_while_sealed
#[test]
fn the_counts_are_available_while_sealed() {
    let (rig, handler) = secrets_env();
    create_expiring(&rig, "expired", Some(-time::Duration::days(1)));
    create_expiring(&rig, "soon", Some(time::Duration::days(2)));
    rig.sealing.seal().unwrap();
    let reply = call(&handler, "status", json!({}));
    assert!(reply["status"] == "completed" && reply["data"]["state"] == "sealed");
    assert_eq!(counts(&reply["data"]), (1, 1));
}

// T035: the secrets due for rotation are counted too, also while sealed (FR-013).
#[test]
fn status_counts_the_secrets_due_for_rotation_also_while_sealed() {
    let (rig, handler) = secrets_env();
    let options = CreateOptions {
        rotate_every: json!("P1D"),
        ..CreateOptions::default()
    };
    rig.service
        .create("secret://hermes/daily", "daily-value", "string", &options)
        .unwrap();
    let due = |handler: &RequestHandler| {
        call(handler, "status", json!({}))["data"]["secrets_due_rotation"].clone()
    };
    assert_eq!(due(&handler), 0);
    rig.clock.advance(time::Duration::days(1));
    assert_eq!(due(&handler), 1);
    rig.sealing.seal().unwrap();
    let reply = call(&handler, "status", json!({}));
    assert_eq!(reply["data"]["state"], "sealed");
    assert_eq!(reply["data"]["secrets_due_rotation"], 1);
}

// py: contract/test_status_op.py::test_expiry_counts_can_be_asked_for_directly_and_leave_no_row_in_the_access_log
#[test]
fn expiry_counts_can_be_asked_for_directly_and_leave_no_row_in_the_access_log() {
    let (rig, _) = secrets_env();
    create_expiring(&rig, "expired", Some(-time::Duration::days(1)));
    let rows = rig.access_log.entries().len();
    let counts = rig.service.expiry_counts(Some(rig.clock.now())).unwrap();
    assert_eq!(
        (counts.secrets_expired, counts.secrets_expiring_soon),
        (1, 0)
    );
    assert_eq!(rig.access_log.entries().len(), rows);
}

// py: contract/test_status_op.py::test_a_secret_with_every_version_destroyed_is_no_longer_counted
#[test]
fn a_secret_with_every_version_destroyed_is_no_longer_counted() {
    let (rig, handler) = secrets_env();
    create_expiring(&rig, "expired", Some(-time::Duration::days(1)));
    let path = "secret://hermes/expired";
    rig.service.revoke(path, &json!(1), &json!(2)).unwrap();
    rig.service
        .destroy(
            path,
            &json!(1),
            &json!(3),
            &json!({"passphrase": GOOD_PASSPHRASE}),
        )
        .unwrap();
    assert_eq!(
        call(&handler, "status", json!({}))["data"]["secrets_expired"],
        0
    );
}

// py: contract/test_status_op.py::test_an_expired_secret_still_resolves
#[test]
fn an_expired_secret_still_resolves() {
    let (rig, _) = secrets_env();
    create_expiring(&rig, "expired", Some(-time::Duration::days(1)));
    let resolved = rig
        .service
        .resolve("hermes", &json!(["secret://hermes/expired"]))
        .unwrap();
    assert_eq!(resolved[0].value.as_str(), "value-of-expired");
}

// py: contract/test_status_op.py::test_a_handler_without_secrets_keeps_the_original_status_fields
#[test]
fn a_handler_without_secrets_keeps_the_original_status_fields() {
    let reply = call(&handler_for(rig().service), "status", json!({}));
    assert_eq!(fields(&reply["data"]), status_fields());
}

// --- The `ca` block (feature 003, T034) ---------------------------------------------------------

#[test]
fn status_with_the_ca_adds_its_block_also_while_sealed() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    let before = call(&handler, "status", json!({}))["data"].clone();
    let mut expected = status_fields();
    expected.push("ca".to_string());
    expected.sort();
    assert_eq!(fields(&before), expected);
    assert_eq!(before["ca"], json!({"exists": false}));

    let confirmed = create_ca_through(&handler, &rig);
    let unsealed = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(
        fields(&unsealed),
        [
            "enrollment_throttled",
            "exists",
            "hecate_certificate",
            "identities_expiring_24h",
            "intermediate_days_left",
            "root_fingerprint",
            "suspicious_renewals_24h",
            "trust_domain"
        ]
    );
    assert_eq!(unsealed["intermediate_days_left"], Value::Null);
    assert_eq!(unsealed["enrollment_throttled"], false);
    assert_eq!(unsealed["identities_expiring_24h"], 0);
    assert_eq!(unsealed["suspicious_renewals_24h"], 0);
    assert_eq!(unsealed["exists"], true);
    assert_eq!(unsealed["trust_domain"], TRUST_DOMAIN);
    assert_eq!(
        unsealed["root_fingerprint"],
        confirmed["data"]["root_fingerprint"]
    );
    let own = &unsealed["hecate_certificate"];
    assert_eq!(fields(own), ["not_after", "serial"]);
    let serial = own["serial"].as_str().unwrap();
    assert!(
        hecate_domain::identity::names::Serial::parse(serial).is_ok(),
        "{serial}"
    );
    assert!(
        own["not_after"]
            .as_str()
            .is_some_and(|text| text.ends_with("+00:00"))
    );

    call(&handler, "seal", json!({}));
    let sealed = call(&handler, "status", json!({}))["data"].clone();
    assert_eq!(sealed["state"], "sealed");
    assert_eq!(sealed["ca"], unsealed);
}

#[test]
fn status_shows_the_days_left_of_the_intermediate_under_30_also_while_sealed() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    let days_left =
        || call(&handler, "status", json!({}))["data"]["ca"]["intermediate_days_left"].clone();
    rig.clock.advance(time::Duration::days(336));
    assert_eq!(days_left(), json!(29));
    call(&handler, "seal", json!({}));
    assert_eq!(days_left(), json!(29));
    rig.clock.advance(time::Duration::days(30));
    assert_eq!(days_left(), json!(0));
}

#[test]
fn status_says_when_enrollments_are_throttled_also_while_sealed() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    for _ in 0..10 {
        let refused = rig.identities.enroll(
            Some("k1"),
            &json!({"token": "x", "csr": "x", "call_key": "x"}),
        );
        assert!(refused.is_err());
    }
    let throttled = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(throttled["enrollment_throttled"], true);
    call(&handler, "seal", json!({}));
    let sealed = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(sealed["enrollment_throttled"], true);
    rig.clock.advance(time::Duration::minutes(1));
    let over = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(over["enrollment_throttled"], false);
}

#[test]
fn status_counts_expiring_identities_and_suspicious_renewals_also_sealed() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    let token = rig
        .identities
        .tokens_create("hermes")
        .unwrap()
        .token
        .to_string();
    let dir = tempfile::tempdir().unwrap();
    let hermes =
        hecate_infrastructure::testing::TestComponent::open(dir.path(), rig.clock.clone()).unwrap();
    let (body, key) = hermes.enrollment(&token).unwrap();
    let enrolled = rig.identities.enroll(Some(&key), &body).unwrap();
    hermes
        .accept_enrollment(&enrolled.certificate, &enrolled.chain)
        .unwrap();
    rig.clock.advance(time::Duration::hours(49));
    let counted = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(counted["identities_expiring_24h"], 1);
    let first = hermes.serial().unwrap();
    let credentials = |from: Option<&hecate_domain::identity::names::Serial>| {
        hecate_application::identity::service::Credentials {
            certificates: hermes
                .certificate_chain_of(from)
                .unwrap()
                .into_iter()
                .take(2)
                .collect(),
            bearer: Some(hermes.call_token(from).unwrap()),
        }
    };
    let (renewal, renewal_key) = hermes.renewal().unwrap();
    let renewed = rig
        .identities
        .renew(&credentials(None), Some(&renewal_key), &renewal)
        .unwrap();
    hermes
        .accept_renewal(&renewed.certificate, &renewed.chain)
        .unwrap();
    let (other, other_key) = hermes.throwaway_renewal(None).unwrap();
    assert!(
        rig.identities
            .renew(&credentials(Some(&first)), Some(&other_key), &other)
            .is_err()
    );
    call(&handler, "seal", json!({}));
    let sealed = call(&handler, "status", json!({}))["data"]["ca"].clone();
    assert_eq!(sealed["suspicious_renewals_24h"], 1);
    assert_eq!(
        sealed["identities_expiring_24h"], 0,
        "its certificates were revoked"
    );
}
