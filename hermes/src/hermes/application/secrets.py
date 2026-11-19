"""The cache of the secrets of Hermes (FR-009, research R5): only in memory, with the times that Hécate gives.

During the soft time a value is used without asking; after it, the next use refreshes it. While Hécate is sealed or
does not answer, the value holds until its hard time; after that, what needs it fails with `internal`.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta

from hermes.application.ports import SecretResolver
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue

FALLBACK = frozenset({ErrorCode.UNAVAILABLE, ErrorCode.INTERNAL})
EXPIRED = "the secret is not available: hecate does not answer and the cached value is past its hard time"


@dataclass(frozen=True)
class _Cached:
    value: SecretValue
    resolved_at: datetime
    soft: timedelta
    hard: timedelta


class SecretCache:
    def __init__(self, resolver: SecretResolver, clock: Clock) -> None:
        self._resolver = resolver
        self._clock = clock
        self._values: dict[str, _Cached] = {}

    async def resolve(self, reference: SecretReference) -> SecretValue:
        key = str(reference)
        cached = self._values.get(key)
        now = self._clock.now()
        if cached is not None and now - cached.resolved_at < cached.soft:
            return cached.value
        try:
            [resolved] = await self._resolver.resolve([reference])
        except HermesError as error:
            if error.code not in FALLBACK:
                self._values.pop(key, None)
                raise
            if cached is not None and now - cached.resolved_at < cached.hard:
                return cached.value
            raise HermesError(ErrorCode.INTERNAL, EXPIRED) from None
        self._values[key] = _Cached(
            value=resolved.value,
            resolved_at=now,
            soft=timedelta(seconds=resolved.cache_ttl_soft_s),
            hard=timedelta(seconds=resolved.cache_ttl_hard_s),
        )
        return resolved.value

    def current(self, reference: SecretReference) -> SecretValue:
        """The value in the cache, for what cannot wait for Hécate: a new connection to the database."""
        cached = self._values.get(str(reference))
        if cached is None or self._clock.now() - cached.resolved_at >= cached.hard:
            raise HermesError(ErrorCode.INTERNAL, EXPIRED)
        return cached.value
