//! The Access Policy of the identities (data-model.md): the policy of an identity as its stream
//! gives it (T007), which the resolution and the status read, and the orders and queries of the
//! admin (T023, US2).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use hecate_application::access_policy::service::AccessPolicyService;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    CaMaterial, Clock, EventRecord, EventStore, IdentityStore, IdentityWork, OwnTlsKey,
};
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::{CaRig, InMemoryIdentityStore};
use hecate_domain::access_policy::policy::{AccessPolicy, stream_name};
use hecate_domain::events::{ChangedBy, PolicyEvent};
use hecate_domain::identity::names::IdentityName;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use hecate_domain::secrets::path::SecretPath;
use time::{Duration, OffsetDateTime};

const SMTP: &str = "secret://hades/smtp/password";
const OWN: &str = "secret://hermes/telegram/bot_token";

fn hermes() -> IdentityName {
    IdentityName::parse("hermes").unwrap()
}

/// Writes events in the stream of a policy, as the orders of the admin will.
fn write(rig: &CaRig, identity: &IdentityName, events: &[PolicyEvent]) {
    let records: Vec<_> = events.iter().map(PolicyEvent::encode).collect();
    rig.store
        .transaction(&mut |writes| {
            writes
                .append(&stream_name(identity), &records, 0)
                .map(|_| ())
        })
        .unwrap();
}

#[test]
fn an_identity_without_a_stream_has_the_empty_policy() {
    let rig = ca_rig();
    let policy = rig.policies.policy_of(&hermes()).unwrap();
    assert_eq!(policy, AccessPolicy::new(hermes()));
    assert_eq!(policy.version, 0);
}

#[test]
fn an_identity_with_events_has_the_policy_they_give() {
    let rig = ca_rig();
    let at = rig.clock.now();
    let smtp = SecretPath::parse("secret://hades/smtp/password").unwrap();
    let events = [
        PolicyEvent::ReadGranted {
            at,
            secret: smtp.clone(),
            by: ChangedBy::Admin,
        },
        PolicyEvent::StatusGranted {
            at,
            by: ChangedBy::Admin,
        },
    ];
    write(&rig, &hermes(), &events);
    let policy = rig.policies.policy_of(&hermes()).unwrap();
    assert_eq!(
        policy,
        AccessPolicy::from_events(hermes(), &events).unwrap()
    );
    assert!(policy.can_read(&smtp));
    assert!(policy.has_status());
    assert_eq!(policy.version, 2);
    let hades = IdentityName::parse("hades").unwrap();
    assert_eq!(rig.policies.policy_of(&hades).unwrap().version, 0);
}

#[test]
fn the_policy_is_read_also_while_sealed() {
    let rig = ca_rig();
    let granted = PolicyEvent::StatusGranted {
        at: rig.clock.now(),
        by: ChangedBy::Admin,
    };
    write(&rig, &hermes(), &[granted]);
    rig.sealing.seal().unwrap();
    assert!(rig.policies.policy_of(&hermes()).unwrap().has_status());
}

#[test]
fn a_stream_that_breaks_the_rules_is_an_internal_error() {
    let rig = ca_rig();
    let revoked = PolicyEvent::StatusRevoked {
        at: rig.clock.now(),
        by: ChangedBy::Admin,
    };
    write(&rig, &hermes(), &[revoked]);
    let error = rig.policies.policy_of(&hermes()).unwrap_err();
    assert!(
        matches!(
            error,
            ServiceError::Internal {
                kind: "InvalidEvents"
            }
        ),
        "{error:?}"
    );
}

// --- The orders and queries of the admin (T023) --------------------------------------------------

/// A rig with a secret of `hades` and one of `hermes`, and an empty access log.
fn with_secrets() -> CaRig {
    let rig = ca_rig();
    for (path, value) in [(SMTP, "smtp-value"), (OWN, "token-value")] {
        rig.secrets
            .create(path, value, "string", &CreateOptions::default())
            .unwrap();
    }
    rig.access_log.clear();
    rig
}

type PolicyRow = (
    Operation,
    AccessResult,
    String,
    Option<String>,
    Option<String>,
);

/// The rows of the Access Policy: operation, result, detail, identity and secret, after checking
/// that the admin sent them from the local channel.
fn policy_rows(rig: &CaRig) -> Vec<PolicyRow> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.operation,
                Operation::PolicyGrant
                    | Operation::PolicyRevoke
                    | Operation::PolicyList
                    | Operation::PolicyGet
            )
        })
        .inspect(|entry| {
            assert_eq!(
                (entry.subject.as_deref(), entry.method),
                (Some("admin"), Method::Local)
            );
        })
        .map(|entry| {
            (
                entry.operation,
                entry.result,
                entry.detail,
                entry.identity,
                entry.path,
            )
        })
        .collect()
}

fn row(
    operation: Operation,
    result: AccessResult,
    detail: &str,
    identity: Option<&str>,
    path: Option<&str>,
) -> PolicyRow {
    (
        operation,
        result,
        detail.to_string(),
        identity.map(str::to_string),
        path.map(str::to_string),
    )
}

