//! The names of the identities (data-model «Valores»): trust domain, identity name, SPIFFE ID,
//! certificate serial, root CA fingerprint and enrollment token id.

use std::fmt;

use aws_lc_rs::digest;
use data_encoding::HEXLOWER;

use crate::sealing::errors::HecateError;

pub const IDENTITY_RULE: &str =
    "the identity must be lowercase letters, digits and '-', starting with a letter";
pub const TRUST_DOMAIN_RULE: &str =
    "the trust domain must be 1-255 characters of lowercase letters, digits, '.', '-' and '_'";
pub const SPIFFE_ID_RULE: &str = "a SPIFFE ID is spiffe://<trust domain>/<identity>";
pub const SERIAL_RULE: &str =
    "a serial is a positive number of up to 128 bits in lowercase hexadecimal";
pub const FINGERPRINT_RULE: &str =
    "a root fingerprint is sha256: and 64 lowercase hexadecimal characters";
pub const TOKEN_ID_RULE: &str = "a token id is 16 lowercase hexadecimal characters";

/// The identities that never get an enrollment token nor secrets: Hécate and the admin.
pub const RESERVED_IDENTITIES: [&str; 2] = ["hecate", "admin"];

const SPIFFE_SCHEME: &str = "spiffe://";
const FINGERPRINT_PREFIX: &str = "sha256:";

/// `[a-z][a-z0-9-]{0,62}`: the name of an identity, also the first segment of secret paths.
pub fn is_identity(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=63).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The trust domain of a Hécate: 1 to 255 characters of the SPIFFE alphabet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrustDomain(String);

impl TrustDomain {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let valid = (1..=255).contains(&text.len())
            && text.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
            });
        if valid {
            Ok(Self(text.to_string()))
        } else {
            Err(HecateError::invalid(TRUST_DOMAIN_RULE))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TrustDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of an identity; `hecate` and `admin` are well formed but reserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdentityName(String);

impl IdentityName {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        if is_identity(text) {
            Ok(Self(text.to_string()))
        } else {
            Err(HecateError::invalid(IDENTITY_RULE))
        }
    }

    /// The identity of Hécate's own certificate.
    pub fn hecate() -> Self {
        Self(RESERVED_IDENTITIES[0].to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_reserved(&self) -> bool {
        RESERVED_IDENTITIES.contains(&self.0.as_str())
    }
}

impl fmt::Display for IdentityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `spiffe://<trust domain>/<identity>`, the only name of a certificate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpiffeId {
    trust_domain: TrustDomain,
    identity: IdentityName,
}

impl SpiffeId {
    pub fn new(trust_domain: TrustDomain, identity: IdentityName) -> Self {
        Self {
            trust_domain,
            identity,
        }
    }

    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let invalid = || HecateError::invalid(SPIFFE_ID_RULE);
        let rest = text.strip_prefix(SPIFFE_SCHEME).ok_or_else(invalid)?;
        let (trust_domain, identity) = rest.split_once('/').ok_or_else(invalid)?;
        Ok(Self::new(
            TrustDomain::parse(trust_domain).map_err(|_| invalid())?,
            IdentityName::parse(identity).map_err(|_| invalid())?,
        ))
    }

    pub fn trust_domain(&self) -> &TrustDomain {
        &self.trust_domain
    }

    pub fn identity(&self) -> &IdentityName {
        &self.identity
    }
}

impl fmt::Display for SpiffeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SPIFFE_SCHEME}{}/{}", self.trust_domain, self.identity)
    }
}

/// The serial of a certificate: a positive integer of up to 128 bits, in lowercase hexadecimal
/// without leading zeros.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Serial(String);

