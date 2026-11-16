//! The AppAccess aggregate (stream `access/<app>`, data-model.md): the identities that may call an
//! app. Access is denied by default: without a stream, the list is empty, at version 0, and no
//! identity may call the app (FR-010). Only identities of apps take part, never a child identity:
//! the names of Hécate have a single segment (FR-011).

use std::collections::BTreeSet;

use time::OffsetDateTime;

use crate::events::{AccessEvent, ChangedBy, InvalidEvents};
use crate::identity::names::IdentityName;
use crate::sealing::errors::HecateError;

/// What the streams of the lists of access start with.
pub const STREAM_PREFIX: &str = "access/";

/// The stream of the list of access of an app.
pub fn stream_name(app: &IdentityName) -> String {
    format!("{STREAM_PREFIX}{app}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppAccess {
    pub app: IdentityName,
    /// The identities that may call the app.
    callers: BTreeSet<IdentityName>,
    /// The version of its stream: how many events it has.
    pub version: u64,
    /// Its events; the version that each one created is its place, from 1.
    pub history: Vec<AccessEvent>,
}

impl AppAccess {
    /// The list of an app without a stream: no one may call it, at version 0.
    pub fn new(app: IdentityName) -> Self {
        Self {
            app,
            callers: BTreeSet::new(),
            version: 0,
            history: Vec::new(),
        }
    }

    pub fn from_events(app: IdentityName, events: &[AccessEvent]) -> Result<Self, InvalidEvents> {
        let mut access = Self::new(app);
        for event in events {
            access.apply(event)?;
        }
        Ok(access)
    }

    /// The list as it was at `instant`: its events up to then, in the order they were written
    /// (SC-006).
    pub fn as_of(&self, instant: OffsetDateTime) -> Result<Self, InvalidEvents> {
        let mut access = Self::new(self.app.clone());
        for event in self
            .history
            .iter()
            .take_while(|event| event.at() <= instant)
        {
            access.apply(event)?;
        }
        Ok(access)
    }

    pub fn apply(&mut self, event: &AccessEvent) -> Result<(), InvalidEvents> {
        let caller = event.caller();
        if self.app.is_reserved() || caller.is_reserved() || *caller == self.app {
            return Err(InvalidEvents(
                "a list of access names neither a reserved identity nor the app itself",
            ));
        }
        match event {
            AccessEvent::CallerGranted { .. } => {
                if !self.callers.insert(caller.clone()) {
                    return Err(InvalidEvents("an access is granted only once"));
                }
            }
            AccessEvent::CallerRevoked { .. } => {
                if !self.callers.remove(caller) {
                    return Err(InvalidEvents("only a granted access is revoked"));
                }
            }
        }
        self.version += 1;
        self.history.push(event.clone());
        Ok(())
    }

    /// The identities that may call the app, in the order of their names.
    pub fn callers(&self) -> impl Iterator<Item = &str> {
        self.callers.iter().map(IdentityName::as_str)
    }

    /// Whether `identity` may call the app: only if it is in the list (FR-010).
    pub fn can_call(&self, identity: &IdentityName) -> bool {
        self.callers.contains(identity)
    }

    /// Neither the app nor the caller is `hecate` or `admin`, and an app is not granted access to
    /// itself (FR-004).
    fn require_allowed(&self, caller: &IdentityName) -> Result<(), HecateError> {
        for identity in [&self.app, caller] {
            if identity.is_reserved() {
                return Err(HecateError::invalid(format!(
                    "the identity '{identity}' is reserved"
                )));
            }
        }
        if *caller == self.app {
            return Err(HecateError::invalid(format!(
                "the app '{}' is not granted access to itself",
                self.app
            )));
        }
        Ok(())
    }

    /// The grant of the access of `caller`, or nothing when it has it already (FR-002).
    pub fn grant(
        &self,
        caller: &IdentityName,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<AccessEvent>, HecateError> {
        self.require_allowed(caller)?;
        Ok(
            (!self.callers.contains(caller)).then(|| AccessEvent::CallerGranted {
                at,
                caller: caller.clone(),
                by,
            }),
        )
    }

    /// The revocation of the access of `caller`, or nothing when it does not have it (FR-002).
    pub fn revoke(
        &self,
        caller: &IdentityName,
        by: ChangedBy,
        at: OffsetDateTime,
    ) -> Result<Option<AccessEvent>, HecateError> {
        self.require_allowed(caller)?;
        Ok(self
            .callers
            .contains(caller)
            .then(|| AccessEvent::CallerRevoked {
                at,
                caller: caller.clone(),
                by,
            }))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use time::Duration;
    use time::macros::datetime;

    use super::*;

    const AT: OffsetDateTime = datetime!(2026-11-16 10:00 UTC);
    const BY: ChangedBy = ChangedBy::Admin;

    fn name(text: &str) -> IdentityName {
        IdentityName::parse(text).unwrap()
    }

    fn hermes() -> IdentityName {
        name("hermes")
    }

    fn granted(caller: &str, at: OffsetDateTime) -> AccessEvent {
        AccessEvent::CallerGranted {
            at,
            caller: name(caller),
            by: BY,
        }
    }

    fn revoked(caller: &str, at: OffsetDateTime) -> AccessEvent {
        AccessEvent::CallerRevoked {
            at,
            caller: name(caller),
            by: BY,
        }
    }

    fn access(events: &[AccessEvent]) -> AppAccess {
        AppAccess::from_events(hermes(), events).unwrap()
    }

    #[test]
    fn an_app_without_a_stream_has_the_empty_list_and_no_one_may_call_it() {
        let empty = AppAccess::new(hermes());
        assert_eq!(empty, access(&[]));
        assert_eq!(empty.version, 0);
        assert!(empty.callers().next().is_none());
        assert!(empty.history.is_empty());
        for anyone in ["hades", "caller", "estigia"] {
            assert!(!empty.can_call(&name(anyone)), "{anyone}");
        }
        assert_eq!(stream_name(&hermes()), "access/hermes");
    }

    #[test]
    fn an_access_is_granted_and_revoked_with_its_events() {
        let grant = AppAccess::new(hermes())
            .grant(&name("hades"), BY, AT)
            .unwrap();
        assert_eq!(grant, Some(granted("hades", AT)));
        let with_hades = access(&[granted("hades", AT)]);
        assert!(with_hades.can_call(&name("hades")));
        assert!(!with_hades.can_call(&name("caller")));
        assert_eq!(with_hades.callers().collect::<Vec<_>>(), ["hades"]);
        assert_eq!(with_hades.version, 1);
        let revoke = with_hades.revoke(&name("hades"), BY, AT).unwrap();
        assert_eq!(revoke, Some(revoked("hades", AT)));
        let without = access(&[granted("hades", AT), revoked("hades", AT)]);
        assert!(!without.can_call(&name("hades")));
        assert_eq!(without.version, 2);
    }

    #[test]
    fn an_order_that_changes_nothing_writes_no_event() {
        let empty = AppAccess::new(hermes());
        assert_eq!(empty.revoke(&name("hades"), BY, AT).unwrap(), None);
        let with_hades = access(&[granted("hades", AT)]);
        assert_eq!(with_hades.grant(&name("hades"), BY, AT).unwrap(), None);
    }

    #[test]
    fn reserved_identities_and_the_app_itself_are_refused() {
        for reserved in ["hecate", "admin"] {
            let app = AppAccess::new(name(reserved));
            for refused in [
                app.grant(&name("hades"), BY, AT),
                app.revoke(&name("hades"), BY, AT),
            ] {
                assert!(
                    matches!(refused, Err(HecateError::InvalidInput(_))),
                    "{reserved} as the app: {refused:?}"
                );
            }
            let empty = AppAccess::new(hermes());
            for refused in [
                empty.grant(&name(reserved), BY, AT),
                empty.revoke(&name(reserved), BY, AT),
            ] {
                assert!(
                    matches!(refused, Err(HecateError::InvalidInput(_))),
                    "{reserved} as the caller: {refused:?}"
                );
            }
        }
        let empty = AppAccess::new(hermes());
        for refused in [
            empty.grant(&hermes(), BY, AT),
            empty.revoke(&hermes(), BY, AT),
        ] {
            assert!(
                matches!(refused, Err(HecateError::InvalidInput(_))),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn only_identities_of_apps_have_a_name_a_child_identity_has_none() {
        for child in ["hades/integration/x", "hades/", "/hades"] {
            assert!(IdentityName::parse(child).is_err(), "{child}");
        }
    }

    #[test]
    fn the_version_and_the_history_come_from_the_events_and_a_past_instant_gives_its_list() {
        let first = AT;
        let second = AT + Duration::hours(1);
        let third = AT + Duration::hours(2);
        let events = [
            granted("hades", first),
            granted("caller", second),
            revoked("hades", third),
        ];
        let now = access(&events);
        assert_eq!(now.version, 3);
        assert_eq!(now.history, events);
        assert_eq!(now.callers().collect::<Vec<_>>(), ["caller"]);
        let before = now.as_of(first - Duration::seconds(1)).unwrap();
        assert_eq!(before, AppAccess::new(hermes()));
        let at_first = now.as_of(first).unwrap();
        assert_eq!(at_first.callers().collect::<Vec<_>>(), ["hades"]);
        assert_eq!(at_first.version, 1);
        let between = now.as_of(second + Duration::minutes(30)).unwrap();
        assert_eq!(between.callers().collect::<Vec<_>>(), ["caller", "hades"]);
        assert_eq!(between.version, 2);
        assert_eq!(now.as_of(third).unwrap(), now);
    }

    #[test]
    fn streams_that_break_the_rules_are_refused() {
        for broken in [
            vec![granted("hades", AT), granted("hades", AT)],
            vec![revoked("hades", AT)],
            vec![granted("hermes", AT)],
            vec![granted("hecate", AT)],
            vec![granted("admin", AT)],
        ] {
            assert!(
                AppAccess::from_events(hermes(), &broken).is_err(),
                "{broken:?}"
            );
        }
    }

    #[test]
    fn the_callers_are_listed_in_the_order_of_their_names() {
        let events = [
            granted("hades", AT),
            granted("caller", AT),
            granted("estigia", AT),
        ];
        assert_eq!(
            access(&events).callers().collect::<Vec<_>>(),
            ["caller", "estigia", "hades"]
        );
    }

    fn caller_name() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec!["hades", "caller", "estigia", "orthros"])
    }

    proptest! {
        /// After any sequence of valid orders, the callers are the set they leave and the version
        /// counts the orders that changed something.
        #[test]
        fn the_list_is_the_set_that_the_orders_leave(
            orders in prop::collection::vec((any::<bool>(), caller_name()), 0..40),
        ) {
            let mut list = AppAccess::new(hermes());
            let mut model = std::collections::BTreeSet::new();
            let mut changes = 0u64;
            for (grant, caller) in orders {
                let caller = name(caller);
                let event = if grant {
                    list.grant(&caller, BY, AT).unwrap()
                } else {
                    list.revoke(&caller, BY, AT).unwrap()
                };
                let changed = if grant {
                    model.insert(caller.to_string())
                } else {
                    model.remove(caller.as_str())
                };
                prop_assert_eq!(event.is_some(), changed);
                if let Some(event) = event {
                    list.apply(&event).unwrap();
                    changes += 1;
                }
            }
            prop_assert_eq!(list.callers().map(str::to_string).collect::<Vec<_>>(),
                            model.into_iter().collect::<Vec<_>>());
            prop_assert_eq!(list.version, changes);
        }
    }
}
