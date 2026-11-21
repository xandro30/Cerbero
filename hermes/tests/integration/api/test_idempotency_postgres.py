"""The send with its Idempotency-Key on the throwaway PostgreSQL (T015 and T019 of 012, research R8): a failure without
send releases the row of its key in the same transaction as its audit entry, and the repetition with the same key
sends; an unknown outcome stays in the row, and its repetition does not send again."""

from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest
import respx
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.infrastructure.persistence.idempotency_repository import SqlIdempotencyRepository
from hermes.infrastructure.providers.fake.adapter import FakeFailure
from support.audit_log import audit_rows
from support.network_api import NetworkApi, send
from support.telegram import API

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

KEY = "case-1234-notify"


@pytest.fixture
async def world(migrated: AsyncEngine) -> AsyncIterator[tuple[NetworkApi, httpx.AsyncClient]]:
    async with httpx.AsyncClient() as http:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            api = NetworkApi(http, router, keys=SqlIdempotencyRepository(migrated))
            transport = httpx.ASGITransport(app=api.app)
            async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as client:
                yield api, client


async def key_states(engine: AsyncEngine) -> list[str]:
    async with engine.connect() as connection:
        return [row.state for row in await connection.execute(text("SELECT state FROM idempotency_keys"))]


async def test_a_failure_without_send_releases_the_row_and_the_repetition_with_the_same_key_sends(
    migrated: AsyncEngine, world: tuple[NetworkApi, httpx.AsyncClient]
) -> None:
    api, client = world
    api.fake.fail_next(FakeFailure.UNREACHABLE, times=20)
    first = await send(client, KEY)
    assert first.json()["error"]["code"] == "provider_unavailable"
    assert await key_states(migrated) == []
    rows: list[Any] = await audit_rows(migrated)
    assert [row.outcome["code"] for row in rows] == ["provider_unavailable"]
    api.fake.clear_failures()
    again = await send(client, KEY)
    assert again.json()["status"] == "completed"
    assert await key_states(migrated) == ["completed"]
    assert [row.outcome["status"] for row in await audit_rows(migrated)] == ["failed", "completed"]
    assert len(api.fake.sent()) == 1


async def test_an_unknown_outcome_stays_in_the_row_and_its_repetition_does_not_send_again(
    migrated: AsyncEngine, world: tuple[NetworkApi, httpx.AsyncClient]
) -> None:
    """Story 3 on PostgreSQL: what may have been sent is never sent again with its key."""
    api, client = world
    api.fake.fail_next(FakeFailure.CUT_AFTER_SEND)
    first = await send(client, KEY)
    error = first.json()["error"]
    assert (error["code"], error["outcome_unknown"]) == ("timeout", True)
    assert await key_states(migrated) == ["completed"]
    again = await send(client, KEY)
    assert again.json()["error"] == error
    assert len(api.fake.sent()) == 1
    assert [row.outcome["code"] for row in await audit_rows(migrated)] == ["timeout"]
