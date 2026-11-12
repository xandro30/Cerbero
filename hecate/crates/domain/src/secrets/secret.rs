//! The `Secret` aggregate with its versions (Event Sourcing): its state is only what its events
//! say. The decision methods return the events of a change without applying them; the caller
//! stores them and applies them.

use std::collections::BTreeMap;

use serde_json::Value;
use time::{Duration, OffsetDateTime};

use crate::events::{EventRecord, Fields, InvalidEvents, event_data, instant_value};
use crate::sealing::errors::HecateError;
use crate::secrets::path::SecretPath;
use crate::secrets::rotation::RotationInterval;
use crate::secrets::value_type::ValueType;

pub const DEFAULT_MAX_VERSIONS: u64 = 10;
/// So that the version just rotated out keeps its grace period (SC-005).
pub const MIN_VERSIONS: u64 = 2;
pub const MAX_VERSIONS_LIMIT: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionState {
    Active,
    Deprecated,
    Revoked,
    Destroyed,
}

impl VersionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::Revoked => "revoked",
            Self::Destroyed => "destroyed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestroyReason {
    Admin,
    MaxVersions,
}

impl DestroyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::MaxVersions => "max_versions",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretEvent {
    SecretCreated {
        at: OffsetDateTime,
        path: String,
        owner: String,
        value_type: String,
        max_versions: u64,
        rotate_every: Option<String>,
        expires_at: Option<OffsetDateTime>,
    },
    VersionAdded {
        at: OffsetDateTime,
        version: u64,
        fingerprint: String,
        key_version: u64,
    },
    VersionDeprecated {
        at: OffsetDateTime,
        version: u64,
    },
    VersionRevoked {
        at: OffsetDateTime,
        version: u64,
    },
    VersionRestored {
        at: OffsetDateTime,
        version: u64,
    },
    VersionPromoted {
        at: OffsetDateTime,
        version: u64,
    },
    VersionDestroyed {
        at: OffsetDateTime,
        version: u64,
        reason: String,
    },
}

