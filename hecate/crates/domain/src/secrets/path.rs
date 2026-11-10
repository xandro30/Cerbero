//! Secret paths `secret://<identity>/<segment>[/<segment>...]`, whose owner is the identity, and
//! references to them with an optional `@<version>`.

use std::fmt;

use crate::sealing::errors::HecateError;

pub use crate::identity::names::IDENTITY_RULE;
use crate::identity::names::is_identity;

pub const SCHEME: &str = "secret://";

/// `[a-z0-9_-]{1,64}`
fn is_segment(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

/// `[1-9][0-9]{0,8}`
fn version_number(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let well_formed =
        (1..=9).contains(&bytes.len()) && bytes[0] != b'0' && bytes.iter().all(u8::is_ascii_digit);
    if well_formed { text.parse().ok() } else { None }
}

pub fn parse_identity(text: &str) -> Result<&str, HecateError> {
    if is_identity(text) {
        Ok(text)
    } else {
        Err(HecateError::invalid(IDENTITY_RULE))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretPath {
    owner: String,
    segments: Vec<String>,
}

impl SecretPath {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let Some(rest) = text.strip_prefix(SCHEME) else {
            return Err(HecateError::invalid(format!(
                "a secret path starts with {SCHEME}"
            )));
        };
        let parts: Vec<&str> = rest.split('/').collect();
        let [owner, segments @ ..] = parts.as_slice() else {
            unreachable!("split always yields one part");
        };
        if segments.is_empty() {
            return Err(HecateError::invalid(
                "a secret path is secret://<identity>/<path>",
            ));
        }
        if !is_identity(owner) {
            return Err(HecateError::invalid(IDENTITY_RULE));
        }
        if !segments.iter().all(|segment| is_segment(segment)) {
            return Err(HecateError::invalid(
                "path segments are 1-64 characters of a-z, 0-9, '_' and '-'",
            ));
        }
        Ok(Self {
            owner: (*owner).to_string(),
            segments: segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// `<identity>/<segment>/...`
    pub fn relative(&self) -> String {
        let mut relative = self.owner.clone();
        for segment in &self.segments {
            relative.push('/');
            relative.push_str(segment);
        }
        relative
    }

    pub fn stream_name(&self) -> String {
        format!("secret/{}", self.relative())
    }
}

impl fmt::Display for SecretPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}", self.relative())
    }
}

/// A path with an optional explicit version: `secret://<identity>/<path>[@<n>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretReference {
    pub path: SecretPath,
    pub version: Option<u64>,
}

impl SecretReference {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let Some((base, version)) = text.split_once('@') else {
            return Ok(Self {
                path: SecretPath::parse(text)?,
                version: None,
            });
        };
        let version = version_number(version).ok_or_else(|| {
            HecateError::invalid("the version after '@' must be a number of 1 or more")
        })?;
        Ok(Self {
            path: SecretPath::parse(base)?,
            version: Some(version),
        })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn rejected(result: Result<impl std::fmt::Debug, HecateError>) -> bool {
        matches!(result, Err(HecateError::InvalidInput(_)))
    }

    // py: unit/domain/test_secret_path.py::test_parses_a_valid_path_and_exposes_its_owner
    #[test]
    fn parses_a_valid_path_and_exposes_its_owner() {
        let path = SecretPath::parse("secret://hermes/telegram/family_bot/bot_token").unwrap();
        assert_eq!(path.owner(), "hermes");
        assert_eq!(path.segments(), ["telegram", "family_bot", "bot_token"]);
        assert_eq!(
            path.to_string(),
            "secret://hermes/telegram/family_bot/bot_token"
        );
        assert_eq!(
            path.stream_name(),
            "secret/hermes/telegram/family_bot/bot_token"
        );
    }

    // py: unit/domain/test_secret_path.py::test_rejects_malformed_paths
    #[test]
    fn rejects_malformed_paths() {
        let long_identity = format!("secret://{}/token", "a".repeat(64));
        let long_segment = format!("secret://hermes/{}", "a".repeat(65));
        let cases = [
            "",
            "hermes/token",
            "secret:/hermes/token",
            "secret://",
            "secret://hermes",
            "secret://hermes/",
            "secret://hermes//token",
            "secret://hermes/token/",
            "secret://Hermes/token",
            "secret://1hermes/token",
            "secret://-hermes/token",
            "secret://her_mes/token",
            "secret://hermes/Token",
            "secret://hermes/to ken",
            "secret://hermes/to.ken",
            "secret://hermes/tok@en",
            "secret://hermes/../etc",
            long_identity.as_str(),
            long_segment.as_str(),
            "secret://hermes/token\n",
        ];
        for text in cases {
            assert!(rejected(SecretPath::parse(text)), "{text:?}");
        }
    }

    proptest! {
        // py: unit/domain/test_secret_path.py::test_every_well_formed_path_round_trips
        #[test]
        fn every_well_formed_path_round_trips(
            identity in "[a-z][a-z0-9-]{0,62}",
            segments in prop::collection::vec("[a-z0-9_-]{1,64}", 1..=5),
        ) {
            let text = format!("secret://{identity}/{}", segments.join("/"));
            let path = SecretPath::parse(&text).unwrap();
            prop_assert_eq!(path.to_string(), text);
            prop_assert_eq!(path.owner(), identity.as_str());
        }
    }

    // py: unit/domain/test_secret_path.py::test_reference_without_version
    #[test]
    fn reference_without_version() {
        let reference = SecretReference::parse("secret://hermes/token").unwrap();
        assert_eq!(
            reference.path,
            SecretPath::parse("secret://hermes/token").unwrap()
        );
        assert_eq!(reference.version, None);
    }

    // py: unit/domain/test_secret_path.py::test_reference_with_version
    #[test]
    fn reference_with_version() {
        let reference = SecretReference::parse("secret://hermes/token@12").unwrap();
        assert_eq!(
            (reference.path.owner(), reference.version),
            ("hermes", Some(12))
        );
    }

    // py: unit/domain/test_secret_path.py::test_rejects_malformed_references
    #[test]
    fn rejects_malformed_references() {
        for text in [
            "secret://hermes/token@",
            "secret://hermes/token@0",
            "secret://hermes/token@-1",
            "secret://hermes/token@x",
            "secret://hermes/token@1@2",
            "secret://hermes/token@01x",
            "@1",
        ] {
            assert!(rejected(SecretReference::parse(text)), "{text:?}");
        }
    }

    #[test]
    fn identities_follow_the_same_rule_as_in_paths() {
        assert_eq!(parse_identity("hermes-2").unwrap(), "hermes-2");
        for text in ["", "Hermes", "2hermes", "her_mes", &"a".repeat(64)] {
            assert_eq!(
                parse_identity(text).unwrap_err(),
                HecateError::invalid(
                    "the identity must be lowercase letters, digits and '-', starting with a letter"
                )
            );
        }
    }
}
