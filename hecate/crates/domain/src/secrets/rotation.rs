//! `rotate_every` (FR-023, contracts/formats.md): an ISO 8601 duration in days or weeks, `P<n>D`
//! with n from 1 to 3650 or `P<n>W` with n from 1 to 520, in capitals, with n in decimal digits
//! without a sign or leading zeros, and nothing before or after. A month or a year has no fixed
//! length, so the other forms of ISO 8601 are refused.

use time::{Duration, OffsetDateTime};

use crate::sealing::errors::HecateError;

pub const ROTATE_EVERY_RULE: &str =
    "rotate_every must be an ISO 8601 duration in days or weeks, such as P90D or P12W";

/// The most digits that n has: 3650 days.
const MAX_DIGITS: usize = 4;

/// How often a secret must be rotated, as its `rotate_every` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RotationInterval {
    days: u32,
}

impl RotationInterval {
    pub fn parse(text: &str) -> Result<Self, HecateError> {
        let invalid = || HecateError::invalid(ROTATE_EVERY_RULE);
        let rest = text.strip_prefix('P').ok_or_else(invalid)?;
        let (digits, days_each, most) = if let Some(digits) = rest.strip_suffix('D') {
            (digits, 1, 3650)
        } else if let Some(digits) = rest.strip_suffix('W') {
            (digits, 7, 520)
        } else {
            return Err(invalid());
        };
        // Only decimal digits without a leading zero, before converting: a plain parse of a
        // number would take a leading `+`.
        let well_formed = (1..=MAX_DIGITS).contains(&digits.len())
            && !digits.starts_with('0')
            && digits.bytes().all(|byte| byte.is_ascii_digit());
        if !well_formed {
            return Err(invalid());
        }
        let count: u32 = digits.parse().map_err(|_| invalid())?;
        if count > most {
            return Err(invalid());
        }
        Ok(Self {
            days: count * days_each,
        })
    }

    pub fn duration(self) -> Duration {
        Duration::days(i64::from(self.days))
    }

    /// Whether a version created at `created_at` is due at `now`: at the exact instant that the
    /// duration is met it already is.
    pub fn is_due(self, created_at: OffsetDateTime, now: OffsetDateTime) -> bool {
        created_at + self.duration() <= now
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;
    use time::{Duration, OffsetDateTime};

    use super::*;

    const CREATED: OffsetDateTime = datetime!(2026-11-11 10:00 UTC);

    #[test]
    fn days_and_weeks_within_their_limits_are_durations() {
        for (text, days) in [
            ("P1D", 1),
            ("P90D", 90),
            ("P365D", 365),
            ("P3650D", 3650),
            ("P1W", 7),
            ("P12W", 84),
            ("P520W", 3640),
        ] {
            let interval = RotationInterval::parse(text).unwrap();
            assert_eq!(interval.duration(), Duration::days(days), "{text}");
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for text in [
            "P0D",
            "P0W",
            "P090D",
            "P012W",
            "P+90D",
            "P-1D",
            "PD",
            "PW",
            "P3651D",
            "P521W",
            "P99999999999D",
            "p90d",
            "P90d",
            "90d",
            "P1W2D",
            "P3M",
            "P1Y",
            "PT36H",
            "P1.5D",
            "",
            "P",
            " P90D",
            "P90D ",
            "P९०D",
        ] {
            let refused = RotationInterval::parse(text).unwrap_err();
            assert_eq!(refused.to_string(), ROTATE_EVERY_RULE, "{text:?}");
        }
    }

    #[test]
    fn a_secret_is_due_at_the_exact_instant_that_its_duration_is_met() {
        let interval = RotationInterval::parse("P90D").unwrap();
        let due = CREATED + Duration::days(90);
        assert!(!interval.is_due(CREATED, due - Duration::microseconds(1)));
        assert!(interval.is_due(CREATED, due));
        assert!(interval.is_due(CREATED, due + Duration::days(1)));
        let weekly = RotationInterval::parse("P1W").unwrap();
        assert!(weekly.is_due(CREATED, CREATED + Duration::days(7)));
        assert!(!weekly.is_due(CREATED, CREATED + Duration::days(6)));
    }
}
