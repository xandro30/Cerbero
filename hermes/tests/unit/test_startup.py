"""The start of `hermes serve` (T063, research R1): its steps in order, and not ready until all of them are done."""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable

import pytest

from hermes.application.startup import NeedsEnrollment, Startup, startup_steps

pytestmark = pytest.mark.anyio

ORDER = [
    "identity",
    "publications",
    "database_password",
    "database",
    "migrations",
    "configuration",
    "instances",
    "polling",
]


class Steps:
    def __init__(self) -> None:
        self.calls: list[str] = []
        self.failures: dict[str, int] = {}

    def __getitem__(self, name: str) -> Callable[[], Awaitable[None]]:
        async def run() -> None:
            self.calls.append(name)
            if self.failures.get(name, 0) > 0:
                self.failures[name] -= 1
                raise RuntimeError(f"{name} failed")

        return run

    def all(self) -> dict[str, Callable[[], Awaitable[None]]]:
        return {name: self[name] for name in ORDER}


async def never() -> None:
    await asyncio.Event().wait()


async def test_the_steps_follow_the_order_of_research_r1() -> None:
    steps = Steps()
    startup = Startup(startup_steps(**steps.all()), wait_for_enrollment=never)
    assert not startup.ready
    await startup.run()
    assert steps.calls == ORDER
    assert startup.ready
    assert startup.step is None


async def test_the_steps_of_later_stories_may_be_left_out() -> None:
    steps = Steps()
    everything = steps.all()
    del everything["instances"], everything["polling"]
    assert [step.name for step in startup_steps(**everything)] == ORDER[:-2]


async def test_a_failing_step_stops_before_ready_says_which_and_is_retried() -> None:
    steps = Steps()
    steps.failures["migrations"] = 1
    seen: list[tuple[bool, str | None, float]] = []
    startup: Startup

    async def sleep(seconds: float) -> None:
        seen.append((startup.ready, startup.step, seconds))

    startup = Startup(startup_steps(**steps.all()), wait_for_enrollment=never, retry_s=30, sleep=sleep)
    await startup.run()
    assert seen == [(False, "migrations", 30)]
    assert steps.calls == [*ORDER[:5], *ORDER[4:]]
    assert startup.ready


async def test_without_an_identity_it_waits_for_the_enrollment() -> None:
    enrolled = asyncio.Event()
    calls: list[str] = []

    async def identity() -> None:
        calls.append("identity")
        if not enrolled.is_set():
            raise NeedsEnrollment

    async def wait() -> None:
        await enrolled.wait()

    steps = Steps()
    named = steps.all()
    named["identity"] = identity
    startup = Startup(startup_steps(**named), wait_for_enrollment=wait)
    task = asyncio.create_task(startup.run())
    for _ in range(20):
        await asyncio.sleep(0)
    assert not startup.ready
    assert startup.step == "identity"
    assert steps.calls == []
    enrolled.set()
    await asyncio.wait_for(task, timeout=5)
    assert startup.ready
    assert calls == ["identity", "identity"]
    assert steps.calls == ORDER[1:]
