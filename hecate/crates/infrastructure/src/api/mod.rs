pub mod auth;
pub mod envelope;
pub mod json;
pub mod routes;
pub mod server;
pub mod supervisor;

use time::{OffsetDateTime, UtcOffset};

/// An instant of the API (`Instant` of the contract): UTC with six digits of fraction.
pub fn instant(at: OffsetDateTime) -> String {
    let at = at.to_offset(UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}+00:00",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.microsecond()
    )
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn instants_have_six_digits_of_fraction_in_utc() {
        assert_eq!(
            instant(datetime!(2026-11-12 12:00 UTC)),
            "2026-11-12T12:00:00.000000+00:00"
        );
        assert_eq!(
            instant(datetime!(2026-11-12 13:00:00.123456789 +01:00)),
            "2026-11-12T12:00:00.123456+00:00"
        );
    }
}
