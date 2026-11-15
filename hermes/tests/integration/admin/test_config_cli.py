"""The configuration and the codes through the local channel and the CLI (T080, contracts/admin-channel.md, cli.md)."""

from __future__ import annotations

import asyncio
import io
import re
import stat
from collections.abc import AsyncIterator
from dataclasses import replace
from pathlib import Path
from typing import Any

import anyio
import httpx
import pytest

from hermes.application.configuration import ConfigurationAdmin, ConfigurationState, HecateSecretChecker
from hermes.application.verification import Verification
from hermes.domain.configuration.model import Configuration
from hermes.infrastructure.admin import cli
from hermes.infrastructure.admin.app import AdminServices, create_admin_app, install_admin
from hermes.infrastructure.config_file.export import write_configuration
from hermes.infrastructure.config_file.models import read_configuration
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.servers import EmbeddedServer, server_config
from hermes.infrastructure.sockets import bind_unix_socket, prepare_private_dir
from support.clock import FakeClock
from support.config_files import VALID_YAML
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, configuration
from support.contract import contract
from support.doubles import MemoryAudit, MemoryConfigStore, Providers, StaticSecrets
from support.fake_hecate import FakeHecate
from support.paths import short_dir
from support.pki import TestCa

pytestmark = pytest.mark.anyio

MANIFESTS = {"telegram": read_manifest("telegram"), "fake": read_manifest("fake")}


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.store = MemoryConfigStore(audit=MemoryAudit())
        self.state = ConfigurationState(Configuration.empty())
        hecate = FakeHecate(TestCa(now=self.clock.now()), self.clock)
        hecate.secrets = {BOT_TOKEN_REF: BOT_TOKEN}
        self.admin = ConfigurationAdmin(
            state=self.state,
            repository=self.store,
            read=read_configuration,
            write=write_configuration,
            manifests=MANIFESTS,
            fake_enabled=True,
            secrets=HecateSecretChecker(hecate),
            clock=self.clock,
        )
        self.verification = Verification(
            configuration=self.state,
            channels=self.store,
            providers=Providers(fake=FakeProvider(read_manifest("fake"), self.clock)),
            secrets=StaticSecrets({}),
            clock=self.clock,
            code_minutes=30,
        )


def in_export_order(configuration: Configuration) -> Configuration:
    """The export lists the channels of each contact by service, provider and instance (contracts/config-file.md)."""
    contacts = tuple(replace(contact, channels=tuple(sorted(contact.channels))) for contact in configuration.contacts)
    return replace(configuration, contacts=contacts)


class NoIdentity:
    async def enroll(self, token: object) -> Any:
        raise AssertionError("not used")


@pytest.fixture
def world() -> World:
    return World()


@pytest.fixture
async def admin_dir(world: World) -> AsyncIterator[Path]:
    with short_dir() as base:
        directory = base / "admin"
        prepare_private_dir(directory)
        sock = bind_unix_socket(directory / "admin.sock")
        app = create_admin_app()
        install_admin(
            app, AdminServices(identity=NoIdentity(), configuration=world.admin, verification=world.verification)
        )
        server = EmbeddedServer(server_config(app))
        task = asyncio.create_task(server.serve(sockets=[sock]))
        yield directory
        server.should_exit = True
        await task
        sock.close()


async def run(admin_dir: Path, args: list[str], stdin: str = "") -> int:
    env = {"HERMES_ADMIN_DIR": str(admin_dir)}
    return await anyio.to_thread.run_sync(lambda: cli.main(args, env=env, stdin=io.StringIO(stdin)))


@pytest.fixture
def config_file(tmp_path: Path) -> Path:
    path = tmp_path / "hermes.yaml"
    path.write_text(VALID_YAML, encoding="utf-8")
    return path


