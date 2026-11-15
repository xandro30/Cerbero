"""The repository of used call tokens (T045): each `(issuer, jti)` once, also after reopening, until it expires."""

import asyncio
from datetime import UTC, datetime, timedelta

import pytest
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.engine import create_engine
from hermes.infrastructure.persistence.tokens_repository import SqlUsedTokens
from support.postgres import Database

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

AT = datetime(2026, 11, 14, 10, 5, 30, tzinfo=UTC)
ISSUER = "spiffe://cerbero.test/hades"


async def test_an_issuer_and_jti_are_kept_once(migrated: AsyncEngine) -> None:
    tokens = SqlUsedTokens(migrated)
    assert await tokens.remember(ISSUER, "call-1", AT) is True
    assert await tokens.remember(ISSUER, "call-1", AT) is False
    assert await tokens.remember("spiffe://cerbero.test/otro", "call-1", AT) is True
    assert await tokens.remember(ISSUER, "call-2", AT) is True


async def test_a_used_jti_is_refused_after_reopening_the_connection(
    migrated: AsyncEngine, fresh_database: Database
) -> None:
    await SqlUsedTokens(migrated).remember(ISSUER, "call-1", AT)
    await migrated.dispose()
    password = SecretValue(fresh_database.password)
    reopened = create_engine(fresh_database.url, lambda: password)
    try:
        assert await SqlUsedTokens(reopened).remember(ISSUER, "call-1", AT) is False
    finally:
        await reopened.dispose()


async def test_the_purge_deletes_the_rows_past_their_expiry(migrated: AsyncEngine) -> None:
    tokens = SqlUsedTokens(migrated)
    await tokens.remember(ISSUER, "old", AT)
    await tokens.remember(ISSUER, "new", AT + timedelta(minutes=10))
    assert await tokens.purge(AT + timedelta(seconds=1)) == 1
    assert await tokens.remember(ISSUER, "old", AT) is True
    assert await tokens.remember(ISSUER, "new", AT) is False


async def test_the_same_jti_at_once_is_accepted_only_once(migrated: AsyncEngine) -> None:
    tokens = SqlUsedTokens(migrated)
    results = await asyncio.gather(*(tokens.remember(ISSUER, "same", AT) for _ in range(10)))
    assert results.count(True) == 1
