from datetime import UTC, datetime, timedelta

from hecate.domain.sealing.attempts import AttemptPolicy

T0 = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)


def fail_n(n: int, now: datetime = T0) -> AttemptPolicy:
    policy = AttemptPolicy()
    for _ in range(n):
        policy = policy.register_failure(now)
    return policy


def test_no_wait_until_second_failure():
    assert AttemptPolicy().retry_after(T0) == 0
    assert fail_n(1).retry_after(T0) == 0
    assert fail_n(2).retry_after(T0) == 0


def test_wait_series_after_third_failure():
    waits = [fail_n(n).retry_after(T0) for n in range(3, 10)]
    assert waits == [60, 120, 240, 480, 900, 900, 900]


def test_wait_counts_down_with_time():
    policy = fail_n(3)
    assert policy.retry_after(T0 + timedelta(seconds=30)) == 30
    assert policy.retry_after(T0 + timedelta(seconds=60)) == 0
    assert policy.retry_after(T0 + timedelta(minutes=5)) == 0


def test_failures_keep_counting_after_wait_expires():
    policy = fail_n(3)
    later = T0 + timedelta(minutes=2)
    assert policy.register_failure(later).consecutive_failures == 4
    assert policy.register_failure(later).retry_after(later) == 120


def test_success_resets():
    policy = fail_n(5).register_success()
    assert policy == AttemptPolicy()
    assert policy.retry_after(T0) == 0