async def test_config_apply_shows_each_change_asks_and_applies(
    admin_dir: Path, world: World, config_file: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    assert await run(admin_dir, ["config", "apply", str(config_file)], "yes\n") == 0
    out = capsys.readouterr().out
    lines = out.splitlines()
    changes = [line for line in lines if re.match(r"^[+~-] ", line)]
    assert "+ instance family_bot" in changes
    assert "+ template_version device_unknown v2" in changes
    assert "+ channel contacto_demo telegram/family_bot" in changes
    assert f"apply these {len(changes)} changes? [yes/no] applied {len(changes)} changes" in out
    assert world.store.configuration == configuration()


async def test_applying_the_same_file_again_says_no_changes(
    admin_dir: Path, config_file: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    await run(admin_dir, ["config", "apply", str(config_file), "--yes"])
    capsys.readouterr()
    assert await run(admin_dir, ["config", "apply", str(config_file)]) == 0
    assert capsys.readouterr().out == "no changes\n"


@pytest.mark.parametrize("answer", ["no\n", "y\n", ""])
async def test_another_answer_or_no_input_cancels(
    admin_dir: Path, world: World, config_file: Path, capsys: pytest.CaptureFixture[str], answer: str
) -> None:
    assert await run(admin_dir, ["config", "apply", str(config_file)], answer) == 1
    assert capsys.readouterr().err == "cancelled\n"
    assert world.store.configuration == Configuration.empty()


async def test_with_yes_it_does_not_ask(admin_dir: Path, config_file: Path, capsys: pytest.CaptureFixture[str]) -> None:
    assert await run(admin_dir, ["config", "apply", str(config_file), "--yes"]) == 0
    out = capsys.readouterr().out
    assert "[yes/no]" not in out
    assert re.search(r"^applied \d+ changes$", out, re.MULTILINE)


async def test_the_validation_errors_come_as_path_and_message(
    admin_dir: Path, world: World, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    broken = tmp_path / "broken.yaml"
    broken.write_text(VALID_YAML.replace("instance: family_bot", "instance: missing_bot"), encoding="utf-8")
    assert await run(admin_dir, ["config", "apply", str(broken), "--yes"]) == 1
    err = capsys.readouterr().err.splitlines()
    assert err[0].startswith("error: validation: ")
    assert {line.split(": ")[1] for line in err[1:]} == {
        "contacts[0].channels[0].instance",
        "contacts[2].channels[0].instance",
    }
    assert world.store.configuration == Configuration.empty()


async def test_a_file_that_cannot_be_read_says_why(
    admin_dir: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    missing = tmp_path / "missing.yaml"
    assert await run(admin_dir, ["config", "apply", str(missing)]) == 1
    assert capsys.readouterr().err.startswith(f"error: cannot read {missing}: ")


async def test_config_export_writes_the_file_of_the_configuration(
    admin_dir: Path, world: World, config_file: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    await run(admin_dir, ["config", "apply", str(config_file), "--yes"])
    capsys.readouterr()
    assert await run(admin_dir, ["config", "export"]) == 0
    assert capsys.readouterr().out == await world.admin.export()
    target = tmp_path / "exported.yaml"
    assert await run(admin_dir, ["config", "export", "--out", str(target)]) == 0
    assert capsys.readouterr().out == f"exported to {target}\n"
    assert stat.S_IMODE(target.stat().st_mode) == 0o600
    assert read_configuration(target.read_text(encoding="utf-8")) == in_export_order(configuration())


async def test_config_export_never_overwrites(
    admin_dir: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    target = tmp_path / "exported.yaml"
    target.write_text("keep me", encoding="utf-8")
    assert await run(admin_dir, ["config", "export", "--out", str(target)]) == 1
    assert capsys.readouterr().err == f"error: {target} already exists; it is not overwritten\n"
    assert target.read_text(encoding="utf-8") == "keep me"


async def test_contacts_code_writes_the_code_and_when_it_expires(
    admin_dir: Path, config_file: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    await run(admin_dir, ["config", "apply", str(config_file), "--yes"])
    capsys.readouterr()
    assert await run(admin_dir, ["contacts", "code", "contacto_fake", "--instance", "test_fake"]) == 0
    first, second = capsys.readouterr().out.splitlines()
    assert re.fullmatch(
        r"code [0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4} for contacto_fake on test_fake expires 2026-11-14T10:30:00Z",
        first,
    )
    assert second == "send it to the bot from the person's Telegram account"


async def test_contacts_code_of_a_contact_that_does_not_exist_is_not_found(
    admin_dir: Path, config_file: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    await run(admin_dir, ["config", "apply", str(config_file), "--yes"])
    capsys.readouterr()
    assert await run(admin_dir, ["contacts", "code", "nadie", "--instance", "test_fake"]) == 1
    assert capsys.readouterr().err.startswith("error: not_found: ")


async def test_the_operations_answer_as_the_contract_says(admin_dir: Path) -> None:
    transport = httpx.AsyncHTTPTransport(uds=str(admin_dir / "admin.sock"))
    known = contract()
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as http:
        plan = (await http.post("/config/plan", json={"yaml": VALID_YAML})).json()
        assert not known.errors(known.common_uri("Completed"), plan)
        assert re.fullmatch(r"sha256:[0-9a-f]{64}", plan["data"]["plan_digest"])
        assert {"action": "create", "kind": "instance", "name": "family_bot", "detail": None} in plan["data"]["changes"]
        applied = (
            await http.post("/config/apply", json={"yaml": VALID_YAML, "plan_digest": plan["data"]["plan_digest"]})
        ).json()
        assert applied["data"]["changes"] == plan["data"]["changes"]
        exported = (await http.get("/config/export")).json()
        assert read_configuration(exported["data"]["yaml"]) == in_export_order(configuration())
        code = (await http.post("/contacts/code", json={"contact": "contacto_fake", "instance": "test_fake"})).json()
        assert re.fullmatch(r"[0-9A-Z]{4}-[0-9A-Z]{4}", code["data"]["code"])
        assert code["data"]["expires_at"] == "2026-11-14T10:30:00Z"
        conflict = await http.post("/config/apply", json={"yaml": VALID_YAML, "plan_digest": "sha256:" + "0" * 64})
        assert conflict.status_code == 409
        assert not known.errors(known.common_uri("Failed"), conflict.json())


async def test_the_local_channel_takes_whole_files_up_to_its_own_limit(admin_dir: Path) -> None:
    """A file of up to 1 MiB goes inside JSON, so the local channel takes bodies of up to 8 MiB, not the 64 KiB of the
    network API; its error says its own limit."""
    transport = httpx.AsyncHTTPTransport(uds=str(admin_dir / "admin.sock"))
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as http:
        long_file = VALID_YAML + "# " + "x" * (100 * 1024) + "\n"
        plan = (await http.post("/config/plan", json={"yaml": long_file})).json()
        assert plan["status"] == "completed"
        too_big = (await http.post("/config/plan", content=b" " * (8 * 1024 * 1024 + 1))).json()
        assert too_big["error"]["code"] == "validation"
        assert too_big["error"]["message"] == "the body must be standard JSON in UTF-8 of up to 8 MiB"
