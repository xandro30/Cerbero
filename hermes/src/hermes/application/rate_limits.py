"""The limit of use of each instance (research R13, FR-028): one bucket of tokens per instance, in memory.

A bucket holds as many tokens as requests per minute and gets them back one by one along the minute. Without a token,
the send answers `rate_limited` with the whole seconds until the next one, before the provider. The arithmetic is exact,
so no rounding leaves a bucket one millionth short of a token.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from datetime import datetime, timedelta
from fractions import Fraction

from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError

MINUTE_US = 60_000_000
LIMIT_REACHED = "the instance reached its limit of messages per minute"


@dataclass(frozen=True)
class _Bucket:
    tokens: Fraction
    at: datetime


class RateLimits:
    def __init__(self, clock: Clock) -> None:
        self._clock = clock
        self._buckets: dict[str, _Bucket] = {}

    def take(self, instance: str, per_minute: int) -> None:
        """One token of the instance, or `rate_limited`; a lower limit never leaves more tokens than it allows."""
        now = self._clock.now()
        bucket = self._buckets.get(instance)
        if bucket is None:
            tokens = Fraction(per_minute)
        else:
            elapsed_us = max((now - bucket.at) // timedelta(microseconds=1), 0)
            tokens = min(Fraction(per_minute), bucket.tokens + Fraction(elapsed_us * per_minute, MINUTE_US))
        if tokens >= 1:
            self._buckets[instance] = _Bucket(tokens - 1, now)
            return
        self._buckets[instance] = _Bucket(tokens, now)
        wait_s = (1 - tokens) * 60 / per_minute
        raise HermesError(ErrorCode.RATE_LIMITED, LIMIT_REACHED, retry_after_s=max(1, math.ceil(wait_s)))