impl Serial {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let valid = (1..=32).contains(&text.len()) && is_lower_hex(text) && !text.starts_with('0');
        if valid {
            Ok(Self(text.to_string()))
        } else {
            Err(HecateError::invalid(SERIAL_RULE))
        }
    }

    /// From the big-endian bytes of the integer, with or without leading zero bytes.
    pub fn from_be_bytes(bytes: &[u8]) -> Result<Self, HecateError> {
        let first = bytes
            .iter()
            .position(|byte| *byte != 0)
            .ok_or_else(|| HecateError::invalid(SERIAL_RULE))?;
        let significant = &bytes[first..];
        if significant.len() > 16 {
            return Err(HecateError::invalid(SERIAL_RULE));
        }
        let hex = HEXLOWER.encode(significant);
        Ok(Self(hex.trim_start_matches('0').to_string()))
    }

    /// The big-endian bytes of the integer, without leading zero bytes.
    pub fn to_be_bytes(&self) -> Vec<u8> {
        let even = if self.0.len() % 2 == 1 {
            format!("0{}", self.0)
        } else {
            self.0.clone()
        };
        HEXLOWER
            .decode(even.as_bytes())
            .expect("a serial is lowercase hexadecimal")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Serial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The SHA-256 of the DER of the root CA certificate, shown as `sha256:<hex>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RootFingerprint([u8; 32]);

impl RootFingerprint {
    pub fn of_der(der: &[u8]) -> Self {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(digest::digest(&digest::SHA256, der).as_ref());
        Self(bytes)
    }

    pub fn parse(text: &str) -> Result<Self, HecateError> {
        text.strip_prefix(FINGERPRINT_PREFIX)
            .and_then(Self::from_hex)
            .ok_or_else(|| HecateError::invalid(FINGERPRINT_RULE))
    }

    /// The 64 hexadecimal characters alone, as the enrollment token carries them.
    pub fn parse_hex(hex: &str) -> Result<Self, HecateError> {
        Self::from_hex(hex).ok_or_else(|| HecateError::invalid(FINGERPRINT_RULE))
    }

    fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 || !is_lower_hex(hex) {
            return None;
        }
        let bytes = HEXLOWER.decode(hex.as_bytes()).ok()?;
        Some(Self(bytes.try_into().ok()?))
    }

    pub fn hex(&self) -> String {
        HEXLOWER.encode(&self.0)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for RootFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{FINGERPRINT_PREFIX}{}", self.hex())
    }
}

impl fmt::Debug for RootFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RootFingerprint({self})")
    }
}

/// The public id of an enrollment token: 8 random bytes in lowercase hexadecimal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenId(String);

impl TokenId {
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(HEXLOWER.encode(&bytes))
    }

    pub fn parse(text: &str) -> Result<Self, HecateError> {
        if text.len() == 16 && is_lower_hex(text) {
            Ok(Self(text.to_string()))
        } else {
            Err(HecateError::invalid(TOKEN_ID_RULE))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::sealing::errors::HecateError;
    use crate::secrets::path::parse_identity;

    #[test]
    fn a_trust_domain_uses_the_spiffe_alphabet() {
        for good in ["cerbero.home", "cerbero.test", "a", "x_y-z.1"] {
            assert_eq!(TrustDomain::parse(good).unwrap().as_str(), good);
        }
        let long = "a".repeat(256);
        for bad in [
            "",
            "Cerbero.home",
            "cerbero home",
            "cerbero/home",
            "café",
            long.as_str(),
        ] {
            assert_eq!(
                TrustDomain::parse(bad).unwrap_err(),
                HecateError::invalid(TRUST_DOMAIN_RULE),
                "{bad:?}"
            );
        }
        assert!(TrustDomain::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn an_identity_name_follows_the_rule_of_secret_paths() {
        let longest = format!("a{}", "b".repeat(62));
        for good in ["hermes", "a", "a-1", "hecate", "admin", longest.as_str()] {
            assert_eq!(IdentityName::parse(good).unwrap().as_str(), good);
            assert_eq!(parse_identity(good).unwrap(), good);
        }
        let too_long = format!("a{}", "b".repeat(63));
        for bad in [
            "",
            "Hermes",
            "1hermes",
            "-x",
            "her_mes",
            "her.mes",
            too_long.as_str(),
        ] {
            assert_eq!(
                IdentityName::parse(bad).unwrap_err(),
                HecateError::invalid(IDENTITY_RULE),
                "{bad:?}"
            );
            assert!(parse_identity(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn hecate_and_admin_are_reserved() {
        assert!(IdentityName::parse("hecate").unwrap().is_reserved());
        assert!(IdentityName::parse("admin").unwrap().is_reserved());
        assert!(!IdentityName::parse("hermes").unwrap().is_reserved());
        assert_eq!(IdentityName::hecate().as_str(), "hecate");
    }

    #[test]
    fn a_spiffe_id_is_read_and_written() {
        let id = SpiffeId::parse("spiffe://cerbero.test/hermes").unwrap();
        assert_eq!(
            (id.trust_domain().as_str(), id.identity().as_str()),
            ("cerbero.test", "hermes")
        );
        assert_eq!(id.to_string(), "spiffe://cerbero.test/hermes");
        let built = SpiffeId::new(
            TrustDomain::parse("cerbero.home").unwrap(),
            IdentityName::parse("hecate").unwrap(),
        );
        assert_eq!(built.to_string(), "spiffe://cerbero.home/hecate");
        for bad in [
            "spiffe://cerbero.test/hermes/extra",
            "spiffe://cerbero.test/",
            "spiffe:///hermes",
            "https://cerbero.test/hermes",
            "spiffe://Cerbero.test/hermes",
            "spiffe://cerbero.test/Hermes",
            "spiffe://cerbero.test/hermes?x=1",
            "",
        ] {
            assert!(SpiffeId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_serial_is_a_positive_number_of_up_to_128_bits_in_lowercase_hex() {
        assert_eq!(Serial::from_be_bytes(&[0x00, 0x01]).unwrap().as_str(), "1");
        assert_eq!(
            Serial::from_be_bytes(&[0xff; 16]).unwrap().as_str(),
            "f".repeat(32)
        );
        assert_eq!(
            Serial::from_be_bytes(&[0x00, 0x0a, 0xbc])
                .unwrap()
                .to_be_bytes(),
            vec![0x0a, 0xbc]
        );
        let mut seventeen = [0u8; 17];
        seventeen[0] = 1;
        for bad in [&[][..], &[0u8, 0], &seventeen[..]] {
            assert_eq!(
                Serial::from_be_bytes(bad).unwrap_err(),
                HecateError::invalid(SERIAL_RULE),
                "{bad:?}"
            );
        }
        assert_eq!(
            Serial::parse("5f1c").unwrap().to_be_bytes(),
            vec![0x5f, 0x1c]
        );
        let too_long = "1".repeat(33);
        for bad in ["", "0", "01", "ABC", "5f1g", too_long.as_str()] {
            assert_eq!(
                Serial::parse(bad).unwrap_err(),
                HecateError::invalid(SERIAL_RULE),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_root_fingerprint_is_the_sha256_of_the_der() {
        let fingerprint = RootFingerprint::of_der(b"abc");
        let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(fingerprint.hex(), hex);
        assert_eq!(fingerprint.to_string(), format!("sha256:{hex}"));
        assert_eq!(
            RootFingerprint::parse(&format!("sha256:{hex}")).unwrap(),
            fingerprint
        );
        assert_eq!(RootFingerprint::parse_hex(hex).unwrap(), fingerprint);
        for bad in [
            hex.to_string(),
            format!("sha256:{}", hex.to_uppercase()),
            format!("sha256:{}", &hex[1..]),
            format!("sha1:{hex}"),
        ] {
            assert_eq!(
                RootFingerprint::parse(&bad).unwrap_err(),
                HecateError::invalid(FINGERPRINT_RULE),
                "{bad:?}"
            );
        }
        assert!(RootFingerprint::parse_hex(&hex.to_uppercase()).is_err());
    }

    #[test]
    fn a_token_id_is_16_lowercase_hex_characters() {
        let id = TokenId::from_bytes([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        assert_eq!(id.as_str(), "0123456789abcdef");
        assert_eq!(TokenId::parse("0123456789abcdef").unwrap(), id);
        for bad in [
            "0123456789ABCDEF",
            "0123456789abcde",
            "0123456789abcdef0",
            "",
        ] {
            assert_eq!(
                TokenId::parse(bad).unwrap_err(),
                HecateError::invalid(TOKEN_ID_RULE),
                "{bad:?}"
            );
        }
    }

    proptest! {
        #[test]
        fn identity_names_round_trip(name in "[a-z][a-z0-9-]{0,62}") {
            prop_assert_eq!(IdentityName::parse(&name).unwrap().to_string(), name);
        }

        #[test]
        fn an_uppercase_letter_makes_a_name_invalid(name in "[a-z]{0,10}[A-Z][a-z0-9-]{0,10}") {
            prop_assert!(IdentityName::parse(&name).is_err());
        }

        #[test]
        fn trust_domains_round_trip(td in "[a-z0-9._-]{1,255}") {
            prop_assert_eq!(TrustDomain::parse(&td).unwrap().to_string(), td);
        }

        #[test]
        fn spiffe_ids_round_trip(td in "[a-z0-9._-]{1,64}", name in "[a-z][a-z0-9-]{0,62}") {
            let text = format!("spiffe://{td}/{name}");
            prop_assert_eq!(SpiffeId::parse(&text).unwrap().to_string(), text);
        }

        #[test]
        fn serials_round_trip(bytes in proptest::array::uniform16(any::<u8>())) {
            prop_assume!(bytes.iter().any(|byte| *byte != 0));
            let serial = Serial::from_be_bytes(&bytes).unwrap();
            prop_assert_eq!(Serial::parse(serial.as_str()).unwrap(), serial.clone());
            let first = bytes.iter().position(|byte| *byte != 0).unwrap();
            prop_assert_eq!(serial.to_be_bytes(), bytes[first..].to_vec());
        }
    }
}
