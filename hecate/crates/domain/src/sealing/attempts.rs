use time::{Duration, OffsetDateTime};

const FREE_FAILURES: u32 = 2;
const MAX_WAIT_MINUTES: i64 = 15;

/// Consecutive failed unlock or proof attempts and the wait they cause: no wait up to the second
/// failure; from the third, 1, 2, 4 and 8 minutes and then 15.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttemptPolicy {
    pub consecutive_failures: u32,
    pub next_attempt_at: Option<OffsetDateTime>,
}

impl AttemptPolicy {
    pub fn register_failure(&self, now: OffsetDateTime) -> Self {
        let failures = self.consecutive_failures.saturating_add(1);
        if failures <= FREE_FAILURES {
            return Self {
                consecutive_failures: failures,
                next_attempt_at: None,
            };
        }
        let exponent = failures - FREE_FAILURES - 1;
        let minutes = if exponent >= 4 {
            MAX_WAIT_MINUTES
        } else {
            (1i64 << exponent).min(MAX_WAIT_MINUTES)
        };
        Self {
            consecutive_failures: failures,
            next_attempt_at: Some(now + Duration::minutes(minutes)),
        }
    }

    pub fn register_success(&self) -> Self {
        Self::default()
    }

    /// Seconds until the next attempt is allowed, rounded up.
    pub fn retry_after(&self, now: OffsetDateTime) -> u64 {
        let Some(next) = self.next_attempt_at else {
            return 0;
        };
        let remaining = next - now;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2026-11-07 12:00 UTC);

    fn fail_n(n: u32) -> AttemptPolicy {
        (0..n).fold(AttemptPolicy::default(), |policy, _| {
            policy.register_failure(T0)
        })
    }

    // py: unit/domain/test_attempts.py::test_no_wait_until_second_failure
    #[test]
    fn no_wait_until_second_failure() {
        assert_eq!(AttemptPolicy::default().retry_after(T0), 0);
        assert_eq!(fail_n(1).retry_after(T0), 0);
        assert_eq!(fail_n(2).retry_after(T0), 0);
    }

    // py: unit/domain/test_attempts.py::test_wait_series_after_third_failure
    #[test]
    fn wait_series_after_third_failure() {
        let waits: Vec<u64> = (3..10).map(|n| fail_n(n).retry_after(T0)).collect();
        assert_eq!(waits, vec![60, 120, 240, 480, 900, 900, 900]);
    }

    // py: unit/domain/test_attempts.py::test_wait_counts_down_with_time
    #[test]
    fn wait_counts_down_with_time() {
        let policy = fail_n(3);
        assert_eq!(policy.retry_after(T0 + Duration::seconds(30)), 30);
        assert_eq!(policy.retry_after(T0 + Duration::seconds(60)), 0);
        assert_eq!(policy.retry_after(T0 + Duration::minutes(5)), 0);
    }

    // py: unit/domain/test_attempts.py::test_failures_keep_counting_after_wait_expires
    #[test]
    fn failures_keep_counting_after_wait_expires() {
        let policy = fail_n(3);
        let later = T0 + Duration::minutes(2);
        assert_eq!(policy.register_failure(later).consecutive_failures, 4);
        assert_eq!(policy.register_failure(later).retry_after(later), 120);
    }

    // py: unit/domain/test_attempts.py::test_success_resets
    #[test]
    fn success_resets() {
        let policy = fail_n(5).register_success();
        assert_eq!(policy, AttemptPolicy::default());
        assert_eq!(policy.retry_after(T0), 0);
    }
}
