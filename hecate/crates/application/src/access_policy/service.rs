//! The Access Policy of the identities (data-model.md): the policy of an identity, which the
//! resolution and the status read, and the orders and queries of the admin (US2). Its streams,
//! `policy/<identity>`, live in the store of the identities, without tables of their own.

use std::sync::Arc;

use hecate_domain::access_policy::policy::{AccessPolicy, STREAM_PREFIX, stream_name};
use hecate_domain::events::{ChangedBy, PolicyEvent};
use hecate_domain::identity::names::IdentityName;
use hecate_domain::sealing::access_log::{AccessLogEntry, AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::secrets::path::SecretPath;
use time::OffsetDateTime;

use crate::errors::ServiceError;
use crate::ports::{AccessLog, Clock, IdentityStore, SecretStore};
use crate::sealing::service::SealingService;
use crate::secrets::service::ADMIN;

/// How many times an order reads again when another one gets ahead of it.
const MAX_RACES: usize = 3;

/// What an order of the admin did: the version of the policy after it, and whether it changed
/// something; a repeated order changes nothing and writes no event (FR-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyChange {
    pub identity: IdentityName,
    pub secret: Option<SecretPath>,
    pub version: u64,
    pub changed: bool,
}

/// What the row of an order or a query names: the identity and the secret of its arguments,
/// each one once well formed, also when Hécate is sealed and the order is not read further.
#[derive(Default)]
struct Trace {
    identity: Option<String>,
    path: Option<String>,
}

impl Trace {
    fn naming(identity: &str, secret: Option<&str>) -> Self {
        Self {
            identity: IdentityName::parse(identity)
                .ok()
                .map(|name| name.to_string()),
            path: secret
                .and_then(|text| SecretPath::parse(text).ok())
                .map(|path| path.to_string()),
        }
    }
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

pub struct AccessPolicyService {
    sealing: Arc<SealingService>,
    store: Arc<dyn IdentityStore>,
    secrets: Arc<dyn SecretStore>,
    access_log: Arc<dyn AccessLog>,
    clock: Arc<dyn Clock>,
}

impl AccessPolicyService {
    pub fn new(
        sealing: Arc<SealingService>,
        store: Arc<dyn IdentityStore>,
        secrets: Arc<dyn SecretStore>,
        access_log: Arc<dyn AccessLog>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            sealing,
            store,
            secrets,
            access_log,
            clock,
        }
    }

