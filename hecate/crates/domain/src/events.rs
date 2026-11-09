//! How domain events are stored, as in the Python version: the type name, the fields of the event
//! as a JSON object without `at` (instants in ISO 8601) and the instant. Events carry metadata
//! only, never a value or a key.

use serde_json::{Map, Value};
use time::OffsetDateTime;

use crate::instant::{isoformat, parse_iso};

#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub kind: String,
    pub data: Map<String, Value>,
    pub at: OffsetDateTime,
}

/// A stored stream that the aggregates cannot read: an unknown type, other fields, a field of
/// another type, or a stream that does not start with its creation event.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid event stream: {0}")]
pub struct InvalidEvents(pub &'static str);

/// The JSON fields of an event, from its name and value pairs.
pub fn event_data(fields: Vec<(&str, Value)>) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

pub fn instant_value(instant: Option<OffsetDateTime>) -> Value {
    instant.map_or(Value::Null, |instant| Value::from(isoformat(instant)))
}

/// The fields of one stored event, which must be exactly the expected ones.
pub struct Fields<'a>(&'a Map<String, Value>);

impl<'a> Fields<'a> {
    pub fn exactly(record: &'a EventRecord, names: &[&str]) -> Result<Self, InvalidEvents> {
        let expected = record.data.len() == names.len()
            && names.iter().all(|name| record.data.contains_key(*name));
        if expected {
            Ok(Self(&record.data))
        } else {
            Err(InvalidEvents("the event does not have the expected fields"))
        }
    }

    pub fn number(&self, name: &str) -> Result<u64, InvalidEvents> {
        self.0[name]
            .as_u64()
            .ok_or(InvalidEvents("an event field is not a number"))
    }

    pub fn text(&self, name: &str) -> Result<String, InvalidEvents> {
        self.0[name]
            .as_str()
            .map(str::to_string)
            .ok_or(InvalidEvents("an event field is not a text"))
    }

    pub fn optional_text(&self, name: &str) -> Result<Option<String>, InvalidEvents> {
        match &self.0[name] {
            Value::Null => Ok(None),
            _ => self.text(name).map(Some),
        }
    }

    pub fn optional_instant(&self, name: &str) -> Result<Option<OffsetDateTime>, InvalidEvents> {
        self.optional_text(name)?
            .map(|text| {
                parse_iso(&text).map_err(|_| InvalidEvents("an event field is not an instant"))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::instant::{isoformat, parse_iso};
    use crate::keys::key::KeyEvent;
    use crate::secrets::secret::SecretEvent;

    #[test]
    fn events_are_read_and_written_with_the_same_json_content() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/events.json"
        );
        let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 10);
        let mut kinds = Vec::new();
        for case in cases {
            let Value::Object(data) = serde_json::from_str(case["data"].as_str().unwrap()).unwrap()
            else {
                panic!("event data is an object");
            };
            let record = EventRecord {
                kind: case["type"].as_str().unwrap().to_string(),
                data,
                at: parse_iso(case["at"].as_str().unwrap()).unwrap(),
            };
            let rewritten = if record.kind == "ScopeKeyCreated" {
                KeyEvent::decode(&record).unwrap().encode()
            } else {
                SecretEvent::decode(&record).unwrap().encode()
            };
            assert_eq!(rewritten, record);
            assert_eq!(isoformat(rewritten.at), case["at"].as_str().unwrap());
            kinds.push(record.kind);
        }
        kinds.sort();
        kinds.dedup();
        assert_eq!(
            kinds.len(),
            8,
            "every event type of Secret and Key: {kinds:?}"
        );
    }

    #[test]
    fn an_event_with_other_fields_or_types_is_corrupt() {
        let at = parse_iso("2026-11-08T12:30:00+00:00").unwrap();
        let record = |kind: &str, data: Value| EventRecord {
            kind: kind.to_string(),
            data: data.as_object().unwrap().clone(),
            at,
        };
        assert!(
            SecretEvent::decode(&record("VersionRevoked", serde_json::json!({"version": 1})))
                .is_ok()
        );
        for bad in [
            record("VersionRevoked", serde_json::json!({})),
            record("VersionRevoked", serde_json::json!({"version": "1"})),
            record(
                "VersionRevoked",
                serde_json::json!({"version": 1, "value": "x"}),
            ),
            record("Unknown", serde_json::json!({"version": 1})),
        ] {
            assert!(SecretEvent::decode(&bad).is_err(), "{bad:?}");
        }
    }
}
