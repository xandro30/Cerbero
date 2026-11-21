"""The idempotency of the requests with effect (research R11, FR-016).

1. One lock in memory per caller and key makes the requests with the same key that arrive at once go one by one.
2. With a row: another fingerprint is `conflict`; `completed` gives back its response without running the request;
   `in_progress`, left by a cut, answers that the outcome is unknown, and the row keeps that answer together with the
   audit entry of its send, which its reservation kept (FR-029).
3. Without a row: the request runs, and only right before the provider it reserves its key with the audit entry of its
   send if its outcome stays unknown; with what was sent or may have been sent, the row is completed in the same
   transaction as its audit entry, and with a failure that guarantees nothing was sent, it is released with it
   (research R2 of 012). A request that fails before the provider leaves no row.
"""

from __future__ import annotations

import asyncio
import contextlib
from collections.abc import AsyncIterator, Awaitable, Callable

from hermes.application.ports import AuditEntry, IdempotencyRepository
from hermes.domain.idempotency.record import RETENTION, KeyState, StoredResponse
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError

KEY_REUSED = "the Idempotency-Key was already used with another request"


class KeyLocks:
    """One lock per key while a request holds it or waits for it; with no one left, the lock goes away."""

    def __init__(self) -> None:
        self._locks: dict[tuple[str, str], tuple[asyncio.Lock, int]] = {}

    def __len__(self) -> int:
        return len(self._locks)

    @contextlib.asynccontextmanager
    async def hold(self, key: tuple[str, str]) -> AsyncIterator[None]:
        lock, users = self._locks.get(key, (asyncio.Lock(), 0))
        self._locks[key] = (lock, users + 1)
        try:
            async with lock:
                yield
        finally:
            lock, users = self._locks[key]
            if users == 1:
                del self._locks[key]
            else:
                self._locks[key] = (lock, users - 1)


class Reservation:
    """The row of a new key: reserved right before the provider; completed with what was sent or may have been sent,
    with its response and its audit entry, or released with the audit entry of a failure without send."""

    def __init__(self, repository: IdempotencyRepository, caller: str, key: str, fingerprint: bytes) -> None:
        self._repository = repository
        self._caller = caller
        self._key = key
        self._fingerprint = fingerprint
        self.response: StoredResponse | None = None

    async def reserve(self, unknown: AuditEntry) -> None:
        """The key, from the time of the attempt (`unknown.at`), with the entry to write if the outcome stays
        unknown."""
        await self._repository.reserve(self._caller, self._key, self._fingerprint, unknown)

    async def complete(self, response: StoredResponse, audit: AuditEntry) -> None:
        await self._repository.complete(self._caller, self._key, response, audit)
        self.response = response

    async def release(self, audit: AuditEntry) -> None:
        """The key goes away with the audit entry of an attempt that failed without sending anything: the next request
        with it is a new one, and `response` stays empty."""
        await self._repository.release(self._caller, self._key, audit)


class Idempotency:
    def __init__(self, repository: IdempotencyRepository, clock: Clock) -> None:
        self._repository = repository
        self._clock = clock
        self._locks = KeyLocks()

    def locks_held(self) -> int:
        return len(self._locks)

    async def run(
        self,
        caller: str,
        key: str,
        fingerprint: bytes,
        perform: Callable[[Reservation], Awaitable[StoredResponse]],
        *,
        unknown_outcome: Callable[[], StoredResponse],
    ) -> StoredResponse:
        """The response of the request: the one of its key if it has one, or the one that `perform` gives."""
        async with self._locks.hold((caller, key)):
            record = await self._repository.find(caller, key)
            if record is None:
                return await perform(Reservation(self._repository, caller, key, fingerprint))
            if record.fingerprint != fingerprint:
                raise HermesError(ErrorCode.CONFLICT, KEY_REUSED)
            if record.state is KeyState.COMPLETED and record.response is not None:
                return record.response
            unknown = unknown_outcome()
            await self._repository.complete_unknown(caller, key, unknown)
            return unknown

    async def purge(self) -> int:
        """The keys of more than 24 hours go away (spec, Assumptions)."""
        return await self._repository.purge(self._clock.now() - RETENTION)
