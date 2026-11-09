//! The `Key` aggregate: the scope key of an identity (Event Sourcing). Its rotation is out of
//! scope, so it has one version.

use std::collections::BTreeMap;

use serde_json::Value;
use time::OffsetDateTime;

use crate::events::{EventRecord, Fields, InvalidEvents, event_data};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    ScopeKeyCreated {
        at: OffsetDateTime,
        identity: String,
        key_version: u64,
    },
}

impl KeyEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ScopeKeyCreated { .. } => "ScopeKeyCreated",
        }
    }

    pub fn encode(&self) -> EventRecord {
        let Self::ScopeKeyCreated {
            at,
            identity,
            key_version,
        } = self;
        EventRecord {
            kind: self.kind().to_string(),
            data: event_data(vec![
                ("identity", Value::from(identity.as_str())),
                ("key_version", Value::from(*key_version)),
            ]),
            at: *at,
        }
    }

    pub fn decode(record: &EventRecord) -> Result<Self, InvalidEvents> {
        if record.kind != "ScopeKeyCreated" {
            return Err(InvalidEvents("unknown key event"));
        }
        let fields = Fields::exactly(record, &["identity", "key_version"])?;
        Ok(Self::ScopeKeyCreated {
            at: record.at,
            identity: fields.text("identity")?,
            key_version: fields.number("key_version")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyVersion {
    pub number: u64,
    pub state: KeyState,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub identity: String,
    pub versions: BTreeMap<u64, KeyVersion>,
    pub stream_version: u64,
}

impl Key {
    pub fn from_events(events: &[KeyEvent]) -> Result<Self, InvalidEvents> {
        if events.is_empty() {
            return Err(InvalidEvents("a key stream starts with ScopeKeyCreated"));
        }
        let mut key = Self {
            identity: String::new(),
            versions: BTreeMap::new(),
            stream_version: 0,
        };
        for event in events {
            key.apply(event);
        }
        Ok(key)
    }

    pub fn apply(&mut self, event: &KeyEvent) {
        let KeyEvent::ScopeKeyCreated {
            at,
            identity,
            key_version,
        } = event;
        self.identity.clone_from(identity);
        self.versions.insert(
            *key_version,
            KeyVersion {
                number: *key_version,
                state: KeyState::Active,
                created_at: *at,
            },
        );
        self.stream_version += 1;
    }

    pub fn current_version(&self) -> u64 {
        self.versions.keys().max().copied().unwrap_or(0)
    }

    pub fn create(identity: &str, at: OffsetDateTime) -> Vec<KeyEvent> {
        vec![KeyEvent::ScopeKeyCreated {
            at,
            identity: identity.to_string(),
            key_version: 1,
        }]
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn a_key_starts_with_one_version_and_rebuilds_from_its_event() {
        let at = datetime!(2026-11-07 12:00 UTC);
        let events = Key::create("hermes", at);
        let key = Key::from_events(&events).unwrap();
        assert_eq!(
            (
                key.identity.as_str(),
                key.current_version(),
                key.stream_version
            ),
            ("hermes", 1, 1)
        );
        assert_eq!(key.versions[&1].created_at, at);
        assert!(Key::from_events(&[]).is_err());
    }
}
