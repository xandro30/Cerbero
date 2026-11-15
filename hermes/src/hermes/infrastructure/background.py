"""The periodic tasks of `hermes serve` (research R1): an action after each interval, until the stop cancels it."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable

logger = logging.getLogger(__name__)


async def every(
    interval_s: float,
    action: Callable[[], Awaitable[None]],
    *,
    name: str,
    sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
) -> None:
    """A failure is logged with its type only, so no secret in its message reaches the log, and the task goes on."""
    while True:
        await sleep(interval_s)
        try:
            await action()
        except Exception as error:
            logger.warning(f"{name}.failed", extra={"fields": {"reason": type(error).__name__}})
