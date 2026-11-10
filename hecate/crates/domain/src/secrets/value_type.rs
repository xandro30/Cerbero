//! The type of a secret's value (research R8): `string`, `json` (standard JSON, FR-018) or `pem`.

use serde_json::Value;

use crate::pem::{self, PemError};
use crate::sealing::errors::HecateError;
use crate::sensitive::SecretBytes;

pub const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    String,
    Json,
    Pem,
}

impl ValueType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Json => "json",
            Self::Pem => "pem",
        }
    }

    pub fn parse(text: &str) -> Result<Self, HecateError> {
        match text {
            "string" => Ok(Self::String),
            "json" => Ok(Self::Json),
            "pem" => Ok(Self::Pem),
            _ => Err(HecateError::invalid(
                "the value type must be 'string', 'json' or 'pem'",
            )),
        }
    }

    /// Checks a value for this type and returns its UTF-8 bytes, in a type that erases them.
    pub fn validate(self, value: &str) -> Result<SecretBytes, HecateError> {
        if value.is_empty() {
            return Err(HecateError::invalid("the value must not be empty"));
        }
        if value.len() > MAX_VALUE_BYTES {
            return Err(HecateError::invalid(format!(
                "the value is larger than {} KiB",
                MAX_VALUE_BYTES / 1024
            )));
        }
        match self {
            Self::String => {}
            Self::Json => {
                serde_json::from_str::<Value>(value)
                    .map_err(|_| HecateError::invalid("the value is not valid JSON"))?;
            }
            Self::Pem => check_pem(value)?,
        }
        Ok(SecretBytes::new(value.as_bytes().to_vec()))
    }
}

fn not_pem() -> HecateError {
    HecateError::invalid("the value is not a valid PEM")
}

