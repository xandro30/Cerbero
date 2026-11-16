//! The lists of access of the apps (data-model.md): the list of an app as its stream gives it, and
//! the orders and queries of the admin (T010, US1).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use hecate_application::app_access::service::AppAccessService;
use hecate_application::errors::ServiceError;
use hecate_application::ports::{
    CaMaterial, Clock, EventRecord, EventStore, IdentityStore, IdentityWork, OwnTlsKey,
};
use hecate_application::testing::{CaRig, InMemoryIdentityStore};
use hecate_domain::app_access::access::{AppAccess, stream_name};
use hecate_domain::events::{AccessEvent, ChangedBy};
use hecate_domain::identity::names::IdentityName;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use time::{Duration, OffsetDateTime};

fn name(text: &str) -> IdentityName {
    IdentityName::parse(text).unwrap()
}

fn hermes() -> IdentityName {
    name("hermes")
}

/// A rig with an empty access log.
fn rig() -> CaRig {
    let rig = ca_rig();
    rig.access_log.clear();
    rig
}

type AccessRow = (Operation, AccessResult, String, Option<String>);

/// The rows of the lists of access: operation, result, detail and app, after checking that the
/// admin sent them from the local channel and that no row names a secret.
fn access_rows(rig: &CaRig) -> Vec<AccessRow> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| {
            matches!(
                entry.operation,
                Operation::AccessGrant
                    | Operation::AccessRevoke
                    | Operation::AccessList
                    | Operation::AccessGet
            )
        })
        .inspect(|entry| {
            assert_eq!(
                (
                    entry.subject.as_deref(),
                    entry.method,
                    entry.path.as_deref()
                ),
                (Some("admin"), Method::Local, None)
            );
        })
        .map(|entry| (entry.operation, entry.result, entry.detail, entry.identity))
        .collect()
}

fn row(operation: Operation, result: AccessResult, detail: &str, app: Option<&str>) -> AccessRow {
    (
        operation,
        result,
        detail.to_string(),
        app.map(str::to_string),
    )
}

#[test]
fn an_app_without_a_stream_has_the_empty_list() {
    let rig = rig();
    let access = rig.access.access_of(&hermes()).unwrap();
    assert_eq!(access, AppAccess::new(hermes()));
    assert_eq!(access.version, 0);
}

#[test]
fn a_new_hecate_has_no_list_and_grants_no_access() {
    let rig = rig();
    assert!(rig.access.list().unwrap().is_empty());
    let empty = rig.access.get("hermes").unwrap();
    assert_eq!((empty.version, empty.callers().count()), (0, 0));
    assert!(rig.store.streams("access/").unwrap().is_empty());
}

#[test]
fn the_admin_grants_and_revokes_an_access() {
    let rig = rig();
    let granted = rig.access.grant("hades", "hermes").unwrap();
    assert_eq!(
        (granted.caller.as_str(), granted.app.as_str()),
        ("hades", "hermes")
    );
    assert_eq!((granted.version, granted.changed), (1, true));
    assert!(
        rig.access
            .access_of(&hermes())
            .unwrap()
            .can_call(&name("hades"))
    );
    let revoked = rig.access.revoke("hades", "hermes").unwrap();
    assert_eq!((revoked.version, revoked.changed), (2, true));
    assert!(
        !rig.access
            .access_of(&hermes())
            .unwrap()
            .can_call(&name("hades"))
    );
}

#[test]
fn the_list_is_read_also_while_sealed() {
    let rig = rig();
    rig.access.grant("hades", "hermes").unwrap();
    rig.sealing.seal().unwrap();
    assert!(
        rig.access
            .access_of(&hermes())
            .unwrap()
            .can_call(&name("hades"))
    );
}

#[test]
fn malformed_or_refused_orders_change_nothing() {
    let rig = rig();
    for (identity, app) in [
        ("Hades", "hermes"),
        ("hades", "Hermes"),
        ("", "hermes"),
        ("hades", ""),
        ("admin", "hermes"),
        ("hecate", "hermes"),
        ("hades", "admin"),
        ("hades", "hecate"),
        ("hermes", "hermes"),
        ("hades/integration/x", "hermes"),
        ("hades", "hermes/x"),
    ] {
        for refused in [
            rig.access.grant(identity, app),
            rig.access.revoke(identity, app),
        ] {
            assert!(is_invalid(&refused.unwrap_err()), "{identity} {app}");
        }
    }
    for app in ["", "admin", "hecate", "Hermes", "hades/integration/x"] {
        assert!(is_invalid(&rig.access.get(app).unwrap_err()), "{app}");
    }
    assert!(rig.store.streams("access/").unwrap().is_empty());
    let refused = AccessResult::InvalidInput;
    let rows = access_rows(&rig);
    assert_eq!(
        rows[..4],
        [
            row(Operation::AccessGrant, refused, "", Some("hermes")),
            row(Operation::AccessRevoke, refused, "", Some("hermes")),
            row(Operation::AccessGrant, refused, "caller hades", None),
            row(Operation::AccessRevoke, refused, "caller hades", None),
        ]
    );
    assert!(rows.iter().all(|(_, result, ..)| *result == refused));
}

