//! Recovery shares in the `HEC2` format (contracts/recovery-share.md): body of 42 bytes (version
//! 2, index, share set and 32 values in GF(256)), a 4-byte SHA-256 checksum, Crockford Base32.

use std::fmt;

use aws_lc_rs::digest::{SHA256, digest};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::sealing::errors::HecateError;
use crate::sensitive::SecretBytes;

pub const PREFIX: &str = "HEC2";
pub const VERSION: u8 = 2;
pub const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECKSUM_LEN: usize = 4;
const SET_ID_LEN: usize = 4;
const VALUES_LEN: usize = 32;
const BODY_LEN: usize = 1 + 1 + SET_ID_LEN + VALUES_LEN;

fn invalid(message: &str) -> HecateError {
    HecateError::invalid(message)
}

/// Whitespace as Python's `str.isspace` sees it, so the same typing slips are cleaned.
fn is_space(ch: char) -> bool {
    ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
}

fn checksum(body: &[u8]) -> [u8; CHECKSUM_LEN] {
    let mut sum = [0u8; CHECKSUM_LEN];
    sum.copy_from_slice(&digest(&SHA256, body).as_ref()[..CHECKSUM_LEN]);
    sum
}

fn b32_encode(data: &[u8]) -> Zeroizing<String> {
    let alphabet = ALPHABET.as_bytes();
    let mut text = Zeroizing::new(String::with_capacity(data.len() * 8 / 5 + 1));
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in data {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            text.push(alphabet[((buffer >> bits) & 31) as usize] as char);
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        text.push(alphabet[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    text
}

fn b32_decode(text: &str, length: usize) -> Result<Zeroizing<Vec<u8>>, HecateError> {
    let total = text.len() * 5;
    if total < length * 8 || total - length * 8 >= 5 {
        return Err(invalid("recovery share has an invalid length"));
    }
    let mut out = Zeroizing::new(Vec::with_capacity(length));
    let (mut buffer, mut bits) = (0u32, 0u32);
    for ch in text.bytes() {
        let value = ALPHABET
            .bytes()
            .position(|letter| letter == ch)
            .unwrap_or(0) as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 && out.len() < length {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    if buffer != 0 {
        return Err(invalid(
            "recovery share is not valid: check how it was copied",
        ));
    }
    Ok(out)
}

/// A recovery share: its index, its share set and its 32 values, which are the secret part. It
/// erases itself when dropped and cannot be cloned; its `Debug` shows only the index (research
/// R14: no `Debug` that shows the values).
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_domain::sealing::share::RecoveryShare>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_domain::sealing::share::RecoveryShare>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RecoveryShare {
    index: u8,
    share_set_id: [u8; SET_ID_LEN],
    values: SecretBytes,
}

impl RecoveryShare {
    pub fn new(
        index: u8,
        share_set_id: [u8; SET_ID_LEN],
        values: Vec<u8>,
    ) -> Result<Self, HecateError> {
        let values = SecretBytes::new(values);
        if !(1..=3).contains(&index) {
            return Err(invalid("recovery share index must be 1, 2 or 3"));
        }
        if values.expose().len() != VALUES_LEN {
            return Err(invalid("recovery share has an invalid length"));
        }
        Ok(Self {
            index,
            share_set_id,
            values,
        })
    }

    pub fn index(&self) -> u8 {
        self.index
    }

    pub fn share_set_id(&self) -> &[u8; SET_ID_LEN] {
        &self.share_set_id
    }

    pub fn values(&self) -> &[u8] {
        self.values.expose()
    }

    fn body(&self) -> Zeroizing<Vec<u8>> {
        let mut body = Zeroizing::new(Vec::with_capacity(BODY_LEN));
        body.push(VERSION);
        body.push(self.index);
        body.extend_from_slice(&self.share_set_id);
        body.extend_from_slice(self.values.expose());
        body
    }

    /// `HEC2-XXXX-XXXX-…`: body and checksum in Crockford Base32, in groups of four.
    pub fn encode(&self) -> Zeroizing<String> {
        let mut raw = self.body();
        let sum = checksum(&raw);
        raw.extend_from_slice(&sum);
        let text = b32_encode(&raw);
        let mut encoded = Zeroizing::new(String::from(PREFIX));
        for group in text.as_bytes().chunks(4) {
            encoded.push('-');
            encoded.push_str(std::str::from_utf8(group).unwrap_or_default());
        }
        encoded
    }

    /// Reads a share typed by the admin: spaces and dashes are dropped, letters upper-cased and
    /// O, I and L read as 0, 1 and 1.
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let stripped: Zeroizing<String> = Zeroizing::new(
            text.chars()
                .filter(|ch| !is_space(*ch) && *ch != '-')
                .collect(),
        );
        let upper = Zeroizing::new(stripped.to_uppercase());
        let cleaned: Zeroizing<String> = Zeroizing::new(
            upper
                .chars()
                .map(|ch| match ch {
                    'O' => '0',
                    'I' | 'L' => '1',
                    other => other,
                })
                .collect(),
        );
        let Some(body_text) = cleaned.strip_prefix(PREFIX) else {
            return Err(invalid("recovery share must start with HEC2"));
        };
        if body_text.chars().any(|ch| !ALPHABET.contains(ch)) {
            return Err(invalid("recovery share has invalid characters"));
        }
        let raw = b32_decode(body_text, BODY_LEN + CHECKSUM_LEN)?;
        let (body, sum) = raw.split_at(BODY_LEN);
        if checksum(body) != sum {
            return Err(invalid(
                "recovery share checksum does not match: check how it was copied",
            ));
        }
        if body[0] != VERSION {
            return Err(invalid("unsupported recovery share version"));
        }
        let mut share_set_id = [0u8; SET_ID_LEN];
        share_set_id.copy_from_slice(&body[2..2 + SET_ID_LEN]);
        Self::new(body[1], share_set_id, body[2 + SET_ID_LEN..].to_vec())
    }
}

impl PartialEq for RecoveryShare {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
            && self.share_set_id == other.share_set_id
            && self.values.expose() == other.values.expose()
    }
}

impl fmt::Debug for RecoveryShare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryShare")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn share_strategy() -> impl Strategy<Value = RecoveryShare> {
        (1u8..=3, any::<[u8; 4]>(), any::<[u8; 32]>()).prop_map(|(index, set, values)| {
            RecoveryShare::new(index, set, values.to_vec()).unwrap()
        })
    }

    proptest! {
        // py: unit/domain/test_share.py::test_text_roundtrip
        #[test]
        fn text_roundtrip(share in share_strategy()) {
            let text = share.encode();
            prop_assert!(text.starts_with("HEC2-"));
            prop_assert!(text.split('-').all(|group| group.len() <= 4));
            prop_assert!(RecoveryShare::parse(&text).unwrap() == share);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        // py: unit/domain/test_share.py::test_any_changed_character_is_detected
        #[test]
        fn any_changed_character_is_detected(
            share in share_strategy(),
            position in any::<proptest::sample::Index>(),
            replacement in any::<proptest::sample::Index>(),
        ) {
            let text = share.encode();
            let body: Vec<usize> = text
                .char_indices()
                .filter(|(at, ch)| *at > PREFIX.len() && *ch != '-')
                .map(|(at, _)| at)
                .collect();
            let at = body[position.index(body.len())];
            let current = text.as_bytes()[at] as char;
            let choices: Vec<char> = ALPHABET.chars().filter(|ch| *ch != current).collect();
            let mut corrupted = text.clone();
            corrupted.replace_range(at..at + 1, &choices[replacement.index(choices.len())].to_string());
            prop_assert!(RecoveryShare::parse(&corrupted).is_err());
        }
    }

    proptest! {
        // py: unit/domain/test_share.py::test_lowercase_spaces_and_confusable_letters_are_accepted
        #[test]
        fn lowercase_spaces_and_confusable_letters_are_accepted(share in share_strategy()) {
            let typed = share
                .encode()
                .to_lowercase()
                .replace('-', " ")
                .replace('0', "o")
                .replace('1', "l");
            prop_assert!(RecoveryShare::parse(&typed).unwrap() == share);
        }
    }

    // py: unit/domain/test_share.py::test_truncated_or_wrong_prefix_is_rejected
    #[test]
    fn truncated_or_wrong_prefix_is_rejected() {
        let text = RecoveryShare::new(1, *b"abcd", vec![0; 32])
            .unwrap()
            .encode();
        assert!(RecoveryShare::parse(&text[..text.len() - 3]).is_err());
        assert!(RecoveryShare::parse(&format!("XYZ1-{}", &text[5..])).is_err());
        assert!(RecoveryShare::parse("").is_err());
    }

    // py: unit/domain/test_share.py::test_repr_does_not_show_fragments
    #[test]
    fn repr_does_not_show_fragments() {
        let share = RecoveryShare::new(2, *b"abcd", vec![1; 32]).unwrap();
        let shown = format!("{share:?}");
        assert!(!shown.contains(&"01".repeat(8)));
        assert!(!shown.contains("1, 1, 1"));
        assert!(shown.contains("index: 2"));
    }

    #[test]
    fn a_share_in_the_python_hec1_format_is_rejected_as_badly_copied() {
        let hec1 = RecoveryShare::new(1, *b"abcd", vec![7; 32])
            .unwrap()
            .encode()
            .replacen("HEC2", "HEC1", 1);
        let error = RecoveryShare::parse(&hec1).err().unwrap();
        assert_eq!(
            error,
            HecateError::invalid("recovery share must start with HEC2")
        );
    }
}
