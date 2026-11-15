"""The skeleton of the local channel (T014): private socket, common envelope and the exit codes of the CLI."""

import argparse
import asyncio
import stat
from collections.abc import AsyncIterator
from pathlib import Path

import anyio
import httpx
import pytest
from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.admin import cli
from hermes.infrastructure.admin.app import create_admin_app
from hermes.infrastructure.api.envelope import completed_response
from hermes.infrastructure.servers import EmbeddedServer, server_config
from hermes.infrastructure.sockets import bind_unix_socket, prepare_private_dir
from support.contract import contract
from support.paths import short_dir

pytestmark = pytest.mark.anyio

router = APIRouter()


@router.get("/ping")
async def ping(request: Request) -> JSONResponse:
    return completed_response(request, {"pong": True})


@router.get("/out-of-envelope/{via}")
async def out_of_envelope(via: str) -> JSONResponse:
    return JSONResponse({"pong": via})


@router.get("/fail")
async def fail() -> JSONResponse:
    raise HermesError(ErrorCode.CONFLICT, "the channel is not verified")


@pytest.fixture
async def admin_dir() -> AsyncIterator[Path]:
    with short_dir() as base:
        directory = base / "admin"
        prepare_private_dir(directory)
        sock = bind_unix_socket(directory / "admin.sock")
        server = EmbeddedServer(server_config(create_admin_app(routers=[router])))
        task = asyncio.create_task(server.serve(sockets=[sock]))
        # The socket listens from bind_unix_socket, so a request waits in its queue until uvicorn takes it.
        yield directory
        server.should_exit = True
        await task
        sock.close()


def commands() -> dict[str, cli.Command]:
    def configure(_: argparse.ArgumentParser) -> None:
        return None

    def ping(_: argparse.Namespace, context: cli.Context) -> int:
        data = context.call("GET", "/ping")
        context.out.write(f"pong {data['pong']}\n")
        return 0

    def fail(_: argparse.Namespace, context: cli.Context) -> int:
        context.call("GET", "/fail")
        return 0

    return {
        **cli.COMMANDS,
        "ping": cli.Command("ping", "Test order", configure, ping),
        "fail": cli.Command("fail", "Test order", configure, fail),
    }


async def test_the_directory_is_0700_and_the_socket_0600(admin_dir: Path) -> None:
    assert stat.S_IMODE((await anyio.Path(admin_dir).stat()).st_mode) == 0o700
    assert stat.S_IMODE((await anyio.Path(admin_dir / "admin.sock").stat()).st_mode) == 0o600


async def test_the_answers_use_the_common_envelope(admin_dir: Path) -> None:
    transport = httpx.AsyncHTTPTransport(uds=str(admin_dir / "admin.sock"))
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as http:
        done = (await http.get("/ping")).json()
        failed = await http.get("/fail")
    known = contract()
    assert not known.errors(known.common_uri("Completed"), done)
    assert failed.status_code == 409
    assert not known.errors(known.common_uri("Failed"), failed.json())


async def test_the_common_check_sees_an_answer_out_of_the_envelope(
    admin_dir: Path, every_admin_answer_uses_the_common_envelope: list[str]
) -> None:
    """The check of tests/integration/admin/conftest.py is not empty (T130): it sees an answer out of the envelope
    through the async transport and through the sync one of the CLI, and nothing in one that follows it."""
    socket = str(admin_dir / "admin.sock")
    async with httpx.AsyncClient(transport=httpx.AsyncHTTPTransport(uds=socket), base_url="http://hermes") as http:
        await http.get("/out-of-envelope/async")
        await http.get("/ping")

    def from_the_cli() -> None:
        with httpx.Client(transport=httpx.HTTPTransport(uds=socket), base_url="http://hermes") as client:
            client.get("/out-of-envelope/sync")

    await anyio.to_thread.run_sync(from_the_cli)
    seen = list(every_admin_answer_uses_the_common_envelope)
    every_admin_answer_uses_the_common_envelope.clear()
    assert {problem.split(": ")[0] for problem in seen} == {
        "GET /out-of-envelope/async 200",
        "GET /out-of-envelope/sync 200",
    }


async def test_the_cli_prints_the_result_or_the_error(admin_dir: Path, capsys: pytest.CaptureFixture[str]) -> None:
    env = {"HERMES_ADMIN_DIR": str(admin_dir)}
    assert await anyio.to_thread.run_sync(lambda: cli.main(["ping"], env=env, commands=commands())) == 0
    assert capsys.readouterr().out == "pong True\n"
    assert await anyio.to_thread.run_sync(lambda: cli.main(["fail"], env=env, commands=commands())) == 1
    assert capsys.readouterr().err == "error: conflict: the channel is not verified\n"


async def test_without_a_socket_the_cli_cannot_reach_hermes(capsys: pytest.CaptureFixture[str]) -> None:
    with short_dir() as base:
        env = {"HERMES_ADMIN_DIR": str(base / "missing")}
        assert cli.main(["ping"], env=env, commands=commands()) == 1
    assert capsys.readouterr().err.startswith(f"error: cannot reach hermes at {base}/missing/admin.sock: ")


async def test_a_wrong_usage_exits_with_2() -> None:
    with pytest.raises(SystemExit) as raised:
        cli.main(["no-such-order"], env={})
    assert raised.value.code == 2
