"""A clock that the tests move by hand."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

START = datetime(2026, 11, 14, 10, 0, tzinfo=UTC)


class FakeClock:
    """`sleep` moves the clock instead of waiting, and keeps each wait, so a retry of seconds takes no time."""

    def __init__(self, start: datetime = START) -> None:
        if start.tzinfo is None:
            raise ValueError("the clock works in aware datetimes")
        self._now = start
        self.slept: list[float] = []

    def now(self) -> datetime:
        return self._now

    def monotonic(self) -> float:
        return self._now.timestamp()

    async def sleep(self, seconds: float) -> None:
        self.slept.append(seconds)
        self.advance(seconds)

    def advance(self, delta: timedelta | float) -> None:
        step = delta if isinstance(delta, timedelta) else timedelta(seconds=delta)
        if step < timedelta(0):
            raise ValueError("the clock does not go back")
        self._now += step

    def set(self, moment: datetime) -> None:
        if moment.tzinfo is None:
            raise ValueError("the clock works in aware datetimes")
        self._now = moment
