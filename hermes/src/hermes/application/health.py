"""The health of Hermes (FR-015, contracts/admin-channel.md): live while the process answers, and ready with a valid
certificate, a copy of its access list, its database and its configuration. When it is not ready, it says the first
reason in the order of the start; it never says anything about instances or contacts."""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from dataclasses import dataclass

from hermes.application.identity import NEEDS_TOKEN

ACCESS_LIST_NOT_LOADED = "access list not loaded"
DATABASE_UNAVAILABLE = "database unavailable"
CONFIGURATION_NOT_LOADED = "configuration not loaded"


@dataclass(frozen=True)
class Health:
    ready: bool
    reason: str | None = None
    live: bool = True


class HealthCheck:
    def __init__(
        self,
        *,
        identity_problem: Callable[[], str | None],
        access_loaded: Callable[[], bool],
        database_answers: Callable[[], Awaitable[bool]],
        configuration_loaded: Callable[[], bool],
    ) -> None:
        self._identity_problem = identity_problem
        self._access_loaded = access_loaded
        self._database_answers = database_answers
        self._configuration_loaded = configuration_loaded

    async def check(self) -> Health:
        if self._identity_problem() is not None:
            return Health(ready=False, reason=NEEDS_TOKEN)
        if not self._access_loaded():
            return Health(ready=False, reason=ACCESS_LIST_NOT_LOADED)
        if not await self._database_answers():
            return Health(ready=False, reason=DATABASE_UNAVAILABLE)
        if not self._configuration_loaded():
            return Health(ready=False, reason=CONFIGURATION_NOT_LOADED)
        return Health(ready=True)
