//! The throttle of enrollments (FR-034, research R13): its own counter, apart from the shared
//! one of unsealing. The tenth failure in a row opens a wait of 1 minute, and the following ones
//! of 2, 4 and 8 and then 15, as the attempt counter of feature 001 does after its free failures.
//! Each wait is a batch that leaves one row when it closes.

use time::{Duration, OffsetDateTime};

/// The failures in a row that pass without a wait.
pub const FREE_FAILURES: u64 = 9;
const MAX_WAIT_MINUTES: i64 = 15;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnrollAttempts {
    pub consecutive_failures: u64,
    pub wait_until: Option<OffsetDateTime>,
    /// The requests refused with `rate_limited` during the current wait.
    pub rejected_during_wait: u64,
}

/// A batch that ended: the failures that opened it and the requests refused while it lasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedBatch {
    pub failures: u64,
    pub rejected: u64,
}

impl EnrollAttempts {
    /// Seconds until enrollments are accepted again, rounded up; 0 without a wait.
    pub fn retry_after(&self, now: OffsetDateTime) -> u64 {
        let Some(until) = self.wait_until else {
            return 0;
        };
        let remaining = until - now;
        if remaining <= Duration::ZERO {
            return 0;
        }
        let seconds = remaining.whole_seconds().unsigned_abs();
        if remaining.subsec_nanoseconds() > 0 {
            seconds + 1
        } else {
            seconds
        }
    }

    pub fn is_throttled(&self, now: OffsetDateTime) -> bool {
        self.retry_after(now) > 0
    }

    /// A refused enrollment; from the tenth in a row, the wait.
    pub fn register_failure(&self, now: OffsetDateTime) -> Self {
        let failures = self.consecutive_failures.saturating_add(1);
        let wait_until = (failures > FREE_FAILURES).then(|| {
            let exponent = failures - FREE_FAILURES - 1;
            let minutes = if exponent >= 4 {
                MAX_WAIT_MINUTES
            } else {
                (1i64 << exponent).min(MAX_WAIT_MINUTES)
            };
            now + Duration::minutes(minutes)
        });
        Self {
            consecutive_failures: failures,
            wait_until: wait_until.or(self.wait_until),
            rejected_during_wait: self.rejected_during_wait,
        }
    }

    /// A request refused by the wait itself: it is no failure.
    pub fn register_rejected(&self) -> Self {
        Self {
            rejected_during_wait: self.rejected_during_wait.saturating_add(1),
            ..self.clone()
        }
    }

    /// An issued enrollment puts the counter back to zero.
    pub fn register_success(&self) -> Self {
        Self::default()
    }

    /// The batch whose wait is over, and the attempts without it; the failures keep counting.
    pub fn close_batch(&self, now: OffsetDateTime) -> Option<(ClosedBatch, Self)> {
        let until = self.wait_until?;
        if now < until {
            return None;
        }
        Some((
            ClosedBatch {
                failures: self.consecutive_failures,
                rejected: self.rejected_during_wait,
            },
            Self {
                consecutive_failures: self.consecutive_failures,
                wait_until: None,
                rejected_during_wait: 0,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::datetime;

    use super::*;

    const T0: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);

    fn failed(times: u64, at: OffsetDateTime) -> EnrollAttempts {
        (0..times).fold(EnrollAttempts::default(), |attempts, _| {
            attempts.register_failure(at)
        })
    }

    #[test]
    fn the_tenth_failure_in_a_row_opens_a_wait_that_grows_to_15_minutes() {
        assert_eq!(failed(9, T0).retry_after(T0), 0);
        assert!(!failed(9, T0).is_throttled(T0));
        let mut attempts = failed(10, T0);
        assert_eq!(attempts.retry_after(T0), 60);
        let mut now = T0;
        let mut waits = Vec::new();
        for _ in 0..5 {
            now += Duration::minutes(16);
            attempts = attempts.register_failure(now);
            waits.push(attempts.retry_after(now));
        }
        assert_eq!(waits, vec![120, 240, 480, 900, 900]);
    }

    #[test]
    fn the_wait_counts_down_and_refused_requests_are_counted() {
        let attempts = failed(10, T0);
        assert!(attempts.is_throttled(T0 + Duration::seconds(30)));
        assert_eq!(attempts.retry_after(T0 + Duration::seconds(30)), 30);
        assert_eq!(attempts.retry_after(T0 + Duration::milliseconds(59_500)), 1);
        assert!(!attempts.is_throttled(T0 + Duration::seconds(60)));
        let refused = attempts.register_rejected().register_rejected();
        assert_eq!(refused.rejected_during_wait, 2);
        assert_eq!(
            refused.consecutive_failures, 10,
            "a refusal of the throttle is no failure"
        );
    }

    #[test]
    fn an_issued_enrollment_resets_the_counter() {
        assert_eq!(failed(12, T0).register_success(), EnrollAttempts::default());
    }

    #[test]
    fn a_batch_closes_once_its_wait_is_over_with_its_failures_and_refusals() {
        let attempts = failed(10, T0).register_rejected().register_rejected();
        assert_eq!(attempts.close_batch(T0 + Duration::seconds(59)), None);
        let (batch, after) = attempts.close_batch(T0 + Duration::seconds(60)).unwrap();
        assert_eq!(
            batch,
            ClosedBatch {
                failures: 10,
                rejected: 2
            }
        );
        assert_eq!(
            after,
            EnrollAttempts {
                consecutive_failures: 10,
                wait_until: None,
                rejected_during_wait: 0,
            }
        );
        assert_eq!(after.close_batch(T0 + Duration::hours(1)), None);
        let next = after.register_failure(T0 + Duration::minutes(2));
        assert_eq!(next.retry_after(T0 + Duration::minutes(2)), 120);
        assert_eq!(EnrollAttempts::default().close_batch(T0), None);
    }
}