impl SecretEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SecretCreated { .. } => "SecretCreated",
            Self::VersionAdded { .. } => "VersionAdded",
            Self::VersionDeprecated { .. } => "VersionDeprecated",
            Self::VersionRevoked { .. } => "VersionRevoked",
            Self::VersionRestored { .. } => "VersionRestored",
            Self::VersionPromoted { .. } => "VersionPromoted",
            Self::VersionDestroyed { .. } => "VersionDestroyed",
        }
    }

    pub fn at(&self) -> OffsetDateTime {
        match self {
            Self::SecretCreated { at, .. }
            | Self::VersionAdded { at, .. }
            | Self::VersionDeprecated { at, .. }
            | Self::VersionRevoked { at, .. }
            | Self::VersionRestored { at, .. }
            | Self::VersionPromoted { at, .. }
            | Self::VersionDestroyed { at, .. } => *at,
        }
    }

    pub fn encode(&self) -> EventRecord {
        let data = match self {
            Self::SecretCreated {
                path,
                owner,
                value_type,
                max_versions,
                rotate_every,
                expires_at,
                ..
            } => event_data(vec![
                ("path", Value::from(path.as_str())),
                ("owner", Value::from(owner.as_str())),
                ("value_type", Value::from(value_type.as_str())),
                ("max_versions", Value::from(*max_versions)),
                (
                    "rotate_every",
                    rotate_every.as_deref().map_or(Value::Null, Value::from),
                ),
                ("expires_at", instant_value(*expires_at)),
            ]),
            Self::VersionAdded {
                version,
                fingerprint,
                key_version,
                ..
            } => event_data(vec![
                ("version", Value::from(*version)),
                ("fingerprint", Value::from(fingerprint.as_str())),
                ("key_version", Value::from(*key_version)),
            ]),
            Self::VersionDeprecated { version, .. }
            | Self::VersionRevoked { version, .. }
            | Self::VersionRestored { version, .. }
            | Self::VersionPromoted { version, .. } => {
                event_data(vec![("version", Value::from(*version))])
            }
            Self::VersionDestroyed {
                version, reason, ..
            } => event_data(vec![
                ("version", Value::from(*version)),
                ("reason", Value::from(reason.as_str())),
            ]),
        };
        EventRecord {
            kind: self.kind().to_string(),
            data,
            at: self.at(),
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        let at = record.at;
        let version_only = |record: &EventRecord| -> Result<u64, InvalidEvents> {
            Fields::exactly(record, &["version"])?.number("version")
        };
        Ok(match record.kind.as_str() {
            "SecretCreated" => {
                let fields = Fields::exactly(
                    record,
                    &[
                        "path",
                        "owner",
                        "value_type",
                        "max_versions",
                        "rotate_every",
                        "expires_at",
                    ],
                )?;
                Self::SecretCreated {
                    at,
                    path: fields.text("path")?,
                    owner: fields.text("owner")?,
                    value_type: fields.text("value_type")?,
                    max_versions: fields.number("max_versions")?,
                    rotate_every: fields.optional_text("rotate_every")?,
                    expires_at: fields.optional_instant("expires_at")?,
                }
            }
            "VersionAdded" => {
                let fields = Fields::exactly(record, &["version", "fingerprint", "key_version"])?;
                Self::VersionAdded {
                    at,
                    version: fields.number("version")?,
                    fingerprint: fields.text("fingerprint")?,
                    key_version: fields.number("key_version")?,
                }
            }
            "VersionDeprecated" => Self::VersionDeprecated {
                at,
                version: version_only(record)?,
            },
            "VersionRevoked" => Self::VersionRevoked {
                at,
                version: version_only(record)?,
            },
            "VersionRestored" => Self::VersionRestored {
                at,
                version: version_only(record)?,
            },
            "VersionPromoted" => Self::VersionPromoted {
                at,
                version: version_only(record)?,
            },
            "VersionDestroyed" => {
                let fields = Fields::exactly(record, &["version", "reason"])?;
                Self::VersionDestroyed {
                    at,
                    version: fields.number("version")?,
                    reason: fields.text("reason")?,
                }
            }
            _ => return Err(InvalidEvents("unknown secret event")),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretVersion {
    pub number: u64,
    pub state: VersionState,
    pub created_at: OffsetDateTime,
    pub deprecated_at: Option<OffsetDateTime>,
    pub fingerprint: String,
    pub key_version: u64,
}

/// What the admin chose for a new secret; `max_versions` is checked when the secret is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretOptions {
    pub max_versions: i64,
    pub rotate_every: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
}

impl Default for SecretOptions {
    fn default() -> Self {
        Self {
            max_versions: DEFAULT_MAX_VERSIONS as i64,
            rotate_every: None,
            expires_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    pub path: SecretPath,
    pub owner: String,
    pub value_type: ValueType,
    pub max_versions: u64,
    pub rotate_every: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub versions: BTreeMap<u64, SecretVersion>,
    pub stream_version: u64,
}

impl Secret {
    pub fn from_events(events: &[SecretEvent]) -> Result<Self, InvalidEvents> {
        let Some(SecretEvent::SecretCreated { path, .. }) = events.first() else {
            return Err(InvalidEvents("a secret stream starts with SecretCreated"));
        };
        let mut secret = Self {
            path: SecretPath::parse(path)
                .map_err(|_| InvalidEvents("the secret path is not valid"))?,
            owner: String::new(),
            value_type: ValueType::String,
            max_versions: DEFAULT_MAX_VERSIONS,
            rotate_every: None,
            expires_at: None,
            versions: BTreeMap::new(),
            stream_version: 0,
        };
        for event in events {
            secret.apply(event)?;
        }
        Ok(secret)
    }

    fn version_mut(&mut self, number: u64) -> Result<&mut SecretVersion, InvalidEvents> {
        self.versions.get_mut(&number).ok_or(InvalidEvents(
            "an event names a version that does not exist",
        ))
    }

    pub fn apply(&mut self, event: &SecretEvent) -> Result<(), InvalidEvents> {
        match event {
            SecretEvent::SecretCreated {
                path,
                owner,
                value_type,
                max_versions,
                rotate_every,
                expires_at,
                ..
            } => {
                self.path = SecretPath::parse(path)
                    .map_err(|_| InvalidEvents("the secret path is not valid"))?;
                self.owner.clone_from(owner);
                self.value_type = ValueType::parse(value_type)
                    .map_err(|_| InvalidEvents("the value type is not valid"))?;
                self.max_versions = *max_versions;
                self.rotate_every.clone_from(rotate_every);
                self.expires_at = *expires_at;
            }
            SecretEvent::VersionAdded {
                at,
                version,
                fingerprint,
                key_version,
            } => {
                self.versions.insert(
                    *version,
                    SecretVersion {
                        number: *version,
                        state: VersionState::Active,
                        created_at: *at,
                        deprecated_at: None,
                        fingerprint: fingerprint.clone(),
                        key_version: *key_version,
                    },
                );
            }
            SecretEvent::VersionDeprecated { at, version }
            | SecretEvent::VersionRestored { at, version } => {
                let version = self.version_mut(*version)?;
                version.state = VersionState::Deprecated;
                version.deprecated_at = Some(*at);
            }
            SecretEvent::VersionRevoked { version, .. } => {
                self.version_mut(*version)?.state = VersionState::Revoked;
            }
            SecretEvent::VersionPromoted { version, .. } => {
                let version = self.version_mut(*version)?;
                version.state = VersionState::Active;
                version.deprecated_at = None;
            }
            SecretEvent::VersionDestroyed { version, .. } => {
                self.version_mut(*version)?.state = VersionState::Destroyed;
            }
        }
        self.stream_version += 1;
        Ok(())
    }

    /// The effective state: a `deprecated` version past its grace period counts as `revoked`.
    pub fn state_of(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
    ) -> Result<VersionState, HecateError> {
        let version = self
            .versions
            .get(&number)
            .ok_or_else(|| HecateError::not_found("version not found"))?;
        let expired = version.state == VersionState::Deprecated
            && version
                .deprecated_at
                .is_some_and(|since| now >= since + grace);
        Ok(if expired {
            VersionState::Revoked
        } else {
            version.state
        })
    }

    pub fn active_version(&self) -> Option<u64> {
        self.versions
            .values()
            .find(|version| version.state == VersionState::Active)
            .map(|version| version.number)
    }

    /// Whether the secret must be rotated (FR-013): its `rotate_every` is a duration and its
    /// active version is at least that old. Without an active version, or with a `rotate_every`
    /// saved before it was a duration, it is not.
    pub fn due_for_rotation(&self, now: OffsetDateTime) -> bool {
        let Some(interval) = self
            .rotate_every
            .as_deref()
            .and_then(|text| RotationInterval::parse(text).ok())
        else {
            return false;
        };
        self.active_version()
            .and_then(|number| self.versions.get(&number))
            .is_some_and(|version| interval.is_due(version.created_at, now))
    }

    pub fn live_versions(&self) -> Vec<u64> {
        self.versions
            .values()
            .filter(|version| version.state != VersionState::Destroyed)
            .map(|version| version.number)
            .collect()
    }

    pub fn create(
        path: &SecretPath,
        value_type: ValueType,
        at: OffsetDateTime,
        fingerprint: &str,
        key_version: u64,
        options: SecretOptions,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        let max_versions = u64::try_from(options.max_versions)
            .ok()
            .filter(|limit| (MIN_VERSIONS..=MAX_VERSIONS_LIMIT).contains(limit))
            .ok_or_else(|| {
                HecateError::invalid(format!(
                    "max_versions must be a number from {MIN_VERSIONS} to {MAX_VERSIONS_LIMIT}"
                ))
            })?;
        Ok(vec![
            SecretEvent::SecretCreated {
                at,
                path: path.to_string(),
                owner: path.owner().to_string(),
                value_type: value_type.as_str().to_string(),
                max_versions,
                rotate_every: options.rotate_every,
                expires_at: options.expires_at,
            },
            SecretEvent::VersionAdded {
                at,
                version: 1,
                fingerprint: fingerprint.to_string(),
                key_version,
            },
        ])
    }

    pub fn add_version(
        &self,
        at: OffsetDateTime,
        fingerprint: &str,
        key_version: u64,
    ) -> Vec<SecretEvent> {
        let number = self.versions.keys().max().map_or(1, |last| last + 1);
        let mut events = Vec::new();
        if let Some(active) = self.active_version() {
            events.push(SecretEvent::VersionDeprecated {
                at,
                version: active,
            });
        }
        events.push(SecretEvent::VersionAdded {
            at,
            version: number,
            fingerprint: fingerprint.to_string(),
            key_version,
        });
        let mut live = self.live_versions();
        live.push(number);
        if live.len() as u64 > self.max_versions {
            events.push(SecretEvent::VersionDestroyed {
                at,
                version: live[0],
                reason: DestroyReason::MaxVersions.as_str().to_string(),
            });
        }
        events
    }

    fn require(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
        allowed: &[VersionState],
    ) -> Result<VersionState, HecateError> {
        let state = self.state_of(number, now, grace)?;
        if !allowed.contains(&state) {
            return Err(HecateError::state_conflict(
                None,
                Some(format!(
                    "version {number} is {}: the change is not allowed",
                    state.as_str()
                )),
                None,
            ));
        }
        Ok(state)
    }

    pub fn revoke(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        self.require(
            number,
            now,
            grace,
            &[VersionState::Active, VersionState::Deprecated],
        )?;
        Ok(vec![SecretEvent::VersionRevoked {
            at: now,
            version: number,
        }])
    }

    pub fn restore(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        self.require(number, now, grace, &[VersionState::Revoked])?;
        Ok(vec![SecretEvent::VersionRestored {
            at: now,
            version: number,
        }])
    }

    pub fn promote(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        self.require(number, now, grace, &[VersionState::Deprecated])?;
        let mut events = Vec::new();
        if let Some(active) = self.active_version() {
            events.push(SecretEvent::VersionDeprecated {
                at: now,
                version: active,
            });
        }
        events.push(SecretEvent::VersionPromoted {
            at: now,
            version: number,
        });
        Ok(events)
    }

    pub fn destroy(
        &self,
        number: u64,
        now: OffsetDateTime,
        grace: Duration,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        self.require(number, now, grace, &[VersionState::Revoked])?;
        Ok(vec![SecretEvent::VersionDestroyed {
            at: now,
            version: number,
            reason: DestroyReason::Admin.as_str().to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use time::macros::datetime;

    use super::*;

    const T0: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);
    const GRACE: Duration = Duration::hours(24);

    fn path() -> SecretPath {
        SecretPath::parse("secret://hermes/token").unwrap()
    }

    fn minutes(n: i64) -> OffsetDateTime {
        T0 + Duration::minutes(n)
    }

    fn kinds(events: &[SecretEvent]) -> Vec<&'static str> {
        events.iter().map(SecretEvent::kind).collect()
    }

    /// Builds a secret by applying the events its decision methods return, as the service does.
    struct Harness {
        history: Vec<SecretEvent>,
        secret: Secret,
    }

    impl Harness {
        fn new() -> Self {
            Self::with(DEFAULT_MAX_VERSIONS as i64, None, None)
        }

        fn with(
            max_versions: i64,
            rotate_every: Option<&str>,
            expires_at: Option<OffsetDateTime>,
        ) -> Self {
            let options = SecretOptions {
                max_versions,
                rotate_every: rotate_every.map(str::to_string),
                expires_at,
            };
            let events = Secret::create(
                &path(),
                ValueType::String,
                T0,
                "aaaa-aaaa-aaaa-0001",
                1,
                options,
            )
            .unwrap();
            let secret = Secret::from_events(&events).unwrap();
            Self {
                history: events,
                secret,
            }
        }

        fn apply(&mut self, events: Vec<SecretEvent>) -> Vec<SecretEvent> {
            for event in &events {
                self.secret.apply(event).unwrap();
            }
            self.history.extend(events.iter().cloned());
            events
        }

        fn add(&mut self, at: OffsetDateTime) -> Vec<SecretEvent> {
            let next = self.secret.versions.keys().max().unwrap() + 1;
            let events = self
                .secret
                .add_version(at, &format!("aaaa-aaaa-aaaa-{next:04}"), 1);
            self.apply(events)
        }

        fn act(&mut self, action: &str, number: u64, now: OffsetDateTime) -> Vec<SecretEvent> {
            let events = decide(&self.secret, action, number, now).unwrap();
            self.apply(events)
        }

        fn state(&self, number: u64, now: OffsetDateTime) -> VersionState {
            self.secret.state_of(number, now, GRACE).unwrap()
        }
    }

    fn decide(
        secret: &Secret,
        action: &str,
        number: u64,
        now: OffsetDateTime,
    ) -> Result<Vec<SecretEvent>, HecateError> {
        match action {
            "revoke" => secret.revoke(number, now, GRACE),
            "restore" => secret.restore(number, now, GRACE),
            "promote" => secret.promote(number, now, GRACE),
            "destroy" => secret.destroy(number, now, GRACE),
            _ => unreachable!(),
        }
    }

    // py: unit/domain/test_secret_aggregate.py::test_create_gives_version_one_active_and_the_metadata
    #[test]
    fn create_gives_version_one_active_and_the_metadata() {
        let expires = T0 + Duration::days(30);
        let h = Harness::with(5, Some("P90D"), Some(expires));
        let s = &h.secret;
        assert_eq!(
            (&s.path, s.owner.as_str(), s.value_type, s.max_versions),
            (&path(), "hermes", ValueType::String, 5)
        );
        assert_eq!(
            (s.rotate_every.as_deref(), s.expires_at),
            (Some("P90D"), Some(expires))
        );
        assert_eq!((s.stream_version, s.active_version()), (2, Some(1)));
        assert_eq!(h.state(1, T0), VersionState::Active);
        assert_eq!(
            (
                s.versions[&1].fingerprint.as_str(),
                s.versions[&1].key_version
            ),
            ("aaaa-aaaa-aaaa-0001", 1)
        );
        assert_eq!(s.versions[&1].created_at, T0);
    }

    // py: unit/domain/test_secret_aggregate.py::test_defaults_to_ten_versions_without_rotation_or_expiry
    #[test]
    fn defaults_to_ten_versions_without_rotation_or_expiry() {
        let s = Harness::new().secret;
        assert_eq!(
            (s.max_versions, s.rotate_every, s.expires_at),
            (10, None, None)
        );
    }

    // py: unit/domain/test_secret_aggregate.py::test_rebuilds_the_same_state_from_its_events
    #[test]
    fn rebuilds_the_same_state_from_its_events() {
        let mut h = Harness::new();
        h.add(minutes(1));
        h.act("revoke", 1, minutes(2));
        h.add(minutes(3));
        let rebuilt = Secret::from_events(&h.history).unwrap();
        assert_eq!(rebuilt.stream_version, h.secret.stream_version);
        assert_eq!(rebuilt.stream_version, h.history.len() as u64);
        assert_eq!(rebuilt.versions, h.secret.versions);
        assert_eq!(rebuilt.active_version(), Some(3));
    }

    // py: unit/domain/test_secret_aggregate.py::test_from_events_needs_a_creation_event_first
    #[test]
    fn from_events_needs_a_creation_event_first() {
        let h = Harness::new();
        assert!(Secret::from_events(&[]).is_err());
        assert!(Secret::from_events(&h.history[1..]).is_err());
    }

    // py: unit/domain/test_secret_aggregate.py::test_adding_a_version_deprecates_the_previous_active_one
    #[test]
    fn adding_a_version_deprecates_the_previous_active_one() {
        let mut h = Harness::new();
        let events = h.add(minutes(10));
        assert_eq!(kinds(&events), ["VersionDeprecated", "VersionAdded"]);
        assert_eq!(h.secret.active_version(), Some(2));
        assert_eq!(
            (h.state(1, T0), h.state(2, T0)),
            (VersionState::Deprecated, VersionState::Active)
        );
        assert_eq!(h.secret.versions[&1].deprecated_at, Some(minutes(10)));
    }

    // py: unit/domain/test_secret_aggregate.py::test_adding_a_version_with_nothing_active_deprecates_nothing
    #[test]
    fn adding_a_version_with_nothing_active_deprecates_nothing() {
        let mut h = Harness::new();
        h.act("revoke", 1, minutes(1));
        let events = h.add(minutes(2));
        assert_eq!(kinds(&events), ["VersionAdded"]);
        assert_eq!(
            (h.state(1, T0), h.state(2, T0)),
            (VersionState::Revoked, VersionState::Active)
        );
    }

    // py: unit/domain/test_secret_aggregate.py::test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change
    #[test]
    fn exceeding_the_maximum_destroys_the_oldest_in_the_same_change() {
        let mut h = Harness::with(2, None, None);
        h.add(minutes(1));
        let events = h.add(minutes(2));
        assert_eq!(
            events.last(),
            Some(&SecretEvent::VersionDestroyed {
                at: minutes(2),
                version: 1,
                reason: "max_versions".into()
            })
        );
        assert_eq!(h.state(1, T0), VersionState::Destroyed);
        assert_eq!(
            [h.state(2, T0), h.state(3, T0)],
            [VersionState::Deprecated, VersionState::Active]
        );
        let events = h.add(minutes(3));
        assert!(matches!(
            events.last(),
            Some(SecretEvent::VersionDestroyed { version: 2, .. })
        ));
        let live = h
            .secret
            .versions
            .values()
            .filter(|v| v.state != VersionState::Destroyed)
            .count();
        assert_eq!(live, 2);
    }

    // py: unit/domain/test_secret_aggregate.py::test_a_maximum_outside_2_to_100_is_rejected
    #[test]
    fn a_maximum_outside_2_to_100_is_rejected() {
        for max_versions in [1, 0, -1, 101] {
            let options = SecretOptions {
                max_versions,
                ..SecretOptions::default()
            };
            let created = Secret::create(
                &path(),
                ValueType::String,
                T0,
                "aaaa-aaaa-aaaa-0001",
                1,
                options,
            );
            assert_eq!(
                created.unwrap_err(),
                HecateError::invalid("max_versions must be a number from 2 to 100"),
                "{max_versions}"
            );
        }
    }

    // py: unit/domain/test_secret_aggregate.py::test_with_the_minimum_of_two_the_previous_version_survives_a_rotation_in_its_grace
    #[test]
    fn with_the_minimum_of_two_the_previous_version_survives_a_rotation_in_its_grace() {
        let mut h = Harness::with(2, None, None);
        let events = h.add(minutes(1));
        assert!(!kinds(&events).contains(&"VersionDestroyed"));
        assert_eq!(
            [h.state(1, minutes(2)), h.state(2, minutes(2))],
            [VersionState::Deprecated, VersionState::Active]
        );
    }

    // py: unit/domain/test_secret_aggregate.py::test_a_maximum_of_two_destroys_the_oldest_with_a_third_version
    #[test]
    fn a_maximum_of_two_destroys_the_oldest_with_a_third_version() {
        let mut h = Harness::with(2, None, None);
        h.add(minutes(1));
        h.add(minutes(2));
        let states: Vec<VersionState> = (1..=3).map(|n| h.state(n, minutes(3))).collect();
        assert_eq!(
            states,
            [
                VersionState::Destroyed,
                VersionState::Deprecated,
                VersionState::Active
            ]
        );
    }

    /// Version 1 ends up in `state`; version 2 stays active when needed.
    fn version_in_state(state: &str) -> Harness {
        let mut h = Harness::new();
        if state == "active" {
            return h;
        }
        h.add(minutes(1));
        if state == "deprecated" {
            return h;
        }
        h.act("revoke", 1, minutes(2));
        if state == "revoked" {
            return h;
        }
        h.act("destroy", 1, minutes(3));
        h
    }

    // py: unit/domain/test_secret_aggregate.py::test_only_the_documented_transitions_are_allowed
    #[test]
    fn only_the_documented_transitions_are_allowed() {
        let allowed = [
            (("active", "revoke"), "revoked"),
            (("deprecated", "revoke"), "revoked"),
            (("deprecated", "promote"), "active"),
            (("revoked", "restore"), "deprecated"),
            (("revoked", "destroy"), "destroyed"),
        ];
        for state in ["active", "deprecated", "revoked", "destroyed"] {
            for action in ["revoke", "restore", "promote", "destroy"] {
                let mut h = version_in_state(state);
                let now = minutes(5);
                assert_eq!(h.state(1, now).as_str(), state);
                match allowed.iter().find(|(key, _)| *key == (state, action)) {
                    Some((_, after)) => {
                        h.act(action, 1, now);
                        assert_eq!(h.state(1, now).as_str(), *after, "{state} {action}");
                    }
                    None => {
                        let before = h.secret.stream_version;
                        let refused = decide(&h.secret, action, 1, now);
                        assert!(
                            matches!(refused, Err(HecateError::StateConflict { .. })),
                            "{state} {action}"
                        );
                        assert_eq!(h.secret.stream_version, before);
                    }
                }
            }
        }
    }

    // py: unit/domain/test_secret_aggregate.py::test_unknown_versions_are_not_found
    #[test]
    fn unknown_versions_are_not_found() {
        let h = Harness::new();
        for action in ["revoke", "restore", "promote", "destroy"] {
            assert_eq!(
                decide(&h.secret, action, 9, T0).unwrap_err(),
                HecateError::not_found("version not found")
            );
        }
        assert!(matches!(
            h.secret.state_of(9, T0, GRACE),
            Err(HecateError::NotFound(_))
        ));
    }

    // py: unit/domain/test_secret_aggregate.py::test_promote_swaps_which_version_is_active
    #[test]
    fn promote_swaps_which_version_is_active() {
        let mut h = Harness::new();
        h.add(minutes(1));
        let events = h.act("promote", 1, minutes(2));
        assert_eq!(kinds(&events), ["VersionDeprecated", "VersionPromoted"]);
        assert_eq!(h.secret.active_version(), Some(1));
        assert_eq!(h.state(2, minutes(2)), VersionState::Deprecated);
        let active = h
            .secret
            .versions
            .keys()
            .filter(|n| h.state(**n, minutes(2)) == VersionState::Active)
            .count();
        assert_eq!(active, 1);
    }

    // py: unit/domain/test_secret_aggregate.py::test_promote_when_nothing_is_active_only_promotes
    #[test]
    fn promote_when_nothing_is_active_only_promotes() {
        let mut h = Harness::new();
        h.add(minutes(1));
        h.act("revoke", 2, minutes(2));
        let events = h.act("promote", 1, minutes(3));
        assert_eq!(kinds(&events), ["VersionPromoted"]);
        assert_eq!(h.secret.active_version(), Some(1));
    }

    // py: unit/domain/test_secret_aggregate.py::test_revoking_the_active_version_leaves_no_active_one
    #[test]
    fn revoking_the_active_version_leaves_no_active_one() {
        let mut h = Harness::new();
        h.act("revoke", 1, minutes(1));
        assert_eq!(h.secret.active_version(), None);
    }

    // py: unit/domain/test_secret_aggregate.py::test_restore_makes_it_deprecated_with_a_new_grace
    #[test]
    fn restore_makes_it_deprecated_with_a_new_grace() {
        let mut h = Harness::new();
        h.add(minutes(1));
        h.act("revoke", 1, minutes(2));
        let events = h.act("restore", 1, minutes(30));
        assert_eq!(kinds(&events), ["VersionRestored"]);
        assert_eq!(h.secret.versions[&1].deprecated_at, Some(minutes(30)));
        assert_eq!(
            h.state(1, minutes(30) + GRACE - Duration::seconds(1)),
            VersionState::Deprecated
        );
        assert_eq!(h.state(1, minutes(30) + GRACE), VersionState::Revoked);
    }

    // py: unit/domain/test_secret_aggregate.py::test_a_version_past_its_grace_can_be_restored_but_not_revoked_or_promoted
    #[test]
    fn a_version_past_its_grace_can_be_restored_but_not_revoked_or_promoted() {
        let mut h = Harness::new();
        h.add(minutes(1));
        let late = minutes(1) + GRACE + Duration::hours(1);
        assert_eq!(h.state(1, late), VersionState::Revoked);
        for action in ["revoke", "promote"] {
            assert!(matches!(
                decide(&h.secret, action, 1, late),
                Err(HecateError::StateConflict { .. })
            ));
        }
        h.act("restore", 1, late);
        assert_eq!(h.state(1, late), VersionState::Deprecated);
    }

    // py: unit/domain/test_secret_aggregate.py::test_a_version_past_its_grace_can_be_destroyed
    #[test]
    fn a_version_past_its_grace_can_be_destroyed() {
        let mut h = Harness::new();
        h.add(minutes(1));
        let late = minutes(1) + GRACE;
        h.act("destroy", 1, late);
        assert_eq!(h.state(1, late), VersionState::Destroyed);
    }

    // py: unit/domain/test_secret_aggregate.py::test_only_one_version_is_ever_active
    #[test]
    fn only_one_version_is_ever_active() {
        let mut h = Harness::new();
        for i in 1..6 {
            h.add(minutes(i));
            if i % 2 == 1 {
                h.act("promote", 1, minutes(i));
            }
            let active = h
                .secret
                .versions
                .keys()
                .filter(|n| h.state(**n, minutes(i)) == VersionState::Active)
                .count();
            assert!(active <= 1);
        }
    }

    proptest! {
        // py: unit/domain/test_secret_aggregate.py::test_effective_state_flips_exactly_at_the_end_of_the_grace
        #[test]
        fn effective_state_flips_exactly_at_the_end_of_the_grace(offset in -7200i64..=7200) {
            let mut h = Harness::new();
            h.add(T0);
            let now = T0 + GRACE + Duration::seconds(offset);
            let expected = if offset >= 0 { VersionState::Revoked } else { VersionState::Deprecated };
            prop_assert_eq!(h.state(1, now), expected);
            prop_assert_eq!(h.state(2, now), VersionState::Active);
        }

        // py: unit/domain/test_secret_aggregate.py::test_the_grace_is_the_one_received
        #[test]
        fn the_grace_is_the_one_received(grace_hours in 1i64..=72, elapsed_hours in 1i64..=72) {
            let mut h = Harness::new();
            h.add(T0);
            let state = h.secret.state_of(1, T0 + Duration::hours(elapsed_hours), Duration::hours(grace_hours)).unwrap();
            prop_assert_eq!(state == VersionState::Revoked, elapsed_hours >= grace_hours);
        }
    }

    // py: unit/domain/test_secret_aggregate.py::test_stream_version_grows_with_every_event
    #[test]
    fn stream_version_grows_with_every_event() {
        let mut h = Harness::new();
        assert_eq!(h.secret.stream_version, 2);
        h.add(minutes(1));
        assert_eq!(h.secret.stream_version, 4);
        h.act("revoke", 1, minutes(2));
        assert_eq!(h.secret.stream_version, 5);
    }

    // py: unit/domain/test_secret_aggregate.py::test_events_never_carry_a_value_or_a_key
    #[test]
    fn events_never_carry_a_value_or_a_key() {
        let mut h = Harness::new();
        h.add(minutes(1));
        for event in &h.history {
            let record = event.encode();
            for key in ["value", "key", "secret", "material"] {
                assert!(!record.data.contains_key(key), "{}", record.kind);
            }
        }
    }
}
