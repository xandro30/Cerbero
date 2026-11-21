"""The keys of idempotency in their table (T089, T122, research R11; T005 of 012), on the throwaway PostgreSQL."""

from dataclasses import replace
from datetime import timedelta
from typing import Any

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.ports import AuditEntry, AuditKind
from hermes.domain.idempotency.record import KeyState, StoredResponse
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.engine import create_engine
from hermes.infrastructure.persistence.idempotency_repository import SqlIdempotencyRepository
from support.audit_log import audit_rows, refuse_audit_entries
from support.clock import START

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

CALLER = "spiffe://cerbero.test/hades"
KEY = "case-1234-notify"
FINGERPRINT = b"\x01" * 32
RESPONSE = StoredResponse(200, {"status": "completed", "data": {"provider_message_ref": "4521"}, "error": None})
UNKNOWN = StoredResponse(200, {"status": "failed", "data": None, "error": {"code": "timeout", "outcome_unknown": True}})


def entry() -> AuditEntry:
    return AuditEntry(
        at=START,
        kind=AuditKind.SEND,
        actor=CALLER,
        outcome={"status": "completed", "code": None, "outcome_unknown": False},
        idempotency_key=KEY,
    )


def unknown(at: Any = START, key: str = KEY) -> AuditEntry:
    """The entry of a send whose outcome stays unknown, as the reservation keeps it (T122)."""
    return AuditEntry(
        at=at,
        kind=AuditKind.SEND,
        actor=CALLER,
        outcome={"status": "failed", "code": "timeout", "outcome_unknown": True},
        function="chat.send_message",
        instance="family_bot",
        recipients=("contacto_demo",),
        side_effect="Envía el mensaje «Dispositivo desconocido en la red» a contacto_demo por telegram/family_bot",
        discloses_data="El título y los datos del mensaje llegan a telegram",
        audit_context={"source": "test", "playbook": "pb-1"},
        idempotency_key=key,
    )


def failed() -> AuditEntry:
    """The entry of a send that failed without sending anything, which releases its key (012, research R2)."""
    return replace(unknown(), outcome={"status": "failed", "code": "provider_unavailable", "outcome_unknown": False})


def as_entry(row: Any) -> AuditEntry:
    return AuditEntry(
        at=row.at,
        kind=AuditKind(row.kind),
        actor=row.actor,
        outcome=row.outcome,
        function=row.function,
        instance=row.instance,
        recipients=tuple(row.recipients) if row.recipients is not None else None,
        side_effect=row.side_effect,
        discloses_data=row.discloses_data,
        audit_context=row.audit_context,
        idempotency_key=row.idempotency_key,
    )


async def rows(engine: AsyncEngine, query: str) -> list[Any]:
    async with engine.connect() as connection:
        return list(await connection.execute(text(query)))


async def test_a_reserved_key_is_in_progress_without_a_response_and_keeps_the_entry_of_its_send(
    migrated: AsyncEngine,
) -> None:
    repository = SqlIdempotencyRepository(migrated)
    assert await repository.find(CALLER, KEY) is None
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.fingerprint, record.state, record.response) == (FINGERPRINT, KeyState.IN_PROGRESS, None)
    [row] = await rows(migrated, "SELECT created_at, pending_audit FROM idempotency_keys")
    assert row.created_at == START
    assert row.pending_audit is not None
    assert await audit_rows(migrated) == []


async def test_a_completed_key_keeps_the_response_and_its_status_with_its_audit_entry(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await repository.complete(CALLER, KEY, RESPONSE, entry())
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.COMPLETED, RESPONSE)
    assert [as_entry(row) for row in await audit_rows(migrated)] == [entry()]
    [row] = await rows(migrated, "SELECT pending_audit FROM idempotency_keys")
    assert row.pending_audit is None


async def test_the_response_and_its_audit_entry_go_in_one_transaction(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    refused = replace(entry(), kind=type("Kind", (), {"value": "bogus"})())
    with pytest.raises(Exception):  # noqa: B017  # the CHECK of the audit table refuses it
        await repository.complete(CALLER, KEY, RESPONSE, refused)
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert record.state is KeyState.IN_PROGRESS
    assert await audit_rows(migrated) == []


async def test_a_key_left_in_progress_completes_with_the_unknown_outcome_and_the_entry_of_its_send(
    migrated: AsyncEngine,
) -> None:
    """A cut left the send without its record: the repetition completes the key and writes the entry that its
    reservation kept, with the time of the attempt (T122, FR-029)."""
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await repository.complete_unknown(CALLER, KEY, UNKNOWN)
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.COMPLETED, UNKNOWN)
    assert [as_entry(row) for row in await audit_rows(migrated)] == [unknown()]
    [row] = await rows(migrated, "SELECT pending_audit FROM idempotency_keys")
    assert row.pending_audit is None


