"""The flow of research R11 (T090, FR-016): one lock per caller and key, and the row of each key."""

import asyncio
from datetime import datetime, timedelta

import pytest

from hermes.application.idempotency import Idempotency, Reservation
from hermes.application.ports import AuditEntry, AuditKind
from hermes.domain.idempotency.record import KeyState, StoredResponse
from hermes.domain.shared.errors import ErrorCode, HermesError
from support.clock import START, FakeClock
from support.doubles import MemoryIdempotency

pytestmark = pytest.mark.anyio

CALLER = "spiffe://cerbero.test/hades"
KEY = "case-1234-notify"
FINGERPRINT = b"\x01" * 32
SENT = StoredResponse(200, {"status": "completed", "data": {"provider_message_ref": "fake-1"}, "error": None})
UNKNOWN = StoredResponse(200, {"status": "failed", "data": {}, "error": {"code": "timeout", "outcome_unknown": True}})


def audit_entry(key: str) -> AuditEntry:
    return AuditEntry(at=START, kind=AuditKind.SEND, actor=CALLER, outcome={"status": "completed"}, idempotency_key=key)


def unknown_entry(key: str = KEY, at: datetime = START) -> AuditEntry:
    """The entry that a reservation keeps, written if the outcome of its send stays unknown (T122)."""
    outcome = {"status": "failed", "code": "timeout", "outcome_unknown": True}
    return AuditEntry(at=at, kind=AuditKind.SEND, actor=CALLER, outcome=outcome, idempotency_key=key)


class Perform:
    """A request that reserves its key, reaches the provider and completes it; it can fail before the provider."""

    def __init__(self, *, fail_before: bool = False, wait: asyncio.Event | None = None) -> None:
        self.calls = 0
        self.fail_before = fail_before
        self.wait = wait

    async def __call__(self, reservation: Reservation) -> StoredResponse:
        self.calls += 1
        if self.fail_before:
            raise HermesError(ErrorCode.NOT_FOUND, "the contact does not exist")
        await reservation.reserve(unknown_entry())
        if self.wait is not None:
            await self.wait.wait()
        await reservation.complete(SENT, audit_entry(KEY))
        return SENT


def flow() -> tuple[Idempotency, MemoryIdempotency, FakeClock]:
    clock = FakeClock()
    repository = MemoryIdempotency()
    return Idempotency(repository, clock), repository, clock


async def run(idempotency: Idempotency, perform: Perform, fingerprint: bytes = FINGERPRINT) -> StoredResponse:
    return await idempotency.run(CALLER, KEY, fingerprint, perform, unknown_outcome=lambda: UNKNOWN)


async def test_a_new_key_runs_the_request_and_keeps_its_response_with_its_audit_entry() -> None:
    idempotency, repository, _ = flow()
    perform = Perform()
    assert await run(idempotency, perform) == SENT
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.COMPLETED, SENT)
    assert [entry.idempotency_key for entry in repository.audit.entries] == [KEY]


async def test_the_same_key_and_body_give_the_same_response_without_running_again() -> None:
    idempotency, _, _ = flow()
    perform = Perform()
    await run(idempotency, perform)
    assert await run(idempotency, perform) == SENT
    assert perform.calls == 1


async def test_the_same_key_with_another_body_is_a_conflict() -> None:
    idempotency, _, _ = flow()
    perform = Perform()
    await run(idempotency, perform)
    with pytest.raises(HermesError) as raised:
        await run(idempotency, perform, fingerprint=b"\x02" * 32)
    assert raised.value.code is ErrorCode.CONFLICT
    assert perform.calls == 1


async def test_a_key_left_in_progress_by_a_cut_answers_the_unknown_outcome_and_completes() -> None:
    idempotency, repository, clock = flow()
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown_entry(at=clock.now()))
    perform = Perform()
    assert await run(idempotency, perform) == UNKNOWN
    assert perform.calls == 0
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.COMPLETED, UNKNOWN)
    assert repository.audit.entries == [unknown_entry(at=clock.now())]
    assert await run(idempotency, perform) == UNKNOWN
    assert len(repository.audit.entries) == 1


async def test_a_request_that_fails_before_the_provider_leaves_no_row() -> None:
    idempotency, repository, _ = flow()
    with pytest.raises(HermesError):
        await run(idempotency, Perform(fail_before=True))
    assert await repository.find(CALLER, KEY) is None
    assert await run(idempotency, Perform()) == SENT


async def test_requests_with_the_same_key_at_once_run_one_by_one_and_only_the_first_reaches_the_provider() -> None:
    idempotency, _, _ = flow()
    release = asyncio.Event()
    perform = Perform(wait=release)
    tasks = [asyncio.create_task(run(idempotency, perform)) for _ in range(10)]
    await asyncio.sleep(0)
    release.set()
    assert await asyncio.gather(*tasks) == [SENT] * 10
    assert perform.calls == 1


async def test_the_lock_of_a_key_goes_away_when_no_request_holds_it() -> None:
    idempotency, _, _ = flow()
    await run(idempotency, Perform())
    with pytest.raises(HermesError):
        await idempotency.run(CALLER, "other", FINGERPRINT, Perform(fail_before=True), unknown_outcome=lambda: UNKNOWN)
    assert idempotency.locks_held() == 0


async def test_the_purge_removes_the_keys_of_more_than_24_hours() -> None:
    idempotency, repository, clock = flow()
    await run(idempotency, Perform())
    clock.advance(timedelta(hours=24, seconds=1))
    assert await idempotency.purge() == 1
    assert await repository.find(CALLER, KEY) is None
    assert repository.audit.entries == [audit_entry(KEY)]


async def test_the_purge_of_a_key_left_in_progress_writes_the_entry_of_its_send() -> None:
    idempotency, repository, clock = flow()
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown_entry(at=clock.now()))
    attempt = clock.now()
    clock.advance(timedelta(hours=24, seconds=1))
    assert await idempotency.purge() == 1
    assert repository.audit.entries == [unknown_entry(at=attempt)]
