//! The lists of access of the apps (data-model.md): the list of an app, and the orders and
//! queries of the admin (US1). Its streams, `access/<app>`, live in the store of the identities,
//! without tables of their own.

use std::sync::Arc;

use hecate_domain::app_access::access::{AppAccess, STREAM_PREFIX, stream_name};
use hecate_domain::events::{AccessEvent, ChangedBy};
use hecate_domain::identity::names::IdentityName;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use time::OffsetDateTime;

use crate::errors::ServiceError;
use crate::ports::{AccessLog, Clock, IdentityStore};
use crate::sealing::service::SealingService;
use crate::secrets::service::ADMIN;

/// How many times an order reads again when another one gets ahead of it.
const MAX_RACES: usize = 3;

/// What an order of the admin did: the version of the list after it, and whether it changed
/// something; a repeated order changes nothing and writes no event (FR-002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessChange {
    pub app: IdentityName,
    pub caller: IdentityName,
    pub version: u64,
    pub changed: bool,
}

/// What the row of an order or a query names: the app, in `identity`, and the caller, in
/// `detail`, each one once well formed, also when Hécate is sealed and the order is not read
/// further (data-model.md).
#[derive(Default)]
struct Trace {
    app: Option<String>,
    detail: String,
}

impl Trace {
    fn naming(caller: Option<&str>, app: &str) -> Self {
        Self {
            app: well_formed(app),
            detail: caller
                .and_then(well_formed)
                .map(|caller| format!("caller {caller}"))
                .unwrap_or_default(),
        }
    }
}

fn well_formed(text: &str) -> Option<String> {
    IdentityName::parse(text).ok().map(|name| name.to_string())
}

fn result_for(error: &ServiceError) -> AccessResult {
    match error {
        ServiceError::Hecate(HecateError::InvalidInput(_)) => AccessResult::InvalidInput,
        ServiceError::Hecate(HecateError::StateConflict { .. }) => AccessResult::RejectedConflict,
        ServiceError::Hecate(HecateError::NotFound(_)) => AccessResult::NotFound,
        _ => AccessResult::Failure,
    }
}

fn corrupt() -> ServiceError {
    ServiceError::internal("InvalidEvents")
}

pub struct AppAccessService {
    sealing: Arc<SealingService>,
    store: Arc<dyn IdentityStore>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
}

