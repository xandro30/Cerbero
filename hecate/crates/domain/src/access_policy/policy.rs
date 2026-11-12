//! The AccessPolicy aggregate (stream `policy/<identity>`, data-model.md): the secrets of other
//! identities that an identity reads and whether it has the status permission. The default rule,
//! that an identity reads its own secrets, is not stored: without a stream, the policy is empty
//! and at version 0.

use std::collections::BTreeSet;

use time::OffsetDateTime;

use crate::events::{ChangedBy, InvalidEvents, PolicyEvent};
use crate::identity::names::IdentityName;
use crate::sealing::errors::HecateError;
use crate::secrets::path::SecretPath;

/// What the streams of the Access Policies start with.
pub const STREAM_PREFIX: &str = "policy/";

/// The stream of the Access Policy of an identity.
pub fn stream_name(identity: &IdentityName) -> String {
    format!("{STREAM_PREFIX}{identity}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPolicy {
    pub identity: IdentityName,
    /// The paths of the secrets of other identities that it reads.
    reads: BTreeSet<String>,
    status: bool,
    /// The version of its stream: how many events it has.
    pub version: u64,
    /// Its events; the version that each one created is its place, from 1.
    pub history: Vec<PolicyEvent>,
}

impl AccessPolicy {
    /// The policy of an identity without a stream: no exceptions, no status permission and
    /// version 0.
    pub fn new(identity: IdentityName) -> Self {
        Self {
            identity,
            reads: BTreeSet::new(),
            status: false,
            version: 0,
            history: Vec::new(),
        }
    }

    pub fn from_events(
        identity: IdentityName,
        events: &[PolicyEvent],
    ) -> Result<Self, InvalidEvents> {
        let mut policy = Self::new(identity);
        for event in events {
            policy.apply(event)?;
        }
        Ok(policy)
    }

    /// The policy as it was at `instant`: its events up to then, in the order they were written
    /// (SC-006).
    pub fn as_of(&self, instant: OffsetDateTime) -> Result<Self, InvalidEvents> {
        let mut policy = Self::new(self.identity.clone());
        for event in self
            .history
            .iter()
            .take_while(|event| event.at() <= instant)
        {
            policy.apply(event)?;
        }
        Ok(policy)
    }

    pub fn apply(&mut self, event: &PolicyEvent) -> Result<(), InvalidEvents> {
        match event {
            PolicyEvent::ReadGranted { secret, .. } => {
                if self.is_own(secret) {
                    return Err(InvalidEvents(
                        "an identity reads its own secrets without an exception",
                    ));
                }
                if !self.reads.insert(secret.to_string()) {
                    return Err(InvalidEvents("a read is granted only once"));
                }
            }
            PolicyEvent::ReadRevoked { secret, .. } => {
                if !self.reads.remove(&secret.to_string()) {
                    return Err(InvalidEvents("only a granted read is revoked"));
                }
            }
            PolicyEvent::StatusGranted { .. } => {
                if self.status {
                    return Err(InvalidEvents("the status permission is granted only once"));
                }
                self.status = true;
            }
            PolicyEvent::StatusRevoked { .. } => {
                if !self.status {
                    return Err(InvalidEvents("only a granted status permission is revoked"));
                }
                self.status = false;
            }
        }
        self.version += 1;
        self.history.push(event.clone());
        Ok(())
    }

    fn is_own(&self, secret: &SecretPath) -> bool {
        secret.owner() == self.identity.as_str()
    }

    /// The paths of the secrets of other identities that it reads, in the order of their text.
    pub fn reads(&self) -> impl Iterator<Item = &str> {
        self.reads.iter().map(String::as_str)
    }

    pub fn has_status(&self) -> bool {
        self.status
    }

    /// Whether the identity resolves `path`: its own secrets always, by the default rule
    /// (FR-005), and a secret of another identity only with an exception for that exact path,
    /// never for a prefix (FR-006).
    pub fn can_read(&self, path: &SecretPath) -> bool {
        self.is_own(path) || self.reads.contains(&path.to_string())
    }

    /// The reserved identities receive neither exceptions nor the status permission (FR-006,
    /// FR-007).
    fn require_not_reserved(&self) -> Result<(), HecateError> {
        if self.identity.is_reserved() {
            return Err(HecateError::invalid(format!(
                "the identity '{}' is reserved",
                self.identity
            )));
        }
        Ok(())
    }

    /// An identity reads its own secrets without an exception (FR-006).
    fn require_foreign(&self, secret: &SecretPath) -> Result<(), HecateError> {
        if self.is_own(secret) {
            return Err(HecateError::invalid(
                "an identity reads its own secrets without an exception",
            ));
        }
        Ok(())
    }

    /// The grant of a read of another identity's secret, or nothing when it has it already
    /// (FR-006, FR-008). That the secret exists is for the service to check, since it sees the
    /// secrets.
    pub fn grant_read(
        &self,
        secret: &SecretPath,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<PolicyEvent>, HecateError> {
        self.require_not_reserved()?;
        self.require_foreign(secret)?;
        Ok(
            (!self.reads.contains(&secret.to_string())).then(|| PolicyEvent::ReadGranted {
                at,
                secret: secret.clone(),
                by,
            }),
        )
    }

    /// The revocation of a read, or nothing when it does not have it (FR-008).
    pub fn revoke_read(
        &self,
        secret: &SecretPath,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<PolicyEvent>, HecateError> {
        self.require_not_reserved()?;
        self.require_foreign(secret)?;
        Ok(self
            .reads
            .contains(&secret.to_string())
            .then(|| PolicyEvent::ReadRevoked {
                at,
                secret: secret.clone(),
                by,
            }))
    }

    /// The grant of the status permission, or nothing when it has it already (FR-007, FR-008).
    pub fn grant_status(
        &self,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<PolicyEvent>, HecateError> {
        self.require_not_reserved()?;
        Ok((!self.status).then_some(PolicyEvent::StatusGranted { at, by }))
    }

    /// The revocation of the status permission, or nothing when it does not have it (FR-008).
    pub fn revoke_status(
        &self,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<PolicyEvent>, HecateError> {
        self.require_not_reserved()?;
        Ok(self.status.then_some(PolicyEvent::StatusRevoked { at, by }))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::datetime;

    use super::*;

    const AT: OffsetDateTime = datetime!(2026-11-11 10:00 UTC);
    const BY: ChangedBy = ChangedBy::Admin;

    fn hermes() -> IdentityName {
        IdentityName::parse("hermes").unwrap()
    }

    fn path(text: &str) -> SecretPath {
        SecretPath::parse(text).unwrap()
    }

    fn smtp() -> SecretPath {
        path("secret://hades/smtp/password")
    }

    fn policy(events: &[PolicyEvent]) -> AccessPolicy {
        AccessPolicy::from_events(hermes(), events).unwrap()
    }

    fn read_granted(secret: SecretPath, at: OffsetDateTime) -> PolicyEvent {
        PolicyEvent::ReadGranted { at, secret, by: BY }
    }

    #[test]
    fn an_identity_without_a_stream_has_the_empty_policy() {
        let empty = AccessPolicy::new(hermes());
        assert_eq!(empty, policy(&[]));
        assert_eq!(empty.version, 0);
        assert!(empty.reads().next().is_none());
        assert!(!empty.has_status());
        assert!(empty.history.is_empty());
        assert_eq!(stream_name(&hermes()), "policy/hermes");
    }

    #[test]
    fn a_read_is_granted_and_revoked_with_its_events() {
        let granted = AccessPolicy::new(hermes())
            .grant_read(&smtp(), BY, AT)
            .unwrap();
        assert_eq!(granted, Some(read_granted(smtp(), AT)));
        let with_read = policy(&[read_granted(smtp(), AT)]);
        assert_eq!(
            with_read.reads().collect::<Vec<_>>(),
            ["secret://hades/smtp/password"]
        );
        let revoked = with_read.revoke_read(&smtp(), BY, AT).unwrap();
        assert_eq!(
            revoked,
            Some(PolicyEvent::ReadRevoked {
                at: AT,
                secret: smtp(),
                by: BY
            })
        );
    }

    #[test]
    fn the_status_permission_is_granted_and_revoked_with_its_events() {
        let granted = AccessPolicy::new(hermes()).grant_status(BY, AT).unwrap();
        assert_eq!(granted, Some(PolicyEvent::StatusGranted { at: AT, by: BY }));
        let with_status = policy(&[PolicyEvent::StatusGranted { at: AT, by: BY }]);
        assert!(with_status.has_status());
        let revoked = with_status.revoke_status(BY, AT).unwrap();
        assert_eq!(revoked, Some(PolicyEvent::StatusRevoked { at: AT, by: BY }));
    }

    #[test]
    fn an_order_that_changes_nothing_writes_no_event() {
        let empty = AccessPolicy::new(hermes());
        assert_eq!(empty.revoke_read(&smtp(), BY, AT).unwrap(), None);
        assert_eq!(empty.revoke_status(BY, AT).unwrap(), None);
        let full = policy(&[
            read_granted(smtp(), AT),
            PolicyEvent::StatusGranted { at: AT, by: BY },
        ]);
        assert_eq!(full.grant_read(&smtp(), BY, AT).unwrap(), None);
        assert_eq!(full.grant_status(BY, AT).unwrap(), None);
    }

    #[test]
    fn an_identity_reads_its_own_secrets_and_only_the_exact_paths_shared_with_it() {
        let shared = policy(&[read_granted(smtp(), AT)]);
        for own in [
            "secret://hermes/telegram/bot_token",
            "secret://hermes/any/deep/path",
        ] {
            assert!(shared.can_read(&path(own)), "{own}");
            assert!(AccessPolicy::new(hermes()).can_read(&path(own)), "{own}");
        }
        assert!(shared.can_read(&smtp()));
        for other in [
            "secret://hades/smtp",
            "secret://hades/smtp/password/old",
            "secret://hades/smtp/user",
            "secret://hermes-old/smtp/password",
        ] {
            assert!(!shared.can_read(&path(other)), "{other}");
        }
        assert!(!AccessPolicy::new(hermes()).can_read(&smtp()));
    }

    #[test]
    fn reserved_identities_and_their_own_secrets_are_refused() {
        for reserved in ["hecate", "admin"] {
            let policy = AccessPolicy::new(IdentityName::parse(reserved).unwrap());
            assert!(policy.grant_read(&smtp(), BY, AT).is_err(), "{reserved}");
            assert!(policy.revoke_read(&smtp(), BY, AT).is_err(), "{reserved}");
            assert!(policy.grant_status(BY, AT).is_err(), "{reserved}");
            assert!(policy.revoke_status(BY, AT).is_err(), "{reserved}");
        }
        let own = path("secret://hermes/telegram/bot_token");
        let empty = AccessPolicy::new(hermes());
        for refused in [
            empty.grant_read(&own, BY, AT),
            empty.revoke_read(&own, BY, AT),
        ] {
            assert!(
                matches!(refused, Err(HecateError::InvalidInput(_))),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn the_version_and_the_history_come_from_the_events_and_a_past_instant_gives_its_policy() {
        let first = AT;
        let second = AT + Duration::hours(1);
        let third = AT + Duration::hours(2);
        let events = [
            read_granted(smtp(), first),
            PolicyEvent::StatusGranted { at: second, by: BY },
            PolicyEvent::ReadRevoked {
                at: third,
                secret: smtp(),
                by: BY,
            },
        ];
        let now = policy(&events);
        assert_eq!(now.version, 3);
        assert_eq!(now.history, events);
        assert!(!now.can_read(&smtp()));
        assert!(now.has_status());
        let before = now.as_of(first - Duration::seconds(1)).unwrap();
        assert_eq!(before, AccessPolicy::new(hermes()));
        let at_first = now.as_of(first).unwrap();
        assert!(at_first.can_read(&smtp()));
        assert_eq!(at_first.version, 1);
        let between = now.as_of(second + Duration::minutes(30)).unwrap();
        assert!(between.can_read(&smtp()) && between.has_status());
        assert_eq!(between.version, 2);
        assert_eq!(now.as_of(third).unwrap(), now);
    }

    #[test]
    fn streams_that_break_the_rules_are_refused() {
        let status = PolicyEvent::StatusGranted { at: AT, by: BY };
        for broken in [
            vec![read_granted(smtp(), AT), read_granted(smtp(), AT)],
            vec![PolicyEvent::ReadRevoked {
                at: AT,
                secret: smtp(),
                by: BY,
            }],
            vec![read_granted(path("secret://hermes/telegram/bot_token"), AT)],
            vec![status.clone(), status],
            vec![PolicyEvent::StatusRevoked { at: AT, by: BY }],
        ] {
            assert!(
                AccessPolicy::from_events(hermes(), &broken).is_err(),
                "{broken:?}"
            );
        }
    }

    #[test]
    fn the_reads_are_listed_in_the_order_of_their_paths() {
        let events = [
            read_granted(path("secret://hades/smtp/user"), AT),
            read_granted(path("secret://ares/db/password"), AT),
            read_granted(path("secret://hades/smtp/password"), AT),
        ];
        assert_eq!(
            policy(&events).reads().collect::<Vec<_>>(),
            [
                "secret://ares/db/password",
                "secret://hades/smtp/password",
                "secret://hades/smtp/user",
            ]
        );
    }
}
