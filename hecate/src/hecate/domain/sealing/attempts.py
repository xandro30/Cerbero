import math
from dataclasses import dataclass
from datetime import datetime, timedelta

FREE_FAILURES = 2
MAX_WAIT_MINUTES = 15


@dataclass(frozen=True)
class AttemptPolicy:
    """Consecutive failed unlock/proof attempts and the wait they cause (FR-016).

    No wait up to the 2nd failure; from the 3rd the wait is 1, 2, 4, 8 minutes and then 15.
    """

    consecutive_failures: int = 0
    next_attempt_at: datetime | None = None

    def register_failure(self, now: datetime) -> "AttemptPolicy":
        failures = self.consecutive_failures + 1
        if failures <= FREE_FAILURES:
            return AttemptPolicy(failures, None)
        minutes = min(MAX_WAIT_MINUTES, 2 ** (failures - FREE_FAILURES - 1))
        return AttemptPolicy(failures, now + timedelta(minutes=minutes))

    def register_success(self) -> "AttemptPolicy":
        return AttemptPolicy()

    def retry_after(self, now: datetime) -> int:
        if self.next_attempt_at is None:
            return 0
        remaining = (self.next_attempt_at - now).total_seconds()
        return max(0, math.ceil(remaining))
