"""The start of `hermes serve` (research R1): its steps in order, and ready only when all of them are done.

Identity, publications of Hécate, password of the database, connection, migrations, configuration, test of the
instances and polling. A step that fails is retried after a while, and Hermes says which one it waits on; without an
identity, it waits for `hermes enroll`.
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable, Sequence
from dataclasses import dataclass

from hermes.domain.shared.errors import HermesError

RETRY_S = 30.0

logger = logging.getLogger(__name__)

Run = Callable[[], Awaitable[None]]


class NeedsEnrollment(Exception):
    """Hermes has no valid identity: the start waits for an enrollment."""


@dataclass(frozen=True)
class StartupStep:
    name: str
    run: Run


def startup_steps(
    *,
    identity: Run,
    publications: Run,
    database_password: Run,
    database: Run,
    migrations: Run,
    configuration: Run,
    instances: Run | None = None,
    polling: Run | None = None,
) -> list[StartupStep]:
    steps = [
        StartupStep("identity", identity),
        StartupStep("publications", publications),
        StartupStep("database_password", database_password),
        StartupStep("database", database),
        StartupStep("migrations", migrations),
        StartupStep("configuration", configuration),
    ]
    if instances is not None:
        steps.append(StartupStep("instances", instances))
    if polling is not None:
        steps.append(StartupStep("polling", polling))
    return steps


class Startup:
    def __init__(
        self,
        steps: Sequence[StartupStep],
        *,
        wait_for_enrollment: Run,
        retry_s: float = RETRY_S,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
    ) -> None:
        self._steps = list(steps)
        self._wait_for_enrollment = wait_for_enrollment
        self._retry_s = retry_s
        self._sleep = sleep
        self._ready = False
        self._step: str | None = self._steps[0].name if self._steps else None

    @property
    def ready(self) -> bool:
        return self._ready

    @property
    def step(self) -> str | None:
        """The step it is on, or waits on; None once ready."""
        return self._step

    async def run(self) -> None:
        index = 0
        while index < len(self._steps):
            step = self._steps[index]
            self._step = step.name
            try:
                await step.run()
            except NeedsEnrollment:
                logger.info("startup.waiting", extra={"fields": {"step": step.name, "for": "enrollment"}})
                await self._wait_for_enrollment()
                continue
            except Exception as error:
                reason = error.code.value if isinstance(error, HermesError) else type(error).__name__
                logger.warning("startup.failed", extra={"fields": {"step": step.name, "reason": reason}})
                await self._sleep(self._retry_s)
                continue
            index += 1
        self._step = None
        self._ready = True
        logger.info("startup.ready")