    /// The policy of an identity as its stream gives it, or the empty one at version 0 without a
    /// stream. It is read also while Hécate is sealed: a policy is metadata (FR-012).
    pub fn policy_of(&self, identity: &IdentityName) -> Result<AccessPolicy, ServiceError> {
        let events = self
            .store
            .load(&stream_name(identity))?
            .iter()
            .map(PolicyEvent::decode)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| corrupt())?;
        AccessPolicy::from_events(identity.clone(), &events).map_err(|_| corrupt())
    }

    fn log(
        &self,
        operation: Operation,
        result: AccessResult,
        detail: &str,
        trace: Trace,
    ) -> Result<(), ServiceError> {
        self.access_log.append(&AccessLogEntry {
            at: self.clock.now(),
            operation,
            method: Method::Local,
            result,
            detail: detail.to_string(),
            subject: Some(ADMIN.to_string()),
            path: trace.path,
            version: None,
            identity: trace.identity,
            certificate: None,
        })
    }

    /// Leaves the row of an order or a query of the admin, served or refused, with what `trace`
    /// names (FR-015). Hécate must be unsealed, as for the rest of the admin's management
    /// (FR-014).
    fn audited<T>(
        &self,
        operation: Operation,
        detail: &str,
        trace: Trace,
        work: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let outcome = self.sealing.unsealed_epoch().and_then(|_| work());
        let result = match &outcome {
            Ok(_) => AccessResult::Success,
            Err(error) => result_for(error),
        };
        self.log(operation, result, detail, trace)?;
        outcome
    }

    /// Leaves a row for a recognized operation whose arguments were malformed.
    pub fn record_invalid_request(
        &self,
        operation: Operation,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.log(
            operation,
            AccessResult::InvalidInput,
            detail,
            Trace::default(),
        )
    }

    /// Applies an order without an expected version (FR-008): it reads the policy, decides, and
    /// writes the event at the version it read; when another order got ahead, it reads again and
    /// decides again. Returns the version after the order and whether it changed something.
    fn change(
        &self,
        identity: &IdentityName,
        decide: impl Fn(&AccessPolicy, OffsetDateTime) -> Result<Option<PolicyEvent>, ServiceError>,
    ) -> Result<(u64, bool), ServiceError> {
        let stream = stream_name(identity);
        for _ in 0..MAX_RACES {
            let policy = self.policy_of(identity)?;
            let Some(event) = decide(&policy, self.clock.now())? else {
                return Ok((policy.version, false));
            };
            let records = [event.encode()];
            match self.store.transaction(&mut |writes| {
                writes.append(&stream, &records, policy.version)?;
                Ok(())
            }) {
                Ok(()) => return Ok((policy.version + 1, true)),
                Err(ServiceError::Concurrency(_)) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(ServiceError::internal("TooManyRaces"))
    }

    fn secret_exists(&self, path: &SecretPath) -> Result<bool, ServiceError> {
        Ok(!self.secrets.load(&path.stream_name())?.is_empty())
    }

    /// Lets an identity read a secret of another identity that exists (FR-006): a reserved
    /// identity and a secret of the identity itself are `validation`, a secret that does not
    /// exist is `not_found`. The identity does not need to have enrolled.
    pub fn grant_read(&self, identity: &str, secret: &str) -> Result<PolicyChange, ServiceError> {
        let trace = Trace::naming(identity, Some(secret));
        self.audited(Operation::PolicyGrant, "read", trace, || {
            let identity = IdentityName::parse(identity)?;
            let path = SecretPath::parse(secret)?;
            let (version, changed) = self.change(&identity, |policy, at| {
                let event = policy.grant_read(&path, ChangedBy::Admin, at)?;
                if event.is_some() && !self.secret_exists(&path)? {
                    return Err(HecateError::not_found("secret not found").into());
                }
                Ok(event)
            })?;
            Ok(PolicyChange {
                identity,
                secret: Some(path),
                version,
                changed,
            })
        })
    }

    /// Stops an identity from reading a secret of another identity (FR-006, FR-010).
    pub fn revoke_read(&self, identity: &str, secret: &str) -> Result<PolicyChange, ServiceError> {
        let trace = Trace::naming(identity, Some(secret));
        self.audited(Operation::PolicyRevoke, "read", trace, || {
            let identity = IdentityName::parse(identity)?;
            let path = SecretPath::parse(secret)?;
            let (version, changed) = self.change(&identity, |policy, at| {
                Ok(policy.revoke_read(&path, ChangedBy::Admin, at)?)
            })?;
            Ok(PolicyChange {
                identity,
                secret: Some(path),
                version,
                changed,
            })
        })
    }

    /// Lets an identity read the aggregate status of Hécate (FR-007).
    pub fn grant_status(&self, identity: &str) -> Result<PolicyChange, ServiceError> {
        let trace = Trace::naming(identity, None);
        self.audited(Operation::PolicyGrant, "status", trace, || {
            let identity = IdentityName::parse(identity)?;
            let (version, changed) = self.change(&identity, |policy, at| {
                Ok(policy.grant_status(ChangedBy::Admin, at)?)
            })?;
            Ok(PolicyChange {
                identity,
                secret: None,
                version,
                changed,
            })
        })
    }

    /// Stops an identity from reading the aggregate status (FR-007, FR-010).
    pub fn revoke_status(&self, identity: &str) -> Result<PolicyChange, ServiceError> {
        let trace = Trace::naming(identity, None);
        self.audited(Operation::PolicyRevoke, "status", trace, || {
            let identity = IdentityName::parse(identity)?;
            let (version, changed) = self.change(&identity, |policy, at| {
                Ok(policy.revoke_status(ChangedBy::Admin, at)?)
            })?;
            Ok(PolicyChange {
                identity,
                secret: None,
                version,
                changed,
            })
        })
    }

    /// The identities whose policy has ever changed, also the ones with nothing granted now,
    /// by name (FR-009).
    pub fn list(&self) -> Result<Vec<AccessPolicy>, ServiceError> {
        self.audited(Operation::PolicyList, "list", Trace::default(), || {
            let mut policies = Vec::new();
            for stream in self.store.streams(STREAM_PREFIX)? {
                let name = stream.strip_prefix(STREAM_PREFIX).unwrap_or_default();
                let identity = IdentityName::parse(name).map_err(|_| corrupt())?;
                policies.push(self.policy_of(&identity)?);
            }
            policies.sort_by(|a, b| a.identity.as_str().cmp(b.identity.as_str()));
            Ok(policies)
        })
    }

    /// The policy of an identity with its history; without changes, the empty one at version 0
    /// (FR-009). The reserved identities have none.
    pub fn get(&self, identity: &str) -> Result<AccessPolicy, ServiceError> {
        let trace = Trace::naming(identity, None);
        self.audited(Operation::PolicyGet, "get", trace, || {
            let identity = IdentityName::parse(identity)?;
            if identity.is_reserved() {
                return Err(
                    HecateError::invalid(format!("the identity '{identity}' is reserved")).into(),
                );
            }
            self.policy_of(&identity)
        })
    }
}
