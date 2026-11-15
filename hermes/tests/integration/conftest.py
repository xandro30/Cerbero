"""The fixtures of the integration tests.

- A clean schema and an engine on the throwaway PostgreSQL, for the tests that use the database.
- Every answer of the network API that these tests receive follows hermes.v1.json and common.v1.json, also the errors
  (T107, T121, SC-009): a common check around the ASGI transport of httpx, so no test can forget it.
"""

import json
from collections.abc import AsyncIterator, Iterator

import httpx
import psycopg
import pytest
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.engine import create_engine
from hermes.infrastructure.persistence.migrations import apply_migrations
from support.contract import contract
from support.postgres import Database


@pytest.fixture
def fresh_database(postgres: Database) -> Database:
    with psycopg.connect(postgres.admin_conninfo(dbname=postgres.name), autocommit=True) as connection:
        connection.execute("DROP SCHEMA IF EXISTS public CASCADE")
        connection.execute("CREATE SCHEMA public AUTHORIZATION hermes")
    return postgres


@pytest.fixture
async def engine(fresh_database: Database) -> AsyncIterator[AsyncEngine]:
    password = SecretValue(fresh_database.password)
    created = create_engine(fresh_database.url, lambda: password)
    yield created
    await created.dispose()


@pytest.fixture
async def migrated(engine: AsyncEngine) -> AsyncEngine:
    await apply_migrations(engine)
    return engine


@pytest.fixture(autouse=True)
def every_answer_follows_the_contract(monkeypatch: pytest.MonkeyPatch) -> Iterator[list[str]]:
    problems: list[str] = []
    known = contract()
    original = httpx.ASGITransport.handle_async_request

    async def checked(transport: httpx.ASGITransport, request: httpx.Request) -> httpx.Response:
        response = await original(transport, request)
        operation = known.operation_of(request.method, request.url.path)
        if operation is None:
            return response
        content = await response.aread()
        try:
            found = known.check(operation, response.status_code, json.loads(content))
        except KeyError:
            found = ["the contract has no answer with this status"]
        except ValueError:
            found = ["the body is not JSON"]
        where = f"{request.method} {request.url.path} {response.status_code}"
        problems.extend(f"{where}: {problem}" for problem in found)
        return httpx.Response(response.status_code, headers=response.headers, content=content, request=request)

    monkeypatch.setattr(httpx.ASGITransport, "handle_async_request", checked)
    yield problems
    assert not problems, "\n".join(problems)
