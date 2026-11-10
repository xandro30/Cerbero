//! The call-signing key of a component (research R5): an Ed25519 public key whose `kid` is its
//! RFC 7638 thumbprint in base64url.

use aws_lc_rs::digest;
use data_encoding::BASE64URL_NOPAD;

use crate::sealing::errors::HecateError;

pub const CALL_KEY_RULE: &str = "a call-signing key is an Ed25519 public key of 32 bytes";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallKey([u8; 32]);

impl CallKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HecateError> {
        <[u8; 32]>::try_from(bytes)
            .map(Self)
            .map_err(|_| HecateError::invalid(CALL_KEY_RULE))
    }

    /// From the JWK member `x`: the key in base64url without padding.
    pub fn from_x(x: &str) -> Result<Self, HecateError> {
        let bytes = BASE64URL_NOPAD
            .decode(x.as_bytes())
            .map_err(|_| HecateError::invalid(CALL_KEY_RULE))?;
        Self::from_bytes(&bytes)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn x(&self) -> String {
        BASE64URL_NOPAD.encode(&self.0)
    }

    /// The SHA-256 of `{"crv":"Ed25519","kty":"OKP","x":"<x>"}`, the required members in
    /// lexicographic order and without spaces (RFC 7638 and RFC 8037).
    pub fn kid(&self) -> String {
        let members = format!(r#"{{"crv":"Ed25519","kty":"OKP","x":"{}"}}"#, self.x());
        BASE64URL_NOPAD.encode(digest::digest(&digest::SHA256, members.as_bytes()).as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sealing::errors::HecateError;

    /// RFC 8037, appendix A.3: the thumbprint of the example Ed25519 key.
    const RFC_X: &str = "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo";
    const RFC_KID: &str = "kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k";

    #[test]
    fn the_kid_is_the_rfc_7638_thumbprint() {
        let key = CallKey::from_x(RFC_X).unwrap();
        assert_eq!(key.kid(), RFC_KID);
        assert_eq!(key.x(), RFC_X);
    }

    #[test]
    fn a_key_that_is_not_32_bytes_is_rejected() {
        assert_eq!(
            CallKey::from_bytes(&[0; 31]).unwrap_err(),
            HecateError::invalid(CALL_KEY_RULE)
        );
        assert!(CallKey::from_bytes(&[0; 33]).is_err());
        assert!(CallKey::from_x("AAAA").is_err());
        assert!(CallKey::from_x(&format!("{RFC_X}=")).is_err());
        assert_eq!(CallKey::from_bytes(&[1; 32]).unwrap().bytes(), &[1; 32]);
    }
}
