//! Python's `str.isprintable()` over the Unicode 17 ranges that the vector generator wrote.

use std::cmp::Ordering;

pub(crate) use crate::printable_table::PRINTABLE;

/// Whether every character is printable, as `str.isprintable` (an empty text is).
pub fn is_printable(text: &str) -> bool {
    text.chars().all(|character| {
        let code = u32::from(character);
        PRINTABLE
            .binary_search_by(|&(start, end)| {
                if end < code {
                    Ordering::Less
                } else if start > code {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn the_table_matches_the_ranges_of_python() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/printable.json"
        );
        let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let ranges: Vec<(u32, u32)> = vectors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|range| {
                (
                    range[0].as_u64().unwrap() as u32,
                    range[1].as_u64().unwrap() as u32,
                )
            })
            .collect();
        assert_eq!(ranges, PRINTABLE);
    }

    #[test]
    fn a_text_is_printable_when_every_character_is() {
        assert!(is_printable("cada 30 días"));
        assert!(is_printable(""));
        assert!(!is_printable("tab\there"));
        assert!(!is_printable("line\n"));
        assert!(!is_printable("nbsp\u{a0}"));
    }
}
