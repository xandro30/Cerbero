//! Instants written like Python's `datetime.isoformat()` and read in extended ISO 8601 (research R6).

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not an extended ISO 8601 date and time")]
pub struct InvalidInstant;

/// `datetime.isoformat()`: no fraction when the microseconds are 0, six digits otherwise.
pub fn isoformat(instant: OffsetDateTime) -> String {
    let mut text = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second()
    );
    if instant.microsecond() != 0 {
        text.push_str(&format!(".{:06}", instant.microsecond()));
    }
    let offset = instant.offset().whole_seconds();
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.unsigned_abs();
    text.push_str(&format!(
        "{sign}{:02}:{:02}",
        offset / 3600,
        offset % 3600 / 60
    ));
    if !offset.is_multiple_of(60) {
        text.push_str(&format!(":{:02}", offset % 60));
    }
    text
}

/// `YYYY-MM-DD`, or `YYYY-MM-DDTHH:MM[:SS[.fraction]]` with `T` or a space, the fraction cut to
/// microseconds and an optional `Z` or `±HH:MM` offset; without an offset the instant is UTC.
pub fn parse_iso(text: &str) -> Result<OffsetDateTime, InvalidInstant> {
    let mut cursor = Cursor {
        bytes: text.as_bytes(),
        at: 0,
    };
    let year = cursor.number(4)?;
    cursor.expect(b'-')?;
    let month = cursor.number(2)?;
    cursor.expect(b'-')?;
    let day = cursor.number(2)?;
    if year == 0 {
        return Err(InvalidInstant);
    }
    let month = Month::try_from(month as u8).map_err(|_| InvalidInstant)?;
    let date =
        Date::from_calendar_date(year as i32, month, day as u8).map_err(|_| InvalidInstant)?;
    if cursor.done() {
        return Ok(PrimitiveDateTime::new(date, Time::MIDNIGHT).assume_utc());
    }
    if !matches!(cursor.next(), Some(b'T' | b' ')) {
        return Err(InvalidInstant);
    }
    let hour = cursor.number(2)?;
    cursor.expect(b':')?;
    let minute = cursor.number(2)?;
    let (mut second, mut microsecond) = (0, 0);
    if cursor.peek() == Some(b':') {
        cursor.next();
        second = cursor.number(2)?;
        if cursor.peek() == Some(b'.') {
            cursor.next();
            microsecond = cursor.fraction()?;
        }
    }
    let offset = match cursor.next() {
        None => UtcOffset::UTC,
        Some(b'Z') => UtcOffset::UTC,
        Some(sign @ (b'+' | b'-')) => {
            let hours = cursor.number(2)?;
            cursor.expect(b':')?;
            let minutes = cursor.number(2)?;
            if hours > 23 || minutes > 59 {
                return Err(InvalidInstant);
            }
            let seconds = (hours * 3600 + minutes * 60) as i32;
            UtcOffset::from_whole_seconds(if sign == b'-' { -seconds } else { seconds })
                .map_err(|_| InvalidInstant)?
        }
        Some(_) => return Err(InvalidInstant),
    };
    if !cursor.done() || hour > 23 || minute > 59 || second > 59 {
        return Err(InvalidInstant);
    }
    let time = Time::from_hms_micro(hour as u8, minute as u8, second as u8, microsecond)
        .map_err(|_| InvalidInstant)?;
    Ok(PrimitiveDateTime::new(date, time).assume_offset(offset))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn done(&self) -> bool {
        self.at == self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    fn expect(&mut self, wanted: u8) -> Result<(), InvalidInstant> {
        (self.next() == Some(wanted))
            .then_some(())
            .ok_or(InvalidInstant)
    }

    fn number(&mut self, digits: usize) -> Result<u32, InvalidInstant> {
        let mut value = 0;
        for _ in 0..digits {
            match self.next() {
                Some(byte @ b'0'..=b'9') => value = value * 10 + u32::from(byte - b'0'),
                _ => return Err(InvalidInstant),
            }
        }
        Ok(value)
    }

    fn fraction(&mut self) -> Result<u32, InvalidInstant> {
        let (mut value, mut digits) = (0, 0);
        while let Some(byte @ b'0'..=b'9') = self.peek() {
            self.at += 1;
            if digits < 6 {
                value = value * 10 + u32::from(byte - b'0');
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(InvalidInstant);
        }
        Ok(value * 10u32.pow(6 - digits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vectors/instants.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn number(case: &Value, key: &str) -> i64 {
        case[key].as_i64().unwrap()
    }

    #[test]
    fn instants_are_written_like_python_isoformat() {
        let vectors = vectors();
        let cases = vectors["cases"].as_array().unwrap();
        for case in cases.iter().filter(|case| case["kind"] == "isoformat") {
            let month = Month::try_from(number(case, "month") as u8).unwrap();
            let date = Date::from_calendar_date(
                number(case, "year") as i32,
                month,
                number(case, "day") as u8,
            )
            .unwrap();
            let time = Time::from_hms_micro(
                number(case, "hour") as u8,
                number(case, "minute") as u8,
                number(case, "second") as u8,
                number(case, "microsecond") as u32,
            )
            .unwrap();
            let offset =
                UtcOffset::from_whole_seconds(number(case, "offset_seconds") as i32).unwrap();
            let instant = PrimitiveDateTime::new(date, time).assume_offset(offset);
            assert_eq!(isoformat(instant), case["text"].as_str().unwrap());
        }
    }

    #[test]
    fn instants_are_read_like_python_except_the_deliberate_changes() {
        let vectors = vectors();
        let cases = vectors["cases"].as_array().unwrap();
        for case in cases.iter().filter(|case| case["kind"] == "parse") {
            let input = case["input"].as_str().unwrap();
            let parsed = parse_iso(input);
            if case["deliberate"].as_bool().unwrap() || case["result"].is_null() {
                assert!(parsed.is_err(), "{input:?} must be rejected");
            } else {
                let parsed = parsed.unwrap_or_else(|_| panic!("{input:?} must be accepted"));
                assert_eq!(
                    isoformat(parsed),
                    case["result"].as_str().unwrap(),
                    "{input:?}"
                );
            }
        }
    }
}
