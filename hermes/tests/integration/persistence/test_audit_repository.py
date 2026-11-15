"""The audit repository (T033): it writes each kind of entry, and nothing changes or deletes them."""

from datetime import UTC, datetime

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.ports import AuditEntry, AuditKind
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.audit_repository import SqlAuditLog
from hermes.infrastructure.persistence.engine import create_engine

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

AT = datetime(2026, 11, 14, 10, 0, tzinfo=UTC)
ENTRIES = (
    AuditEntry(
        at=AT,
        kind=AuditKind.SEND,
        actor="spiffe://cerbero.test/hades",
        outcome={"status": "completed", "code": None, "outcome_unknown": False},
        function="chat.send_message",
        instance="family_bot",
        recipients=("contacto_demo",),
        side_effect="Envía a contacto_demo el mensaje «Aviso» por la instancia family_bot",
        discloses_data="telegram recibe el texto del mensaje y el chat de contacto_demo",
        audit_context={"source": "test"},
        idempotency_key="case-1234-notify",
    ),
    AuditEntry(at=AT, kind=AuditKind.CONFIG_APPLIED, actor="admin", outcome={"changes": ["+ instance family_bot"]}),
    AuditEntry(
        at=AT,
        kind=AuditKind.VERIFICATION_CODE_ISSUED,
        actor="admin",
        outcome={"contact": "contacto_demo", "instance": "family_bot"},
    ),
    AuditEntry(
        at=AT,
        kind=AuditKind.CHANNEL_VERIFIED,
        actor="admin",
        outcome={"contact": "contacto_demo", "instance": "family_bot"},
    ),
)


async def rows(engine: AsyncEngine) -> list[dict[str, object]]:
    async with engine.connect() as connection:
        result = await connection.execute(text("SELECT * FROM audit_log ORDER BY id"))
        return [dict(row._mapping) for row in result]


async def test_each_kind_of_entry_is_written_as_it_is(migrated: AsyncEngine) -> None:
    audit = SqlAuditLog(migrated)
    for entry in ENTRIES:
        await audit.append(entry)
    written = await rows(migrated)
    assert [row["kind"] for row in written] == [
        "send",
        "config_applied",
        "verification_code_issued",
        "channel_verified",
    ]
    send = written[0]
    assert send["at"] == AT
    assert send["actor"] == "spiffe://cerbero.test/hades"
    assert send["function"] == "chat.send_message"
    assert send["instance"] == "family_bot"
    assert send["recipients"] == ["contacto_demo"]
    assert send["side_effect"] == ENTRIES[0].side_effect
    assert send["discloses_data"] == ENTRIES[0].discloses_data
    assert send["audit_context"] == {"source": "test"}
    assert send["idempotency_key"] == "case-1234-notify"
    assert send["outcome"] == {"status": "completed", "code": None, "outcome_unknown": False}
    applied = written[1]
    assert applied["function"] is None
    assert applied["recipients"] is None
    assert applied["audit_context"] is None
    assert applied["outcome"] == {"changes": ["+ instance family_bot"]}


@pytest.mark.parametrize(
    "statement",
    ["UPDATE audit_log SET actor = 'otro'", "DELETE FROM audit_log", "TRUNCATE audit_log"],
)
async def test_no_entry_can_be_changed_or_deleted(migrated: AsyncEngine, statement: str) -> None:
    await SqlAuditLog(migrated).append(ENTRIES[0])
    with pytest.raises(DBAPIError, match="append-only"):
        async with migrated.begin() as connection:
            await connection.execute(text(statement))
    assert len(await rows(migrated)) == 1


async def test_a_database_that_does_not_answer_is_unavailable() -> None:
    engine = create_engine("postgresql+psycopg://hermes@127.0.0.1:1/hermes", lambda: SecretValue("unused"))
    try:
        with pytest.raises(HermesError) as raised:
            await SqlAuditLog(engine).append(ENTRIES[0])
    finally:
        await engine.dispose()
    assert raised.value.code is ErrorCode.UNAVAILABLE
