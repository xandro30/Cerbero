"""The channels in their table (T085, data-model.md#channels): the code, the binding and the verified state."""

from __future__ import annotations

import hashlib
from datetime import timedelta

import pytest
from sqlalchemy.exc import DBAPIError
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.ports import AuditEntry, AuditKind, ChannelKey
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.channels_repository import SqlChannelRepository
from hermes.infrastructure.persistence.config_repository import SqlConfigRepository
from hermes.infrastructure.persistence.engine import create_engine
from support.audit_log import audit_rows, refuse_audit_entries
from support.clock import START
from support.configuration import CHAT, configuration

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

KEY = ChannelKey("contacto_demo", "chat", "telegram", "family_bot")
CODE = hashlib.sha256(b"ABCD2345").digest()
EXPIRES = START + timedelta(minutes=30)
WHO = {"contact": "contacto_demo", "instance": "family_bot"}
ISSUED = AuditEntry(at=START, kind=AuditKind.VERIFICATION_CODE_ISSUED, actor="admin", outcome=WHO)
VERIFIED = AuditEntry(at=START, kind=AuditKind.CHANNEL_VERIFIED, actor="admin", outcome=WHO)


@pytest.fixture
async def channels(migrated: AsyncEngine) -> SqlChannelRepository:
    await SqlConfigRepository(migrated).replace(configuration())
    return SqlChannelRepository(migrated)


async def test_a_new_channel_has_no_address_verification_or_code(channels: SqlChannelRepository) -> None:
    state = await channels.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash, state.code_expires_at) == (None, None, None, None)
    assert await channels.state(ChannelKey("nadie", "chat", "telegram", "family_bot")) is None


async def test_a_code_leaves_the_channel_unverified_and_pending(channels: SqlChannelRepository) -> None:
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    assert await channels.bind(KEY, CODE, CHAT, START, VERIFIED) is True
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    state = await channels.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash, state.code_expires_at) == (None, None, CODE, EXPIRES)
    assert [pending.key for pending in await channels.pending("family_bot")] == [KEY]
    assert await channels.pending("test_fake") == []


async def test_the_binding_needs_the_pending_code_before_it_expires(channels: SqlChannelRepository) -> None:
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    assert await channels.bind(KEY, hashlib.sha256(b"OTHER123").digest(), CHAT, START, VERIFIED) is False
    assert await channels.bind(KEY, CODE, CHAT, EXPIRES, VERIFIED) is False
    assert await channels.bind(KEY, CODE, CHAT, START, VERIFIED) is True
    state = await channels.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash) == (CHAT, START, None)
    assert await channels.bind(KEY, CODE, "otro", START, VERIFIED) is False


async def test_a_code_for_a_channel_that_does_not_exist_is_not_found(channels: SqlChannelRepository) -> None:
    with pytest.raises(HermesError) as raised:
        await channels.issue_code(ChannelKey("nadie", "chat", "telegram", "family_bot"), CODE, EXPIRES, ISSUED)
    assert raised.value.code is ErrorCode.NOT_FOUND


async def test_each_code_and_each_binding_leave_their_entry_with_the_change(
    channels: SqlChannelRepository, migrated: AsyncEngine
) -> None:
    """The entry goes in the transaction of the change (FR-029); a binding that does not happen leaves none, and
    neither does a code for a channel that does not exist."""
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    assert await channels.bind(KEY, hashlib.sha256(b"OTHER123").digest(), CHAT, START, VERIFIED) is False
    assert await channels.bind(KEY, CODE, CHAT, START, VERIFIED) is True
    with pytest.raises(HermesError):
        await channels.issue_code(ChannelKey("nadie", "chat", "telegram", "family_bot"), CODE, EXPIRES, ISSUED)
    assert [(row.kind, row.actor, row.outcome) for row in await audit_rows(migrated)] == [
        ("verification_code_issued", "admin", WHO),
        ("channel_verified", "admin", WHO),
    ]


async def test_a_binding_whose_entry_cannot_be_written_leaves_the_channel_pending(
    channels: SqlChannelRepository, migrated: AsyncEngine
) -> None:
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    await refuse_audit_entries(migrated)
    with pytest.raises(DBAPIError):
        await channels.bind(KEY, CODE, CHAT, START, VERIFIED)
    state = await channels.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash, state.code_expires_at) == (None, None, CODE, EXPIRES)


async def test_a_code_whose_entry_cannot_be_written_leaves_the_channel_verified(
    channels: SqlChannelRepository, migrated: AsyncEngine
) -> None:
    await channels.issue_code(KEY, CODE, EXPIRES, ISSUED)
    assert await channels.bind(KEY, CODE, CHAT, START, VERIFIED) is True
    await refuse_audit_entries(migrated)
    with pytest.raises(DBAPIError):
        await channels.issue_code(KEY, hashlib.sha256(b"WXYZ6789").digest(), EXPIRES, ISSUED)
    state = await channels.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash, state.code_expires_at) == (CHAT, START, None, None)


async def test_a_database_that_does_not_answer_is_unavailable() -> None:
    engine = create_engine("postgresql+psycopg://hermes@127.0.0.1:1/hermes", lambda: SecretValue("unused"))
    try:
        with pytest.raises(HermesError) as raised:
            await SqlChannelRepository(engine).state(KEY)
    finally:
        await engine.dispose()
    assert raised.value.code is ErrorCode.UNAVAILABLE
