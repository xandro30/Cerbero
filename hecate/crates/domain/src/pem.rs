//! The PEM reader of RFC 7468 (FR-018 of change 005 and FR-029): ASCII spaces around the body,
//! non-empty blocks and strict base64. A secret of type `pem` takes one or more blocks of any
//! label; the inputs of the identities take exactly one block of the expected label.

use data_encoding::BASE64;

use crate::sealing::errors::HecateError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub label: String,
    pub der: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PemError {
    NotPem,
    EmptyBlock,
    InvalidBase64,
}

/// The ASCII whitespace of RFC 7468: space, tab, CR, LF, VT and FF.
pub fn is_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c')
}

fn is_body(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=') || is_space(character)
}

/// One block of Python's `-----BEGIN (L)-----\s*\n(body)\s*\n-----END \1-----` at the start of
/// `text`: the length it takes, its label and the body without spaces, or `None` if it does not
/// match.
fn pem_block(text: &str) -> Option<(usize, String, String)> {
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
    Some((
        taken,
        label.to_string(),
        run.chars().filter(|c| !is_space(*c)).collect(),
    ))
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

/// The labels and bodies of the blocks of a text, checked but not decoded.
fn scan(value: &str) -> Result<Vec<(String, String)>, PemError> {
    let text = value.trim_matches(is_space);
    let mut position = 0;
    let mut found = Vec::new();
    while position < text.len() {
        let (taken, label, body) = pem_block(&text[position..]).ok_or(PemError::NotPem)?;
        if body.is_empty() {
            return Err(PemError::EmptyBlock);
        }
        if !is_base64(&body) {
            return Err(PemError::InvalidBase64);
        }
        found.push((label, body));
        position += taken;
        position += text[position..]
            .find(|c: char| !is_space(c))
            .unwrap_or(text.len() - position);
    }
    if found.is_empty() {
        return Err(PemError::NotPem);
    }
    Ok(found)
}

/// Checks one or more blocks of any label, as the secret type `pem` does.
pub fn check(value: &str) -> Result<(), PemError> {
    scan(value).map(|_| ())
}

/// One or more blocks of any label, with their decoded content.
pub fn blocks(value: &str) -> Result<Vec<Block>, PemError> {
    scan(value)?
        .into_iter()
        .map(|(label, body)| {
            let der = BASE64
                .decode(body.as_bytes())
                .map_err(|_| PemError::InvalidBase64)?;
            Ok(Block { label, der })
        })
        .collect()
}

/// Exactly one block of `label`, with its decoded content; anything else is `validation`.
pub fn single(value: &str, label: &str) -> Result<Vec<u8>, HecateError> {
    match blocks(value) {
        Ok(mut found) if found.len() == 1 && found[0].label == label => Ok(found.remove(0).der),
        _ => Err(HecateError::invalid(format!(
            "the value is not a single PEM block {label}"
        ))),
    }
}

/// One block of `label` in lines of 64 characters, as RFC 7468 writes it, ending in a line break.
pub fn encode(label: &str, der: &[u8]) -> String {
    let body = BASE64.encode(der);
    let mut text = String::with_capacity(body.len() + body.len() / 64 + 2 * label.len() + 40);
    text.push_str("-----BEGIN ");
    text.push_str(label);
    text.push_str("-----\n");
    for line in body.as_bytes().chunks(64) {
        text.push_str(std::str::from_utf8(line).unwrap_or_default());
        text.push('\n');
    }
    text.push_str("-----END ");
    text.push_str(label);
    text.push_str("-----\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n";

    #[test]
    fn one_block_of_the_expected_label_gives_its_der() {
        assert_eq!(single(CERT, "CERTIFICATE").unwrap(), vec![1, 2, 3]);
        let crlf = CERT.replace('\n', "\r\n");
        assert_eq!(single(&crlf, "CERTIFICATE").unwrap(), vec![1, 2, 3]);
        let wrapped = "-----BEGIN PUBLIC KEY-----\nAQID\nBAUG\n-----END PUBLIC KEY-----";
        assert_eq!(
            single(wrapped, "PUBLIC KEY").unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn anything_but_one_block_of_the_expected_label_is_rejected() {
        let two = format!("{CERT}{CERT}");
        let empty = "-----BEGIN CERTIFICATE-----\n\n-----END CERTIFICATE-----\n";
        let bad_base64 = "-----BEGIN CERTIFICATE-----\nAQI\n-----END CERTIFICATE-----\n";
        let junk = format!("junk\n{CERT}");
        let nbsp = CERT.replace('\n', "\u{a0}\n");
        for (text, label) in [
            (CERT, "CERTIFICATE REQUEST"),
            (two.as_str(), "CERTIFICATE"),
            (empty, "CERTIFICATE"),
            (bad_base64, "CERTIFICATE"),
            (junk.as_str(), "CERTIFICATE"),
            (nbsp.as_str(), "CERTIFICATE"),
            ("", "CERTIFICATE"),
        ] {
            assert!(single(text, label).is_err(), "{text:?} as {label}");
        }
    }

    #[test]
    fn blocks_reads_one_or_more_blocks_of_any_label() {
        let two = format!("{CERT}-----BEGIN X509 CRL-----\nBAUG\n-----END X509 CRL-----\n");
        let read = blocks(&two).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(
            (read[0].label.as_str(), read[0].der.clone()),
            ("CERTIFICATE", vec![1, 2, 3])
        );
        assert_eq!(
            (read[1].label.as_str(), read[1].der.clone()),
            ("X509 CRL", vec![4, 5, 6])
        );
        assert_eq!(blocks("not a pem").unwrap_err(), PemError::NotPem);
        assert_eq!(
            blocks("-----BEGIN A-----\n \n-----END A-----").unwrap_err(),
            PemError::EmptyBlock
        );
        assert_eq!(
            blocks("-----BEGIN A-----\nAQI\n-----END A-----").unwrap_err(),
            PemError::InvalidBase64
        );
    }

    #[test]
    fn encode_writes_lines_of_64_characters_that_single_reads_back() {
        let der: Vec<u8> = (0..100).collect();
        let text = encode("CERTIFICATE", &der);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.first(), Some(&"-----BEGIN CERTIFICATE-----"));
        assert_eq!(lines.last(), Some(&"-----END CERTIFICATE-----"));
        assert!(
            lines[1..lines.len() - 1]
                .iter()
                .all(|line| line.len() <= 64)
        );
        assert_eq!(lines[1].len(), 64);
        assert!(text.ends_with('\n'));
        assert_eq!(single(&text, "CERTIFICATE").unwrap(), der);
    }
}
