use hecate_application::ports::Clock;
use time::OffsetDateTime;

/// UTC now, cut to the microsecond like Python's `datetime`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        let now = OffsetDateTime::now_utc();
        now.replace_microsecond(now.microsecond()).unwrap_or(now)
    }
}
