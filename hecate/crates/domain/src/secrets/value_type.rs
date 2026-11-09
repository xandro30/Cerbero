//! The type of a secret's value (research R8): `string`, `json` (standard JSON, FR-018) or `pem`.

use serde_json::Value;

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

/// The ASCII whitespace of RFC 7468: space, tab, CR, LF, VT and FF (FR-018).
fn is_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c')
}

fn is_body(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=') || is_space(character)
}

fn not_pem() -> HecateError {
    HecateError::invalid("the value is not a valid PEM")
}

/// One block of Python's `-----BEGIN (L)-----\s*\n(body)\s*\n-----END \1-----` at the start of
/// `text`: the length it takes and the body without spaces, or `None` if it does not match.
fn pem_block(text: &str) -> Option<(usize, String)> {
    let rest = text.strip_prefix("-----BEGIN ")?;
    let label_len = rest
        .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == ' '))
        .unwrap_or(rest.len());
    if label_len == 0 {
        return None;
    }
    let label = &rest[..label_len];
    let rest = rest[label_len..].strip_prefix("-----")?;
    // The body and the line breaks around it are one run of body characters, since `-` is not one.
    let run_len = rest.find(|c: char| !is_body(c)).unwrap_or(rest.len());
    let run = &rest[..run_len];
    // A line break must open the run after only spaces, the run must end in a line break, and at
    // least one character must sit between both.
    let leading = run.find(|c: char| !is_space(c)).unwrap_or(run.len());
    let opening = run[..leading].find('\n')?;
    if !run.ends_with('\n') || opening + 3 > run_len {
        return None;
    }
    let closing = format!("-----END {label}-----");
    if !rest[run_len..].starts_with(&closing) {
        return None;
    }
    let taken = text.len() - rest.len() + run_len + closing.len();
    Some((taken, run.chars().filter(|c| !is_space(*c)).collect()))
}

/// Strict base64 as `b64decode(validate=True)`: groups of four, padding only at the very end.
fn is_base64(body: &str) -> bool {
    let bytes = body.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return false;
    }
    let alphabet = |b: &u8| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/';
    let (head, last) = bytes.split_at(bytes.len() - 4);
    head.iter().all(alphabet)
        && alphabet(&last[0])
        && alphabet(&last[1])
        && match (last[2], last[3]) {
            (b'=', b'=') => true,
            (third, b'=') => alphabet(&third),
            (third, fourth) => alphabet(&third) && alphabet(&fourth),
        }
}

fn check_pem(value: &str) -> Result<(), HecateError> {
    let text = value.trim_matches(is_space);
    let mut position = 0;
    let mut blocks = 0;
    while position < text.len() {
        let (taken, body) = pem_block(&text[position..]).ok_or_else(not_pem)?;
        if body.is_empty() {
            return Err(HecateError::invalid("a PEM block is empty"));
        }
        if !is_base64(&body) {
            return Err(HecateError::invalid(
                "a PEM block does not contain valid base64",
            ));
        }
        blocks += 1;
        position += taken;
        position += text[position..]
            .find(|c: char| !is_space(c))
            .unwrap_or(text.len() - position);
    }
    if blocks == 0 {
        return Err(not_pem());
    }
    Ok(())
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
