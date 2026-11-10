//! The digest of a request body that recognizes its exact repetition (research R12): the SHA-256
//! of its fields in canonical JSON, with sorted keys and no spaces.

use aws_lc_rs::digest;
use serde_json::{Map, Value};

pub fn request_digest(fields: &[(&str, &str)]) -> [u8; 32] {
    let object: Map<String, Value> = fields
        .iter()
        .map(|(name, value)| ((*name).to_string(), Value::from(*value)))
        .collect();
    let mut hash = [0; 32];
    hash.copy_from_slice(
        digest::digest(
            &digest::SHA256,
            Value::Object(object).to_string().as_bytes(),
        )
        .as_ref(),
    );
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_does_not_depend_on_the_order_of_the_fields() {
        let one = request_digest(&[("token", "t"), ("csr", "c"), ("call_key", "k")]);
        let other = request_digest(&[("call_key", "k"), ("csr", "c"), ("token", "t")]);
        assert_eq!(one, other);
        assert_ne!(
            one,
            request_digest(&[("call_key", "k"), ("csr", "c"), ("token", "u")])
        );
        let expected = digest::digest(
            &digest::SHA256,
            br#"{"call_key":"k","csr":"c","token":"t"}"#,
        );
        assert_eq!(one.as_slice(), expected.as_ref());
    }
}