fn check_pem(value: &str) -> Result<(), HecateError> {
    pem::check(value).map_err(|error| match error {
        PemError::NotPem => not_pem(),
        PemError::EmptyBlock => HecateError::invalid("a PEM block is empty"),
        PemError::InvalidBase64 => {
            HecateError::invalid("a PEM block does not contain valid base64")
        }
    })
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    const PEM: &str = "-----BEGIN CERTIFICATE-----\n\
        MIIBszCCAVmgAwIBAgIUAAAAAAAAAAAAAAAAAAAAAAAwCgYIKoZIzj0EAwIw\n\
        -----END CERTIFICATE-----\n";

    fn invalid(result: Result<SecretBytes, HecateError>) -> bool {
        matches!(result, Err(HecateError::InvalidInput(_)))
    }

    // py: unit/domain/test_value_type.py::test_parse_known_and_unknown_types
    #[test]
    fn parse_known_and_unknown_types() {
        assert_eq!(ValueType::parse("string").unwrap(), ValueType::String);
        assert_eq!(ValueType::parse("json").unwrap(), ValueType::Json);
        assert_eq!(ValueType::parse("pem").unwrap(), ValueType::Pem);
        for bad in ["binary", "", "STRING"] {
            assert_eq!(
                ValueType::parse(bad).unwrap_err(),
                HecateError::invalid("the value type must be 'string', 'json' or 'pem'")
            );
        }
    }

    // py: unit/domain/test_value_type.py::test_string_accepts_utf8_text_and_returns_its_bytes
    #[test]
    fn string_accepts_utf8_text_and_returns_its_bytes() {
        assert_eq!(
            ValueType::String.validate("pässwörd €").unwrap().expose(),
            "pässwörd €".as_bytes()
        );
    }

    // py: unit/domain/test_value_type.py::test_string_rejects_empty_and_non_text
    #[test]
    fn string_rejects_empty() {
        assert_eq!(
            ValueType::String.validate("").err().unwrap(),
            HecateError::invalid("the value must not be empty")
        );
    }

    // py: unit/domain/test_value_type.py::test_json_accepts_valid_and_rejects_invalid
    #[test]
    fn json_accepts_valid_and_rejects_invalid() {
        assert_eq!(
            ValueType::Json
                .validate("{\"a\": [1, 2]}")
                .unwrap()
                .expose(),
            b"{\"a\": [1, 2]}"
        );
        for bad in ["{not json", "", "{'a': 1}"] {
            assert!(invalid(ValueType::Json.validate(bad)), "{bad:?}");
        }
    }

    // py: unit/domain/test_value_type.py::test_pem_accepts_one_or_more_blocks
    #[test]
    fn pem_accepts_one_or_more_blocks() {
        assert_eq!(
            ValueType::Pem.validate(PEM).unwrap().expose(),
            PEM.as_bytes()
        );
        assert!(ValueType::Pem.validate(&format!("{PEM}{PEM}")).is_ok());
    }

    // py: unit/domain/test_value_type.py::test_pem_rejects_malformed
    #[test]
    fn pem_rejects_malformed() {
        let junk_before = format!("junk\n{PEM}");
        let junk_after = format!("{PEM}junk\n");
        for text in [
            "not a pem",
            "-----BEGIN CERTIFICATE-----\n!!!not base64!!!\n-----END CERTIFICATE-----\n",
            "-----BEGIN CERTIFICATE-----\nMIIB\n-----END PRIVATE KEY-----\n",
            "-----BEGIN CERTIFICATE-----\nMIIB\n",
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n",
            junk_before.as_str(),
            junk_after.as_str(),
        ] {
            assert!(invalid(ValueType::Pem.validate(text)), "{text:?}");
        }
    }

    // py: unit/domain/test_value_type.py::test_size_limit_is_64_kib_for_every_type
    #[test]
    fn size_limit_is_64_kib_for_every_type() {
        assert_eq!(MAX_VALUE_BYTES, 64 * 1024);
        let string = |n: usize| "a".repeat(n);
        let json = |n: usize| format!("\"{}\"", "a".repeat(n - 2));
        for (value_type, build) in [
            (ValueType::String, &string as &dyn Fn(usize) -> String),
            (ValueType::Json, &json),
        ] {
            assert!(value_type.validate(&build(MAX_VALUE_BYTES)).is_ok());
            assert_eq!(
                value_type
                    .validate(&build(MAX_VALUE_BYTES + 1))
                    .err()
                    .unwrap(),
                HecateError::invalid("the value is larger than 64 KiB")
            );
        }
    }

    // py: unit/domain/test_value_type.py::test_size_limit_counts_bytes_not_characters
    #[test]
    fn size_limit_counts_bytes_not_characters() {
        assert!(invalid(
            ValueType::String.validate(&"€".repeat(MAX_VALUE_BYTES / 3 + 1))
        ));
    }

    // py: unit/domain/test_value_type.py::test_pem_over_the_limit_is_rejected
    #[test]
    fn pem_over_the_limit_is_rejected() {
        let body = format!("{}\n", "A".repeat(64));
        let big = format!(
            "-----BEGIN CERTIFICATE-----\n{}-----END CERTIFICATE-----\n",
            body.repeat(1100)
        );
        assert!(big.len() > MAX_VALUE_BYTES);
        assert!(invalid(ValueType::Pem.validate(&big)));
    }

    #[test]
    fn values_are_checked_like_python_except_the_deliberate_changes() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/values.json"
        );
        let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 62);
        for case in cases {
            let value = case["value"].as_str().unwrap();
            let checked = ValueType::parse(case["type"].as_str().unwrap())
                .unwrap()
                .validate(value);
            match case["expected_error"].as_str() {
                None => assert_eq!(checked.unwrap().expose(), value.as_bytes(), "{value:?}"),
                Some(message) => assert_eq!(
                    checked.err().unwrap(),
                    HecateError::invalid(message),
                    "{value:?}"
                ),
            }
        }
    }
}
