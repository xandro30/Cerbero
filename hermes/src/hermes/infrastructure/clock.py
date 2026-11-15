"""The clock of the system, the one that `hermes serve` gives to the use cases."""

from __future__ import annotations

from datetime import UTC, datetime


class SystemClock:
    def now(self) -> datetime:
        return datetime.now(UTC)
