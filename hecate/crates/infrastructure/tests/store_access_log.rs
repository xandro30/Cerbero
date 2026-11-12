//! The access log of the identities (FR-024): the columns `identity` and `certificate`, the new
//! operations and methods, and the same append-only triggers.

use hecate_application::ports::AccessLog;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_infrastructure::store::sealing::SqliteAccessLog;
use time::macros::datetime;

fn entry(
    operation: Operation,
    method: Method,
    result: AccessResult,
    subject: Option<&str>,
    identity: Option<&str>,
    certificate: Option<&str>,
) -> AccessLogEntry {
    AccessLogEntry {
        at: datetime!(2026-11-09 12:00:00.123456 UTC),
        operation,
        method,
        result,
        detail: operation.as_str().to_string(),
        subject: subject.map(str::to_string),
        path: None,
        version: None,
        identity: identity.map(str::to_string),
        certificate: certificate.map(str::to_string),
    }
}

#[test]
fn rows_with_identity_and_certificate_are_written_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let log = SqliteAccessLog::open(&dir.path().join("hecate.db")).unwrap();
    let entries = [
        entry(
            Operation::CertificateRevoke,
            Method::Local,
            AccessResult::Success,
            Some("admin"),
            Some("hermes"),
            Some("5f1c"),
        ),
        entry(
            Operation::IdentityEnroll,
            Method::EnrollmentToken,
            AccessResult::Failure,
            None,
            None,
            None,
        ),
        entry(
            Operation::IdentityEnrollThrottled,
            Method::Internal,
            AccessResult::RejectedRateLimited,
            None,
            None,
            None,
        ),
        entry(
            Operation::CertificateRenew,
            Method::Identity,
            AccessResult::Success,
            Some("spiffe://cerbero.test/hermes"),
            Some("hermes"),
            Some("6a2d"),
        ),
    ];
    for entry in &entries {
        log.append(entry).unwrap();
    }
    assert_eq!(log.entries().unwrap(), entries.to_vec());
}

#[test]
fn every_new_operation_and_method_has_its_text() {
    let operations = [
        (Operation::CaCreate, "ca_create"),
        (Operation::CaIntermediateRenew, "ca_intermediate_renew"),
        (Operation::CaReplace, "ca_replace"),
        (Operation::TokenCreate, "token_create"),
        (Operation::TokenList, "token_list"),
        (Operation::TokenAnnul, "token_annul"),
        (Operation::IdentityList, "identity_list"),
        (Operation::IdentityGet, "identity_get"),
        (Operation::IdentityDisable, "identity_disable"),
        (Operation::IdentityEnable, "identity_enable"),
        (Operation::IdentityEnroll, "identity_enroll"),
        (
            Operation::IdentityEnrollThrottled,
            "identity_enroll_throttled",
        ),
        (Operation::CertificateRenew, "certificate_renew"),
        (Operation::CertificateRevoke, "certificate_revoke"),
        (Operation::KeyList, "key_list"),
        (Operation::KeyGet, "key_get"),
        (Operation::RevocationList, "revocation_list"),
        (Operation::CapabilitiesGet, "capabilities_get"),
    ];
    for (operation, text) in operations {
        assert_eq!(operation.as_str(), text);
        assert_eq!(Operation::parse(text), Some(operation));
    }
    for (method, text) in [
        (Method::EnrollmentToken, "enrollment_token"),
        (Method::Internal, "internal"),
    ] {
        assert_eq!(method.as_str(), text);
        assert_eq!(Method::parse(text), Some(method));
    }
}

#[test]
fn rows_of_the_access_policy_and_the_status_are_written_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let log = SqliteAccessLog::open(&dir.path().join("hecate.db")).unwrap();
    let admin = |operation, identity| {
        entry(
            operation,
            Method::Local,
            AccessResult::Success,
            Some("admin"),
            identity,
            None,
        )
    };
    let mut grant = admin(Operation::PolicyGrant, Some("hermes"));
    grant.detail = "read".to_string();
    grant.path = Some("secret://hades/smtp/password".to_string());
    let mut revoke = admin(Operation::PolicyRevoke, Some("hermes"));
    revoke.detail = "status".to_string();
    let entries = [
        grant,
        revoke,
        admin(Operation::PolicyList, None),
        admin(Operation::PolicyGet, Some("hermes")),
        entry(
            Operation::StatusGet,
            Method::Identity,
            AccessResult::Failure,
            Some("spiffe://cerbero.test/hermes"),
            Some("hermes"),
            Some("6a2d"),
        ),
        entry(
            Operation::StatusGet,
            Method::Identity,
            AccessResult::Failure,
            None,
            None,
            None,
        ),
    ];
    for entry in &entries {
        log.append(entry).unwrap();
    }
    assert_eq!(log.entries().unwrap(), entries.to_vec());
}

#[test]
fn the_log_still_rejects_update_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let log = SqliteAccessLog::open(&path).unwrap();
    log.append(&entry(
        Operation::TokenCreate,
        Method::Local,
        AccessResult::Success,
        Some("admin"),
        Some("hermes"),
        None,
    ))
    .unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    for statement in [
        "UPDATE access_log SET identity = 'hades'",
        "DELETE FROM access_log",
    ] {
        let error = raw.execute(statement, []).unwrap_err();
        assert!(
            error.to_string().contains("append-only"),
            "{statement}: {error}"
        );
    }
}