#[test]
fn the_admin_grants_and_revokes_a_read_and_the_status_permission() {
    let rig = with_secrets();
    let read = rig.policies.grant_read("hermes", SMTP).unwrap();
    assert_eq!(
        (read.identity.as_str(), read.secret.unwrap().to_string()),
        ("hermes", SMTP.to_string())
    );
    assert_eq!((read.version, read.changed), (1, true));
    let status = rig.policies.grant_status("hermes").unwrap();
    assert_eq!(
        (status.secret, status.version, status.changed),
        (None, 2, true)
    );
    let smtp = SecretPath::parse(SMTP).unwrap();
    let policy = rig.policies.policy_of(&hermes()).unwrap();
    assert!(policy.can_read(&smtp) && policy.has_status());
    let revoked = rig.policies.revoke_read("hermes", SMTP).unwrap();
    assert_eq!((revoked.version, revoked.changed), (3, true));
    let revoked = rig.policies.revoke_status("hermes").unwrap();
    assert_eq!((revoked.version, revoked.changed), (4, true));
    let policy = rig.policies.policy_of(&hermes()).unwrap();
    assert!(!policy.can_read(&smtp) && !policy.has_status());
}

#[test]
fn malformed_or_refused_orders_change_nothing() {
    let rig = with_secrets();
    for (identity, secret) in [
        ("Hermes", SMTP),
        ("admin", SMTP),
        ("hecate", SMTP),
        ("hermes", "secret://hades"),
        ("hermes", "secret://hades/smtp/password@1"),
        ("hermes", OWN),
    ] {
        for refused in [
            rig.policies.grant_read(identity, secret),
            rig.policies.revoke_read(identity, secret),
        ] {
            assert!(is_invalid(&refused.unwrap_err()), "{identity} {secret}");
        }
    }
    for identity in ["", "admin", "hecate"] {
        assert!(is_invalid(
            &rig.policies.grant_status(identity).unwrap_err()
        ));
        assert!(is_invalid(
            &rig.policies.revoke_status(identity).unwrap_err()
        ));
        assert!(is_invalid(&rig.policies.get(identity).unwrap_err()));
    }
    let missing = rig
        .policies
        .grant_read("hermes", "secret://hades/smtp/nothing")
        .unwrap_err();
    assert!(is_not_found(&missing));
    assert!(rig.store.streams("policy/").unwrap().is_empty());
}

#[test]
fn an_identity_that_has_not_enrolled_takes_a_policy() {
    let rig = with_secrets();
    assert!(rig.policies.grant_read("ares", SMTP).unwrap().changed);
    assert!(rig.policies.grant_status("ares").unwrap().changed);
    assert_eq!(rig.policies.get("ares").unwrap().version, 2);
}

#[test]
fn a_repeated_order_writes_no_event_and_still_leaves_its_row() {
    let rig = with_secrets();
    rig.policies.grant_read("hermes", SMTP).unwrap();
    let again = rig.policies.grant_read("hermes", SMTP).unwrap();
    assert_eq!((again.version, again.changed), (1, false));
    let absent = rig.policies.revoke_status("hermes").unwrap();
    assert_eq!((absent.version, absent.changed), (1, false));
    assert_eq!(rig.store.load("policy/hermes").unwrap().len(), 1);
    assert_eq!(
        policy_rows(&rig),
        [
            row(
                Operation::PolicyGrant,
                AccessResult::Success,
                "read",
                Some("hermes"),
                Some(SMTP)
            ),
            row(
                Operation::PolicyGrant,
                AccessResult::Success,
                "read",
                Some("hermes"),
                Some(SMTP)
            ),
            row(
                Operation::PolicyRevoke,
                AccessResult::Success,
                "status",
                Some("hermes"),
                None
            ),
        ]
    );
}

#[test]
fn the_list_holds_every_identity_whose_policy_changed_and_get_its_history() {
    let rig = with_secrets();
    assert!(rig.policies.list().unwrap().is_empty());
    rig.policies.grant_status("zeus").unwrap();
    rig.policies.grant_read("hermes", SMTP).unwrap();
    rig.clock.advance(Duration::minutes(5));
    rig.policies.revoke_read("hermes", SMTP).unwrap();
    let listed: Vec<(String, u64)> = rig
        .policies
        .list()
        .unwrap()
        .iter()
        .map(|policy| (policy.identity.to_string(), policy.version))
        .collect();
    assert_eq!(listed, [("hermes".to_string(), 2), ("zeus".to_string(), 1)]);
    let empty = rig.policies.get("hades").unwrap();
    assert_eq!(
        empty,
        AccessPolicy::new(IdentityName::parse("hades").unwrap())
    );
    let history = rig.policies.get("hermes").unwrap().history;
    assert_eq!(
        history.iter().map(PolicyEvent::kind).collect::<Vec<_>>(),
        ["ReadGranted", "ReadRevoked"]
    );
    assert_eq!(history[1].at() - history[0].at(), Duration::minutes(5));
    assert!(history.iter().all(|event| event.by() == ChangedBy::Admin));
    let queries: Vec<PolicyRow> = policy_rows(&rig)
        .into_iter()
        .filter(|(operation, ..)| matches!(operation, Operation::PolicyList | Operation::PolicyGet))
        .collect();
    assert_eq!(
        queries,
        [
            row(
                Operation::PolicyList,
                AccessResult::Success,
                "list",
                None,
                None
            ),
            row(
                Operation::PolicyList,
                AccessResult::Success,
                "list",
                None,
                None
            ),
            row(
                Operation::PolicyGet,
                AccessResult::Success,
                "get",
                Some("hades"),
                None
            ),
            row(
                Operation::PolicyGet,
                AccessResult::Success,
                "get",
                Some("hermes"),
                None
            ),
        ]
    );
}