impl AppAccessService {
    pub fn new(
        sealing: Arc<SealingService>,
        store: Arc<dyn IdentityStore>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            sealing,
            store,
            access_log,
            clock,
        }
    }

    /// The list of an app as its stream gives it, or the empty one at version 0 without a stream:
    /// no one may call the app (FR-010). It is read also while Hécate is sealed: a list is
    /// metadata, like the Access Policy.
    pub fn access_of(&self, app: &IdentityName) -> Result<AppAccess, ServiceError> {
        let events = self
            .store
            .load(&stream_name(app))?
            .iter()
            .map(AccessEvent::decode)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| corrupt())?;
        AppAccess::from_events(app.clone(), &events).map_err(|_| corrupt())
    }

    fn log(
        &self,
        operation: Operation,
        result: AccessResult,
        trace: Trace,
    ) -> Result<(), ServiceError> {
        self.access_log.append(&AccessLogEntry {
            at: self.clock.now(),
            operation,
            method: Method::Local,
            result,
            detail: trace.detail,
            subject: Some(ADMIN.to_string()),
            path: None,
            version: None,
            identity: trace.app,
            certificate: None,
        })
    }

    /// Leaves the row of an order or a query of the admin, served or refused, with what `trace`
    /// names; Hécate must be unsealed (FR-002).
    fn audited<T>(
        &self,
        operation: Operation,
        trace: Trace,
        work: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let outcome = self.sealing.unsealed_epoch().and_then(|_| work());
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(operation, result, trace)?;
        outcome
    }

    /// Leaves a row for a recognized operation whose arguments were malformed.
    pub fn record_invalid_request(&self, operation: Operation) -> Result<(), ServiceError> {
        self.log(operation, AccessResult::InvalidInput, Trace::default())
    }

    /// Applies an order without an expected version: it reads the list, decides, and writes the
    /// event at the version it read; when another order got ahead, it reads again and decides
    /// again. Returns the version after the order and whether it changed something.
    fn change(
        &self,
        app: &IdentityName,
        decide: impl Fn(&AppAccess, OffsetDateTime) -> Result<Option<AccessEvent>, HecateError>,
    ) -> Result<(u64, bool), ServiceError> {
        let stream = stream_name(app);
        for _ in 0..MAX_RACES {
            let access = self.access_of(app)?;
            let Some(event) = decide(&access, self.clock.now())? else {
                return Ok((access.version, false));
            };
            let records = [event.encode()];
            match self.store.transaction(&mut |writes| {
                writes.append(&stream, &records, access.version)?;
                Ok(())
            }) {
                Ok(()) => return Ok((access.version + 1, true)),
                Err(ServiceError::Concurrency(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ServiceError::internal("TooManyRaces"))
    }

    /// One order on the list of `app`: both names well formed, with a single segment (FR-011),
    /// neither of them reserved, and not the app itself as the caller (FR-004). Neither of them
    /// needs to have enrolled (FR-005).
    fn order(
        &self,
        operation: Operation,
        caller: &str,
        app: &str,
        decide: impl Fn(
            &AppAccess,
            &IdentityName,
            OffsetDateTime,
        ) -> Result<Option<AccessEvent>, HecateError>,
    ) -> Result<AccessChange, ServiceError> {
        self.audited(operation, Trace::naming(Some(caller), app), || {
            let caller = IdentityName::parse(caller)?;
            let app = IdentityName::parse(app)?;
            let (version, changed) = self.change(&app, |access, at| decide(access, &caller, at))?;
            Ok(AccessChange {
                app,
                caller,
                version,
                changed,
            })
        })
    }

    /// Lets `caller` call `app`; granting what is granted changes nothing (FR-002).
    pub fn grant(&self, caller: &str, app: &str) -> Result<AccessChange, ServiceError> {
        self.order(Operation::AccessGrant, caller, app, |access, caller, at| {
            access.grant(caller, ChangedBy::Admin, at)
        })
    }

    /// Stops `caller` from calling `app`; revoking what is not granted changes nothing (FR-002).
    pub fn revoke(&self, caller: &str, app: &str) -> Result<AccessChange, ServiceError> {
        self.order(
            Operation::AccessRevoke,
            caller,
            app,
            |access, caller, at| access.revoke(caller, ChangedBy::Admin, at),
        )
    }

    /// The apps whose list has ever changed, also the ones that no one calls now, by name
    /// (FR-003).
    pub fn list(&self) -> Result<Vec<AppAccess>, ServiceError> {
        self.audited(Operation::AccessList, Trace::default(), || {
            let mut lists = Vec::new();
            for stream in self.store.streams(STREAM_PREFIX)? {
                let name = stream.strip_prefix(STREAM_PREFIX).unwrap_or_default();
                let app = IdentityName::parse(name).map_err(|_| corrupt())?;
                lists.push(self.access_of(&app)?);
            }
            lists.sort_by(|a, b| a.app.as_str().cmp(b.app.as_str()));
            Ok(lists)
        })
    }

    /// The list of an app with its history; without changes, the empty one at version 0
    /// (FR-003). The reserved identities have none.
    pub fn get(&self, app: &str) -> Result<AppAccess, ServiceError> {
        self.audited(Operation::AccessGet, Trace::naming(None, app), || {
            let app = IdentityName::parse(app)?;
            if app.is_reserved() {
                return Err(
                    HecateError::invalid(format!("the identity '{app}' is reserved")).into(),
                );
            }
            self.access_of(&app)
        })
    }
}
