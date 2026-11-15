"""The health of Hermes through the local channel (T101, contracts/admin-channel.md, cli.md, FR-015): whether the
process answers and whether it is ready, with the reason when it is not; the network API has no route for it."""

from __future__ import annotations

import asyncio
import io
from collections.abc import AsyncIterator
from pathlib import Path
from typing import Any

import anyio
import httpx
import pytest
from fastapi.routing import APIRoute, iter_route_contexts

from hermes.application.health import HealthCheck
from hermes.infrastructure.admin import cli
from hermes.infrastructure.admin.app import AdminServices, create_admin_app, install_admin
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.servers import EmbeddedServer, server_config
from hermes.infrastructure.sockets import bind_unix_socket, prepare_private_dir
from support.contract import contract
from support.paths import short_dir

pytestmark = pytest.mark.anyio

REASONS = ["needs a new enrollment token", "database unavailable", "configuration not loaded"]


class Situation:
    """What the health looks at, moved by each test."""

    def __init__(self) -> None:
        self.identity: str | None = None
        self.database = True
        self.configuration = True

    def check(self) -> HealthCheck:
        async def database() -> bool:
            return self.database

        return HealthCheck(
            identity_problem=lambda: self.identity,
            database_answers=database,
            configuration_loaded=lambda: self.configuration,
        )

    def lose(self, reason: str) -> None:
        if reason == REASONS[0]:
            self.identity = reason
        elif reason == REASONS[1]:
            self.database = False
        else:
            self.configuration = False


class NoIdentity:
    async def enroll(self, token: object) -> Any:
        raise AssertionError("not used")


@pytest.fixture
def situation() -> Situation:
    return Situation()


@pytest.fixture
async def admin_dir(situation: Situation) -> AsyncIterator[Path]:
    with short_dir() as base:
        directory = base / "admin"
        prepare_private_dir(directory)
        sock = bind_unix_socket(directory / "admin.sock")
        app = create_admin_app()
        install_admin(app, AdminServices(identity=NoIdentity(), health=situation.check()))
        server = EmbeddedServer(server_config(app))
        task = asyncio.create_task(server.serve(sockets=[sock]))
        yield directory
        server.should_exit = True
        await task
        sock.close()


async def run(admin_dir: Path, args: list[str]) -> int:
    env = {"HERMES_ADMIN_DIR": str(admin_dir)}
    return await anyio.to_thread.run_sync(lambda: cli.main(args, env=env, stdin=io.StringIO("")))


async def health(admin_dir: Path) -> dict[str, Any]:
    transport = httpx.AsyncHTTPTransport(uds=str(admin_dir / "admin.sock"))
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as http:
        body: dict[str, Any] = (await http.get("/health")).json()
    known = contract()
    assert not known.errors(known.common_uri("Completed"), body)
    return body


async def test_a_ready_hermes_says_it_is_live_and_ready(admin_dir: Path, capsys: pytest.CaptureFixture[str]) -> None:
    assert (await health(admin_dir))["data"] == {"live": True, "ready": True}
    assert await run(admin_dir, ["health"]) == 0
    assert capsys.readouterr().out == "ready\n"


@pytest.mark.parametrize("reason", REASONS)
async def test_a_hermes_that_is_not_ready_says_why_and_exits_with_1(
    admin_dir: Path, situation: Situation, reason: str, capsys: pytest.CaptureFixture[str]
) -> None:
    situation.lose(reason)
    assert (await health(admin_dir))["data"] == {"live": True, "ready": False, "reason": reason}
    assert await run(admin_dir, ["health"]) == 1
    assert capsys.readouterr().out == f"not ready: {reason}\n"


async def test_the_first_reason_in_the_order_of_the_start_is_the_one_it_says(
    admin_dir: Path, situation: Situation
) -> None:
    for reason in REASONS:
        situation.lose(reason)
    assert (await health(admin_dir))["data"]["reason"] == REASONS[0]
    situation.identity = None
    assert (await health(admin_dir))["data"]["reason"] == REASONS[1]


async def test_live_exits_with_0_while_the_process_answers_ready_or_not(
    admin_dir: Path, situation: Situation, capsys: pytest.CaptureFixture[str]
) -> None:
    situation.lose(REASONS[1])
    assert await run(admin_dir, ["health", "--live"]) == 0
    assert capsys.readouterr().out == "live\n"


async def test_without_the_process_health_exits_with_1(capsys: pytest.CaptureFixture[str]) -> None:
    with short_dir() as base:
        directory = base / "admin"
        prepare_private_dir(directory)
        for args in (["health"], ["health", "--live"]):
            assert await run(directory, args) == 1
            assert capsys.readouterr().err.startswith("error: ")


def test_the_network_api_has_no_route_of_health() -> None:
    paths = {
        str(context.path)
        for context in iter_route_contexts(create_network_api().routes)
        if isinstance(context.original_route, APIRoute)
    }
    assert paths == {"/chat/v1/send_message", "/v1/capabilities", "/v1/instances/{name}/test"}
