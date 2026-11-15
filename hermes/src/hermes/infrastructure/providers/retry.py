"""The retries of a send that did not leave (research R10, FR-027).

Each library says what did not leave with `NotSent`: no connection, or a limit of the provider with the wait it asks
for. Those are retried with exponential waits and full jitter (base 0.2 s, top 2 s), or with the wait of the provider,
while the technical timeout of the function lasts; each attempt gets only the time that is left. When a wait does not
fit, the send ends: `rate_limited` with the wait of the provider, or `provider_unavailable` if it never connected.
What may have left is never retried: the library raises its `ProviderFailure` instead.
"""

from __future__ import annotations

import asyncio
import logging
import math
import random
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field

from hermes.domain.services.chat.provider import ProviderFailure
from hermes.domain.shared.errors import ErrorCode

BASE_S = 0.2
TOP_S = 2.0

logger = logging.getLogger(__name__)


class NotSent(Exception):
    """The request did not leave Hermes, or the provider limited it without acting: it can go again."""

    def __init__(self, retry_after_s: float | None = None) -> None:
        super().__init__("the request did not leave")
        self.retry_after_s = retry_after_s


@dataclass(frozen=True)
class RetryTiming:
    """Where the waits and the time come from; the tests give a clock of their own."""

    sleep: Callable[[float], Awaitable[None]] = asyncio.sleep
    monotonic: Callable[[], float] = time.monotonic
    jitter: Callable[[], float] = field(default=random.random)  # a jitter, not a secret


async def send_with_retries[T](
    attempt: Callable[[float], Awaitable[T]], *, timeout_s: float, provider: str, timing: RetryTiming
) -> T:
    """The result of the first attempt that leaves; `attempt` receives the seconds it has."""
    deadline = timing.monotonic() + timeout_s
    failures = 0
    while True:
        try:
            return await attempt(max(deadline - timing.monotonic(), 0.0))
        except NotSent as failure:
            failures += 1
            left = deadline - timing.monotonic()
            if failure.retry_after_s is not None:
                wait = failure.retry_after_s
                if wait >= left:
                    raise ProviderFailure(
                        ErrorCode.RATE_LIMITED,
                        f"{provider} limits the instance for longer than the time of the function",
                        retry_after_s=max(1, math.ceil(wait)),
                    ) from None
            else:
                wait = timing.jitter() * min(TOP_S, BASE_S * 2 ** (failures - 1))
                if wait >= left:
                    raise ProviderFailure(
                        ErrorCode.PROVIDER_UNAVAILABLE, f"{provider} did not answer in the time of the function"
                    ) from None
            logger.info(
                "provider.retry",
                extra={"fields": {"provider": provider, "attempt": failures, "wait_s": round(wait, 3)}},
            )
            await timing.sleep(wait)
