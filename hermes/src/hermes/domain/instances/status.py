"""The state of an instance (data-model.md#estado-de-una-instancia, FR-013): it comes from its last ten calls and its
last connection test, in memory.

- `unavailable`: its last test failed, the provider rejected its credentials (until a test works) or its last three
  calls failed;
- `degraded`: any of its last ten calls failed;
- `healthy`: otherwise.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from enum import StrEnum

WINDOW = 10
FAILED_IN_A_ROW = 3


class InstanceStatus(StrEnum):
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNAVAILABLE = "unavailable"


@dataclass(frozen=True)
class InstanceRecord:
    """The last calls of an instance (`True` if it worked), its last test, and whether its credentials were rejected."""

    calls: tuple[bool, ...] = ()
    last_test: bool | None = None
    credentials_rejected: bool = False

    @property
    def status(self) -> InstanceStatus:
        if self.last_test is False or self.credentials_rejected:
            return InstanceStatus.UNAVAILABLE
        if len(self.calls) >= FAILED_IN_A_ROW and not any(self.calls[-FAILED_IN_A_ROW:]):
            return InstanceStatus.UNAVAILABLE
        if not all(self.calls):
            return InstanceStatus.DEGRADED
        return InstanceStatus.HEALTHY

    def with_call(self, worked: bool, *, credentials_rejected: bool = False) -> InstanceRecord:
        return replace(
            self,
            calls=(*self.calls, worked)[-WINDOW:],
            credentials_rejected=self.credentials_rejected or credentials_rejected,
        )

    def with_test(self, worked: bool, *, credentials_rejected: bool = False) -> InstanceRecord:
        """A test that works clears a rejection of the credentials; one that fails keeps it, or marks it."""
        rejected = False if worked else self.credentials_rejected or credentials_rejected
        return replace(self, last_test=worked, credentials_rejected=rejected)
