"""The enrollment through the local channel and `hermes enroll` (T061, contracts/admin-channel.md and cli.md)."""

from __future__ import annotations

import asyncio
import getpass
import io
import logging
from collections.abc import AsyncIterator
from datetime import UTC, datetime
from pathlib import Path

import anyio
import pytest

from hermes.application.ports import IssuedCertificate
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.admin import cli
from hermes.infrastructure.admin.app import AdminServices, create_admin_app, install_admin
from hermes.infrastructure.servers import EmbeddedServer, server_config
from hermes.infrastructure.sockets import bind_unix_socket, prepare_private_dir
from support.paths import short_dir
from support.pki import TestCa, spiffe

pytestmark = pytest.mark.anyio

TOKEN = TestCa().enrollment_token()
ISSUED = IssuedCertificate(
    spiffe_id=SpiffeId.parse(spiffe("hermes")),
    serial="1a2b3c",
    certificate_pem="-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----\n",
    chain_pem=("intermediate", "root"),
    not_after=datetime(2026, 11, 17, 10, 0, tzinfo=UTC),
    call_key_id="k" * 43,
)


class FakeIdentity:
    def __init__(self) -> None:
        self.tokens: list[str] = []
        self.outcome: IssuedCertificate | HermesError = ISSUED

    async def enroll(self, token: SecretValue) -> IssuedCertificate:
        self.tokens.append(token.reveal())
        if isinstance(self.outcome, HermesError):
            raise self.outcome
        return self.outcome


class Tty(io.StringIO):
    def isatty(self) -> bool:
        return True


@pytest.fixture
def identity() -> FakeIdentity:
    return FakeIdentity()


@pytest.fixture
async def admin_dir(identity: FakeIdentity) -> AsyncIterator[Path]:
    with short_dir() as base:
        directory = base / "admin"
        prepare_private_dir(directory)
        sock = bind_unix_socket(directory / "admin.sock")
        app = create_admin_app()
        install_admin(app, AdminServices(identity=identity))
        server = EmbeddedServer(server_config(app))
        task = asyncio.create_task(server.serve(sockets=[sock]))
        yield directory
        server.should_exit = True
        await task
        sock.close()


async def run(admin_dir: Path, args: list[str], stdin: io.StringIO) -> int:
    env = {"HERMES_ADMIN_DIR": str(admin_dir)}
    return await anyio.to_thread.run_sync(lambda: cli.main(args, env=env, stdin=stdin))


async def test_from_a_terminal_the_token_is_asked_without_echo(
    admin_dir: Path, identity: FakeIdentity, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    prompts: list[str] = []

    def ask(prompt: str = "", stream: object = None) -> str:
        prompts.append(prompt)
        return TOKEN

    monkeypatch.setattr(getpass, "getpass", ask)
    assert await run(admin_dir, ["enroll"], Tty()) == 0
    assert prompts == ["Enrollment token: "]
    assert identity.tokens == [TOKEN]
    out = capsys.readouterr()
    assert out.out == "enrolled spiffe://cerbero.test/hermes serial 1a2b3c until 2026-11-17T10:00:00Z\n"
    assert TOKEN not in out.out + out.err


async def test_without_a_terminal_the_token_is_read_from_standard_input(
    admin_dir: Path, identity: FakeIdentity, capsys: pytest.CaptureFixture[str]
) -> None:
    assert await run(admin_dir, ["enroll"], io.StringIO(TOKEN + "\n")) == 0
    assert identity.tokens == [TOKEN]
    assert capsys.readouterr().out.startswith("enrolled spiffe://cerbero.test/hermes")


async def test_the_token_is_never_an_argument(admin_dir: Path, identity: FakeIdentity) -> None:
    with pytest.raises(SystemExit) as raised:
        await run(admin_dir, ["enroll", TOKEN], io.StringIO(""))
    assert raised.value.code == 2
    assert identity.tokens == []


async def test_without_a_token_it_is_cancelled(
    admin_dir: Path, identity: FakeIdentity, capsys: pytest.CaptureFixture[str]
) -> None:
    assert await run(admin_dir, ["enroll"], io.StringIO("")) == 1
    assert identity.tokens == []
    assert capsys.readouterr().err == "cancelled\n"


@pytest.mark.parametrize(
    ("outcome", "code"),
    [
        (HermesError(ErrorCode.CONFLICT, "hermes already has a valid identity"), "conflict"),
        (HermesError(ErrorCode.AUTH, "hecate does not have the fingerprint of the token"), "auth"),
        (HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer"), "unavailable"),
    ],
)
async def test_each_refusal_is_written_with_its_code(
    admin_dir: Path, identity: FakeIdentity, capsys: pytest.CaptureFixture[str], outcome: HermesError, code: str
) -> None:
    identity.outcome = outcome
    assert await run(admin_dir, ["enroll"], io.StringIO(TOKEN + "\n")) == 1
    err = capsys.readouterr().err
    assert err == f"error: {code}: {outcome.message}\n"
    assert TOKEN not in err


async def test_a_token_without_the_format_of_hecate_is_validation_before_reaching_the_identity(
    admin_dir: Path, identity: FakeIdentity, capsys: pytest.CaptureFixture[str]
) -> None:
    assert await run(admin_dir, ["enroll"], io.StringIO("HET1.not-a-token\n")) == 1
    assert capsys.readouterr().err.startswith("error: validation: ")
    assert identity.tokens == []


async def test_the_token_never_reaches_the_log(
    admin_dir: Path, identity: FakeIdentity, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.DEBUG, logger="hermes")
    await run(admin_dir, ["enroll"], io.StringIO(TOKEN + "\n"))
    assert "identity.enroll" in caplog.text
    assert TOKEN not in caplog.text
