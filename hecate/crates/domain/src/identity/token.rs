//! The enrollment token (research R6): `HET1.<identity>.<id>.<secret>.<fingerprint>`. Hécate
//! keeps only the SHA-256 of the secret; the token is shown once and its secret is erased from
//! memory when dropped.

use std::fmt;

use aws_lc_rs::{constant_time, digest};
use data_encoding::BASE64URL_NOPAD;
use zeroize::Zeroizing;

use crate::identity::names::{IdentityName, RootFingerprint, TokenId};
use crate::sealing::errors::HecateError;
use crate::sensitive::SecretKey;

pub const TOKEN_FORMAT: &str = "the enrollment token is not well formed";

const PREFIX: &str = "HET1";
/// `HET1.` + identity (63) + `.` + id (16) + `.` + secret (43) + `.` + fingerprint (64).
const MAX_TEXT: usize = 4 + 1 + 63 + 1 + 16 + 1 + 43 + 1 + 64;

/// A single-use enrollment token for one identity and one root CA. Its secret erases itself when
/// dropped; the token can't be cloned and `Debug` does not show the secret.
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_domain::identity::token::EnrollmentToken>();
/// ```
pub struct EnrollmentToken {
    identity: IdentityName,
    id: TokenId,
    secret: SecretKey,
    fingerprint: RootFingerprint,
}

impl EnrollmentToken {
    pub fn new(
        identity: IdentityName,
        id: TokenId,
        secret: SecretKey,
        fingerprint: RootFingerprint,
    ) -> Self {
        Self {
            identity,
            id,
            secret,
            fingerprint,
        }
    }

    /// Reads a token; anything that does not follow the format is `validation`.
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let invalid = || HecateError::invalid(TOKEN_FORMAT);
        let parts: Vec<&str> = text.split('.').collect();
        let [prefix, identity, id, secret, fingerprint] = parts.as_slice() else {
            return Err(invalid());
        };
        if *prefix != PREFIX || secret.len() != 43 {
            return Err(invalid());
        }
        let decoded = Zeroizing::new(
            BASE64URL_NOPAD
                .decode(secret.as_bytes())
                .map_err(|_| invalid())?,
        );
        Ok(Self {
            identity: IdentityName::parse(identity).map_err(|_| invalid())?,
            id: TokenId::parse(id).map_err(|_| invalid())?,
            secret: SecretKey::from_slice(&decoded).ok_or_else(invalid)?,
            fingerprint: RootFingerprint::parse_hex(fingerprint).map_err(|_| invalid())?,
        })
    }

    /// The token as the admin receives it, in memory that is erased when dropped.
    pub fn text(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(String::with_capacity(MAX_TEXT));
        text.push_str(PREFIX);
        text.push('.');
        text.push_str(self.identity.as_str());
        text.push('.');
        text.push_str(self.id.as_str());
        text.push('.');
        BASE64URL_NOPAD.encode_append(self.secret.expose(), &mut text);
        text.push('.');
        text.push_str(&self.fingerprint.hex());
        text
    }

    pub fn identity(&self) -> &IdentityName {
        &self.identity
    }

    pub fn id(&self) -> &TokenId {
        &self.id
    }

    pub fn fingerprint(&self) -> &RootFingerprint {
        &self.fingerprint
    }

    /// What Hécate keeps of the token.
    pub fn secret_sha256(&self) -> [u8; 32] {
        let mut hash = [0; 32];
        hash.copy_from_slice(digest::digest(&digest::SHA256, self.secret.expose()).as_ref());
        hash
    }

    /// Compares the hash of the secret with a stored one in constant time.
    pub fn matches_hash(&self, stored: &[u8; 32]) -> bool {
        constant_time::verify_slices_are_equal(&self.secret_sha256(), stored).is_ok()
    }
}

impl fmt::Debug for EnrollmentToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnrollmentToken")
            .field("identity", &self.identity)
            .field("id", &self.id)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::identity::names::{IdentityName, RootFingerprint, TokenId};
    use crate::sealing::errors::HecateError;

    const ID: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
    const SECRET: [u8; 32] = [7; 32];

    fn token() -> EnrollmentToken {
        EnrollmentToken::new(
            IdentityName::parse("hermes").unwrap(),
            TokenId::from_bytes(ID),
            SecretKey::new(SECRET),
            RootFingerprint::of_der(b"root"),
        )
    }

    #[test]
    fn the_token_has_its_format() {
        let token = token();
        let text = token.text();
        let parts: Vec<&str> = text.split('.').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(
            (parts[0], parts[1], parts[2]),
            ("HET1", "hermes", "0123456789abcdef")
        );
        assert_eq!(parts[3].len(), 43);
        assert!(
            parts[3]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(parts[4], RootFingerprint::of_der(b"root").hex());
    }

    #[test]
    fn a_token_is_read_back_with_the_same_parts() {
        let original = token();
        let parsed = EnrollmentToken::parse(&original.text()).unwrap();
        assert_eq!(parsed.identity(), original.identity());
        assert_eq!(parsed.id(), original.id());
        assert_eq!(parsed.fingerprint(), original.fingerprint());
        assert_eq!(parsed.secret_sha256(), original.secret_sha256());
    }

    #[test]
    fn a_malformed_token_is_rejected() {
        let good = token().text();
        let parts: Vec<String> = good.split('.').map(str::to_string).collect();
        let with = |index: usize, value: &str| {
            let mut changed = parts.clone();
            changed[index] = value.to_string();
            changed.join(".")
        };
        for bad in [
            String::new(),
            with(0, "HET2"),
            with(1, "Hermes"),
            with(2, "0123456789ABCDEF"),
            with(2, "0123456789abcde"),
            with(3, &parts[3][1..]),
            with(3, &format!("+{}", &parts[3][1..])),
            with(4, &parts[4].to_uppercase()),
            format!("{}.extra", good.as_str()),
            parts[..4].join("."),
        ] {
            assert_eq!(
                EnrollmentToken::parse(&bad).unwrap_err(),
                HecateError::invalid(TOKEN_FORMAT),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn only_the_hash_of_the_secret_is_kept_and_compared() {
        let token = token();
        let expected = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &SECRET);
        assert_eq!(token.secret_sha256().as_slice(), expected.as_ref());
        assert!(token.matches_hash(&token.secret_sha256()));
        let other = EnrollmentToken::new(
            IdentityName::parse("hermes").unwrap(),
            TokenId::from_bytes(ID),
            SecretKey::new([8; 32]),
            RootFingerprint::of_der(b"root"),
        );
        assert!(!token.matches_hash(&other.secret_sha256()));
    }

    #[test]
    fn debug_does_not_show_the_secret() {
        let token = token();
        let text = token.text();
        let secret = text.split('.').nth(3).unwrap();
        let shown = format!("{token:?}");
        assert!(shown.contains("hermes"));
        assert!(!shown.contains(secret), "{shown}");
    }

    proptest! {
        #[test]
        fn tokens_round_trip(
            name in "[a-z][a-z0-9-]{0,62}",
            id in proptest::array::uniform8(any::<u8>()),
            secret in proptest::array::uniform32(any::<u8>()),
            root in proptest::collection::vec(any::<u8>(), 1..64),
        ) {
            let token = EnrollmentToken::new(
                IdentityName::parse(&name).unwrap(),
                TokenId::from_bytes(id),
                SecretKey::new(secret),
                RootFingerprint::of_der(&root),
            );
            let text = token.text();
            let parsed = EnrollmentToken::parse(&text).unwrap();
            let reread = parsed.text();
            prop_assert_eq!(reread.as_str(), text.as_str());
        }
    }
}