#[test]
fn an_identity_or_an_app_that_has_not_enrolled_takes_an_access() {
    let rig = rig();
    assert!(rig.access.grant("ares", "zeus").unwrap().changed);
    assert_eq!(rig.access.get("zeus").unwrap().version, 1);
}

#[test]
fn a_repeated_order_writes_no_event_and_still_leaves_its_row() {
    let rig = rig();
    rig.access.grant("hades", "hermes").unwrap();
    let again = rig.access.grant("hades", "hermes").unwrap();
    assert_eq!((again.version, again.changed), (1, false));
    let absent = rig.access.revoke("caller", "hermes").unwrap();
    assert_eq!((absent.version, absent.changed), (1, false));
    assert_eq!(rig.store.load("access/hermes").unwrap().len(), 1);
    let ok = AccessResult::Success;
    assert_eq!(
        access_rows(&rig),
        [
            row(Operation::AccessGrant, ok, "caller hades", Some("hermes")),
            row(Operation::AccessGrant, ok, "caller hades", Some("hermes")),
            row(Operation::AccessRevoke, ok, "caller caller", Some("hermes")),
        ]
    );
}

#[test]
fn the_list_holds_every_app_whose_list_changed_and_get_its_history() {
    let rig = rig();
    assert!(rig.access.list().unwrap().is_empty());
    rig.access.grant("hades", "hermes").unwrap();
    rig.access.grant("caller", "estigia").unwrap();
    rig.clock.advance(Duration::minutes(5));
    rig.access.revoke("hades", "hermes").unwrap();
    let listed: Vec<(String, u64, usize)> = rig
        .access
        .list()
        .unwrap()
        .iter()
        .map(|access| {
            (
                access.app.to_string(),
                access.version,
                access.callers().count(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [("estigia".to_string(), 1, 1), ("hermes".to_string(), 2, 0)]
    );
    assert_eq!(
        rig.access.get("hades").unwrap(),
        AppAccess::new(name("hades"))
    );
    let history = rig.access.get("hermes").unwrap().history;
    assert_eq!(
        history.iter().map(AccessEvent::kind).collect::<Vec<_>>(),
        ["CallerGranted", "CallerRevoked"]
    );
    assert_eq!(history[1].at() - history[0].at(), Duration::minutes(5));
    assert!(history.iter().all(|event| event.by() == ChangedBy::Admin));
    let ok = AccessResult::Success;
    let queries: Vec<AccessRow> = access_rows(&rig)
        .into_iter()
        .filter(|(operation, ..)| matches!(operation, Operation::AccessList | Operation::AccessGet))
        .collect();
    assert_eq!(
        queries,
        [
            row(Operation::AccessList, ok, "", None),
            row(Operation::AccessList, ok, "", None),
            row(Operation::AccessGet, ok, "", Some("hades")),
            row(Operation::AccessGet, ok, "", Some("hermes")),
        ]
    );
}

#[test]
fn sealed_the_orders_and_queries_are_conflicts_with_their_rows() {
    let rig = rig();
    rig.sealing.seal().unwrap();
    let refusals = [
        rig.access.grant("hades", "hermes").map(|_| ()),
        rig.access.revoke("hades", "hermes").map(|_| ()),
        rig.access.list().map(|_| ()),
        rig.access.get("hermes").map(|_| ()),
    ];
    for refused in refusals {
        assert_eq!(
            conflict_state(&refused.unwrap_err()),
            Some(SealState::Sealed)
        );
    }
    let sealed = AccessResult::RejectedConflict;
    assert_eq!(
        access_rows(&rig),
        [
            row(
                Operation::AccessGrant,
                sealed,
                "caller hades",
                Some("hermes")
            ),
            row(
                Operation::AccessRevoke,
                sealed,
                "caller hades",
                Some("hermes")
            ),
            row(Operation::AccessList, sealed, "", None),
            row(Operation::AccessGet, sealed, "", Some("hermes")),
        ]
    );
}

#[test]
fn a_power_cut_leaves_the_previous_list_or_the_new_one() {
    let rig = rig();
    rig.store.fail_commits(true);
    assert!(rig.access.grant("hades", "hermes").is_err());
    assert_eq!(rig.access.access_of(&hermes()).unwrap().version, 0);
    rig.store.fail_commits(false);
    assert_eq!(rig.access.grant("hades", "hermes").unwrap().version, 1);
}

/// The store of the identities of a test where another order grants `hades` the access to
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
            let other = AccessEvent::CallerGranted {
                at: self.at,
                caller: name("hades"),
                by: ChangedBy::Admin,
            }
            .encode();
            self.inner.transaction(&mut |writes| {
                writes
                    .append(&stream_name(&hermes()), std::slice::from_ref(&other), 0)
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
    let rig = rig();
    let racing = Arc::new(Racing {
        inner: rig.store.clone(),
        interfered: AtomicBool::new(false),
        at: rig.clock.now(),
    });
    let access = AppAccessService::new(
        rig.sealing.clone(),
        racing,
        rig.access_log.clone(),
        rig.clock.clone(),
    );
    let hades = access.grant("hades", "hermes").unwrap();
    assert_eq!((hades.version, hades.changed), (1, false));
    let caller = access.grant("caller", "hermes").unwrap();
    assert_eq!((caller.version, caller.changed), (2, true));
}