#[test]
fn sealed_the_orders_and_queries_are_conflicts_with_their_rows() {
    let rig = with_secrets();
    rig.sealing.seal().unwrap();
    let refusals = [
        rig.policies.grant_read("hermes", SMTP).map(|_| ()),
        rig.policies.revoke_read("hermes", SMTP).map(|_| ()),
        rig.policies.grant_status("hermes").map(|_| ()),
        rig.policies.revoke_status("hermes").map(|_| ()),
        rig.policies.list().map(|_| ()),
        rig.policies.get("hermes").map(|_| ()),
    ];
    for refused in refusals {
        assert_eq!(
            conflict_state(&refused.unwrap_err()),
            Some(SealState::Sealed)
        );
    }
    // Each row names the identity of the policy and the secret, as the contract asks, also when
    // Hécate is sealed.
    let sealed = AccessResult::RejectedConflict;
    assert_eq!(
        policy_rows(&rig),
        [
            row(
                Operation::PolicyGrant,
                sealed,
                "read",
                Some("hermes"),
                Some(SMTP)
            ),
            row(
                Operation::PolicyRevoke,
                sealed,
                "read",
                Some("hermes"),
                Some(SMTP)
            ),
            row(
                Operation::PolicyGrant,
                sealed,
                "status",
                Some("hermes"),
                None
            ),
            row(
                Operation::PolicyRevoke,
                sealed,
                "status",
                Some("hermes"),
                None
            ),
            row(Operation::PolicyList, sealed, "list", None, None),
            row(Operation::PolicyGet, sealed, "get", Some("hermes"), None),
        ]
    );
}

#[test]
fn a_power_cut_leaves_the_previous_policy_or_the_new_one() {
    let rig = with_secrets();
    rig.store.fail_commits(true);
    assert!(rig.policies.grant_read("hermes", SMTP).is_err());
    assert_eq!(rig.policies.policy_of(&hermes()).unwrap().version, 0);
    rig.store.fail_commits(false);
    assert_eq!(rig.policies.grant_read("hermes", SMTP).unwrap().version, 1);
}

/// The store of the identities of a test where another order grants the status permission to
/// `hermes` just before the first transaction writes.
struct Racing {
    inner: Arc<InMemoryIdentityStore>,
    interfered: AtomicBool,
    at: OffsetDateTime,
}

impl EventStore for Racing {
    fn load(&self, stream: &str) -> Result<Vec<EventRecord>, ServiceError> {
        self.inner.load(stream)
    }

    fn streams(&self, prefix: &str) -> Result<Vec<String>, ServiceError> {
        self.inner.streams(prefix)
    }
}

impl IdentityStore for Racing {
    fn transaction(&self, work: &mut IdentityWork<'_>) -> Result<(), ServiceError> {
        if !self.interfered.swap(true, Ordering::SeqCst) {
            let other = PolicyEvent::StatusGranted {
                at: self.at,
                by: ChangedBy::Admin,
            }
            .encode();
            self.inner.transaction(&mut |writes| {
                writes
                    .append("policy/hermes", std::slice::from_ref(&other), 0)
                    .map(|_| ())
            })?;
        }
        self.inner.transaction(work)
    }

    fn load_ca_material(&self) -> Result<Vec<CaMaterial>, ServiceError> {
        self.inner.load_ca_material()
    }

    fn load_own_tls_key(&self) -> Result<Option<OwnTlsKey>, ServiceError> {
        self.inner.load_own_tls_key()
    }
}

#[test]
fn the_order_that_loses_a_race_reads_again_and_decides_again() {
    let rig = with_secrets();
    let racing = Arc::new(Racing {
        inner: rig.store.clone(),
        interfered: AtomicBool::new(false),
        at: rig.clock.now(),
    });
    let policies = AccessPolicyService::new(
        rig.sealing.clone(),
        racing,
        rig.secrets_store.clone(),
        rig.access_log.clone(),
        rig.clock.clone(),
    );
    let status = policies.grant_status("hermes").unwrap();
    assert_eq!((status.version, status.changed), (1, false));
    let read = policies.grant_read("hermes", SMTP).unwrap();
    assert_eq!((read.version, read.changed), (2, true));
}
