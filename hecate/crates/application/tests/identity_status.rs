//! T033: the aggregate status as a use case (US3): a component with the status permission reads
//! the counts of Hécate, also sealed; without it, or with credentials that do not work, it is
//! refused with a row (FR-010 to FR-012, FR-015).

mod common;

use common::*;
use hecate_application::identity::service::AggregateStatus;
use hecate_application::ports::Clock;
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::CaRig;
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use serde_json::json;
use time::Duration;

const DAILY: &str = "secret://hades/backups/key";
const SOON: &str = "secret://hades/smtp/password";

/// A rig with a CA, two secrets of `hades` (one rotated every day and one that expires in three
/// days), `hermes` enrolled with the status permission, and an empty access log.
fn with_status() -> (CaRig, Component) {
    let (rig, _) = created_ca();
    let daily = CreateOptions {
        rotate_every: json!("P1D"),
        ..CreateOptions::default()
    };
    rig.secrets
        .create(DAILY, "daily-value", "string", &daily)
        .unwrap();
    let soon = CreateOptions {
        expires_at: json!(isoformat(rig.clock.now() + Duration::days(3))),
        ..CreateOptions::default()
    };
    rig.secrets
        .create(SOON, "smtp-value", "string", &soon)
        .unwrap();
    let hermes = Component::enrolled(&rig, "hermes");
    rig.policies.grant_status("hermes").unwrap();
    rig.access_log.clear();
    (rig, hermes)
}

fn status_rows(rig: &CaRig) -> Vec<AccessLogEntry> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::StatusGet)
        .collect()
}

#[test]
fn a_component_with_the_permission_reads_the_counts_and_leaves_no_row() {
    let (rig, hermes) = with_status();
    rig.clock.advance(Duration::days(1));
    let status = rig.identities.status(&hermes.credentials(None)).unwrap();
    assert_eq!(
        status,
        AggregateStatus {
            sealed: false,
            secrets_expired: 0,
            secrets_expiring_soon: 1,
            secrets_due_rotation: 1,
            identities_expiring_24h: 0,
            suspicious_renewals_24h: 0,
            intermediate_days_left: None,
            enrollment_throttled: false,
        }
    );
    let shown = format!("{status:?}");
    assert!(!shown.contains("secret://") && !shown.contains("hades") && !shown.contains("hermes"));
    // The certificates of the components last 72 hours: two days later, hermes's expires within
    // 24 hours.
    rig.clock.advance(Duration::days(1));
    let later = rig.identities.status(&hermes.credentials(None)).unwrap();
    assert_eq!(later.identities_expiring_24h, 1);
    assert!(status_rows(&rig).is_empty());
}

#[test]
fn without_the_permission_the_query_is_refused_with_a_row() {
    let (rig, _hermes) = with_status();
    let hades = Component::enrolled(&rig, "hades");
    rig.access_log.clear();
    let error = rig.identities.status(&hades.credentials(None)).unwrap_err();
    assert!(is_auth(&error), "{error:?}");
    let rows = status_rows(&rig);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(
        (
            row.method,
            row.result,
            row.detail.as_str(),
            row.subject.clone(),
            row.identity.as_deref(),
            row.certificate.clone(),
        ),
        (
            Method::Identity,
            AccessResult::Failure,
            "get",
            Some(spiffe("hades").to_string()),
            Some("hades"),
            Some(hades.serial().to_string()),
        )
    );
    assert!(row.path.is_none() && row.version.is_none());
}

#[test]
fn revoking_the_permission_refuses_the_next_query() {
    let (rig, hermes) = with_status();
    assert!(rig.identities.status(&hermes.credentials(None)).is_ok());
    rig.policies.revoke_status("hermes").unwrap();
    rig.access_log.clear();
    let error = rig
        .identities
        .status(&hermes.credentials(None))
        .unwrap_err();
    assert!(is_auth(&error), "{error:?}");
    let rows = status_rows(&rig);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].identity.as_deref(), Some("hermes"));
}

#[test]
fn sealed_it_answers_with_the_same_counts_and_refuses_the_same_way() {
    let (rig, hermes) = with_status();
    let hades = Component::enrolled(&rig, "hades");
    rig.clock.advance(Duration::days(1));
    let unsealed = rig.identities.status(&hermes.credentials(None)).unwrap();
    rig.sealing.seal().unwrap();
    rig.access_log.clear();
    let sealed = rig.identities.status(&hermes.credentials(None)).unwrap();
    assert_eq!(
        sealed,
        AggregateStatus {
            sealed: true,
            ..unsealed
        }
    );
    assert!(status_rows(&rig).is_empty());
    let mut forged = hermes.credentials(None);
    forged.bearer = Some("not.a.token".into());
    assert!(is_auth(&rig.identities.status(&forged).unwrap_err()));
    assert!(is_auth(
        &rig.identities.status(&hades.credentials(None)).unwrap_err()
    ));
    let rows: Vec<(AccessResult, Option<String>)> = status_rows(&rig)
        .into_iter()
        .map(|row| (row.result, row.identity))
        .collect();
    assert_eq!(
        rows,
        [
            (AccessResult::Failure, Some("hermes".to_string())),
            (AccessResult::Failure, Some("hades".to_string())),
        ]
    );
}