async def test_an_unknown_outcome_whose_entry_cannot_be_written_leaves_the_key_in_progress(
    migrated: AsyncEngine,
) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await refuse_audit_entries(migrated)
    with pytest.raises(DBAPIError):
        await repository.complete_unknown(CALLER, KEY, UNKNOWN)
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.IN_PROGRESS, None)


async def test_a_released_key_goes_away_with_the_entry_of_its_attempt_in_one_transaction(migrated: AsyncEngine) -> None:
    """A failure that guarantees nothing was sent keeps nothing, and its attempt stays in the audit log (012, R2)."""
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await repository.release(CALLER, KEY, failed())
    assert await repository.find(CALLER, KEY) is None
    assert await rows(migrated, "SELECT key FROM idempotency_keys") == []
    assert [as_entry(row) for row in await audit_rows(migrated)] == [failed()]


async def test_a_release_whose_entry_cannot_be_written_leaves_the_key_in_progress(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await refuse_audit_entries(migrated)
    with pytest.raises(DBAPIError):
        await repository.release(CALLER, KEY, failed())
    record = await repository.find(CALLER, KEY)
    assert record is not None
    assert (record.state, record.response) == (KeyState.IN_PROGRESS, None)


async def test_a_release_leaves_the_same_key_of_another_caller(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    other = "spiffe://cerbero.test/tenaro"
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    await repository.reserve(other, KEY, b"\x02" * 32, unknown())
    await repository.release(CALLER, KEY, failed())
    assert await repository.find(CALLER, KEY) is None
    kept = await repository.find(other, KEY)
    assert kept is not None
    assert kept.state is KeyState.IN_PROGRESS


async def test_the_key_belongs_to_its_caller(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    assert await repository.find("spiffe://cerbero.test/tenaro", KEY) is None
    await repository.reserve("spiffe://cerbero.test/tenaro", KEY, b"\x02" * 32, unknown())
    other = await repository.find("spiffe://cerbero.test/tenaro", KEY)
    assert other is not None
    assert other.fingerprint == b"\x02" * 32


async def test_reserving_a_key_twice_is_a_conflict(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    with pytest.raises(HermesError) as raised:
        await repository.reserve(CALLER, KEY, FINGERPRINT, unknown())
    assert raised.value.code is ErrorCode.CONFLICT


async def test_the_purge_removes_the_keys_older_than_its_limit(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    old = START - timedelta(hours=24, seconds=1)
    await repository.reserve(CALLER, "old", FINGERPRINT, unknown(old, "old"))
    await repository.complete(CALLER, "old", RESPONSE, replace(entry(), at=old, idempotency_key="old"))
    await repository.reserve(CALLER, "recent", FINGERPRINT, unknown(START - timedelta(hours=23), "recent"))
    assert await repository.purge(START - timedelta(hours=24)) == 1
    assert await repository.find(CALLER, "old") is None
    assert await repository.find(CALLER, "recent") is not None
    assert [row.idempotency_key for row in await audit_rows(migrated)] == ["old"]


async def test_the_purge_writes_the_entry_of_a_key_still_in_progress_before_it_removes_it(
    migrated: AsyncEngine,
) -> None:
    """Nobody repeated the request after its cut: the entry of its send stays in the audit log (T122, FR-029)."""
    repository = SqlIdempotencyRepository(migrated)
    old = START - timedelta(hours=24, seconds=1)
    await repository.reserve(CALLER, "cut", FINGERPRINT, unknown(old, "cut"))
    assert await repository.purge(START - timedelta(hours=24)) == 1
    assert await repository.find(CALLER, "cut") is None
    assert [as_entry(row) for row in await audit_rows(migrated)] == [unknown(old, "cut")]


async def test_a_purge_whose_entry_cannot_be_written_removes_nothing(migrated: AsyncEngine) -> None:
    repository = SqlIdempotencyRepository(migrated)
    await repository.reserve(CALLER, "cut", FINGERPRINT, unknown(START - timedelta(hours=25), "cut"))
    await refuse_audit_entries(migrated)
    with pytest.raises(DBAPIError):
        await repository.purge(START - timedelta(hours=24))
    assert await repository.find(CALLER, "cut") is not None


async def test_a_database_that_does_not_answer_is_unavailable() -> None:
    engine = create_engine("postgresql+psycopg://hermes@127.0.0.1:1/hermes", lambda: SecretValue("unused"))
    repository = SqlIdempotencyRepository(engine)
    try:
        for call in (
            repository.find(CALLER, KEY),
            repository.reserve(CALLER, KEY, FINGERPRINT, unknown()),
            repository.complete(CALLER, KEY, RESPONSE, entry()),
            repository.complete_unknown(CALLER, KEY, UNKNOWN),
            repository.release(CALLER, KEY, failed()),
            repository.purge(START),
        ):
            with pytest.raises(HermesError) as raised:
                await call
            assert raised.value.code is ErrorCode.UNAVAILABLE
    finally:
        await engine.dispose()
