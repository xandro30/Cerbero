"""The background tasks of `hermes serve` (research R1): an action every interval, and a failure never stops it."""

import asyncio
import logging

import pytest

from hermes.infrastructure.background import every

pytestmark = pytest.mark.anyio


class StopAfter:
    def __init__(self, sleeps: int) -> None:
        self.left = sleeps
        self.intervals: list[float] = []

    async def __call__(self, interval: float) -> None:
        self.intervals.append(interval)
        if self.left == 0:
            raise asyncio.CancelledError
        self.left -= 1


async def test_the_action_runs_after_each_interval() -> None:
    runs: list[int] = []

    async def action() -> None:
        runs.append(len(runs))

    sleep = StopAfter(3)
    with pytest.raises(asyncio.CancelledError):
        await every(60, action, name="tokens.purge", sleep=sleep)
    assert runs == [0, 1, 2]
    assert sleep.intervals == [60, 60, 60, 60]


async def test_a_failure_is_logged_without_its_message_and_the_task_goes_on(caplog: pytest.LogCaptureFixture) -> None:
    calls = 0

    async def action() -> None:
        nonlocal calls
        calls += 1
        raise RuntimeError("postgresql://hermes:secret@db failed")

    caplog.set_level(logging.WARNING, logger="hermes")
    with pytest.raises(asyncio.CancelledError):
        await every(1, action, name="tokens.purge", sleep=StopAfter(2))
    assert calls == 2
    assert "tokens.purge.failed" in caplog.text
    assert "secret" not in caplog.text
