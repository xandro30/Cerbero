"""Plan, apply and export the configuration (T075, FR-024, FR-025, SC-010), on the throwaway PostgreSQL."""

from __future__ import annotations

from datetime import UTC, datetime

import pytest
import yaml
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.configuration import ConfigurationAdmin, ConfigurationState, HecateSecretChecker
from hermes.application.ports import AuditEntry
from hermes.domain.configuration.model import Configuration
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.config_file.export import write_configuration
from hermes.infrastructure.config_file.models import read_configuration
from hermes.infrastructure.persistence.config_repository import SqlConfigRepository
from hermes.infrastructure.providers.registry import read_manifest
from support.audit_log import refuse_audit_entries
from support.clock import FakeClock
from support.config_files import VALID_YAML
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, configuration
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

MANIFESTS = {"telegram": read_manifest("telegram"), "fake": read_manifest("fake")}


class ReadBackFails(SqlConfigRepository):
    """The configuration in its tables, where the database stops answering right after an application commits."""

    def __init__(self, engine: AsyncEngine) -> None:
        super().__init__(engine)
        self.committed = False

    async def replace_if(self, expected: bytes, configuration: Configuration, audit: AuditEntry) -> Configuration:
        loaded = await super().replace_if(expected, configuration, audit)
        self.committed = True
        return loaded

    async def load(self) -> Configuration:
        if self.committed:
            raise HermesError(ErrorCode.UNAVAILABLE, "the database does not answer")
        return await super().load()


class World:
    def __init__(self, engine: AsyncEngine, repository: SqlConfigRepository | None = None) -> None:
        self.engine = engine
        self.clock = FakeClock()
        self.hecate = FakeHecate(TestCa(now=self.clock.now()), self.clock)
        self.hecate.secrets = {BOT_TOKEN_REF: BOT_TOKEN}
        self.state = ConfigurationState(Configuration.empty())
        self.repository = repository if repository is not None else SqlConfigRepository(engine)
        self.loaded: list[Configuration] = []
        self.admin = ConfigurationAdmin(
            state=self.state,
            repository=self.repository,
            read=read_configuration,
            write=write_configuration,
            manifests=MANIFESTS,
            fake_enabled=True,
            secrets=HecateSecretChecker(self.hecate),
            clock=self.clock,
            applied=self.loaded.append,
        )

    async def applied(self, source: str = VALID_YAML) -> None:
        plan = await self.admin.plan(source)
        await self.admin.apply(source, plan.digest)

    async def rows(self, sql: str) -> list[dict[str, object]]:
        async with self.engine.connect() as connection:
            return [dict(row._mapping) for row in await connection.execute(text(sql))]


@pytest.fixture
def world(migrated: AsyncEngine) -> World:
    return World(migrated)


async def test_the_plan_shows_the_changes_and_the_application_writes_all_of_them(world: World) -> None:
    plan = await world.admin.plan(VALID_YAML)
    assert {change.action for change in plan.changes} == {"create"}
    assert await world.repository.load() == Configuration.empty()
    applied = await world.admin.apply(VALID_YAML, plan.digest)
    assert applied == plan.changes
    assert await world.repository.load() == configuration()
    assert world.state.current() == configuration()
    assert world.loaded == [configuration()]


async def test_an_application_whose_audit_entry_cannot_be_written_changes_nothing(world: World) -> None:
    """The entry `config_applied` goes in the transaction of the change (FR-029, FR-024): without it, nothing."""
    await refuse_audit_entries(world.engine)
    plan = await world.admin.plan(VALID_YAML)
    with pytest.raises(DBAPIError):
        await world.admin.apply(VALID_YAML, plan.digest)
    assert await world.repository.load() == Configuration.empty()
    assert world.state.current() == Configuration.empty()
    assert world.loaded == []


async def test_the_configuration_in_use_is_the_applied_one_even_if_the_database_stops_answering_right_after(
    migrated: AsyncEngine,
) -> None:
    """The application reads back what it wrote inside its own transaction (FR-024, research R7): once it commits,
    the snapshot and the pollers take it, with nothing left in between that could fail."""
    world = World(migrated, repository=ReadBackFails(migrated))
    await world.applied()
    assert world.state.current() == configuration()
    assert world.loaded == [configuration()]


async def test_applying_the_same_file_twice_changes_nothing(world: World) -> None:
    await world.applied()
    plan = await world.admin.plan(VALID_YAML)
    assert plan.changes == ()
    assert await world.admin.apply(VALID_YAML, plan.digest) == ()
    assert await world.repository.load() == configuration()
    assert len(await world.rows("SELECT * FROM audit_log WHERE kind = 'config_applied'")) == 1


async def test_a_new_secret_reference_that_does_not_resolve_is_validation_and_nothing_is_written(world: World) -> None:
    world.hecate.secrets = {}
    with pytest.raises(HermesError) as raised:
        await world.admin.plan(VALID_YAML)
    assert raised.value.code is ErrorCode.VALIDATION
    assert [detail["path"] for detail in raised.value.details] == ["instances[0].secrets.bot_token"]
    assert BOT_TOKEN_REF not in raised.value.details[0]["message"]
    assert await world.repository.load() == Configuration.empty()


async def test_with_hecate_down_nothing_is_applied(world: World) -> None:
    world.hecate.down = True
    with pytest.raises(HermesError) as raised:
        await world.admin.plan(VALID_YAML)
    assert raised.value.code is ErrorCode.UNAVAILABLE
    assert await world.repository.load() == Configuration.empty()


async def test_references_that_do_not_change_are_not_checked_again(world: World) -> None:
    await world.applied()
    world.hecate.down = True
    plan = await world.admin.plan(VALID_YAML.replace("https://cerbero.home", "https://tenaro.cerbero.home"))
    assert [(change.action, change.kind, change.name) for change in plan.changes] == [
        ("update", "variable", "console_url")
    ]


async def test_another_plan_digest_is_a_conflict(world: World) -> None:
    with pytest.raises(HermesError) as raised:
        await world.admin.apply(VALID_YAML, "sha256:" + "0" * 64)
    assert raised.value.code is ErrorCode.CONFLICT
    assert await world.repository.load() == Configuration.empty()


async def test_a_change_in_the_database_after_the_plan_is_a_conflict(world: World) -> None:
    plan = await world.admin.plan(VALID_YAML)
    other = VALID_YAML.replace("https://cerbero.home", "https://otro.cerbero.home")
    await world.applied(other)
    with pytest.raises(HermesError) as raised:
        await world.admin.apply(VALID_YAML, plan.digest)
    assert raised.value.code is ErrorCode.CONFLICT


async def test_a_failure_halfway_writes_nothing(world: World, monkeypatch: pytest.MonkeyPatch) -> None:
    original = SqlConfigRepository._replace

    async def broken(self: SqlConfigRepository, connection: object, configuration: Configuration) -> None:
        await original(self, connection, configuration)  # type: ignore[arg-type]
        raise RuntimeError("the disk is full")

    monkeypatch.setattr(SqlConfigRepository, "_replace", broken)
    plan = await world.admin.plan(VALID_YAML)
    with pytest.raises(RuntimeError):
        await world.admin.apply(VALID_YAML, plan.digest)
    assert await world.repository.load() == Configuration.empty()
    assert world.state.current() == Configuration.empty()


async def test_a_file_with_errors_writes_nothing_and_says_what_fails(world: World) -> None:
    broken = VALID_YAML.replace("instance: family_bot", "instance: missing_bot")
    with pytest.raises(HermesError) as raised:
        await world.admin.plan(broken)
    assert raised.value.code is ErrorCode.VALIDATION
    assert {detail["path"] for detail in raised.value.details} == {
        "contacts[0].channels[0].instance",
        "contacts[2].channels[0].instance",
    }


async def test_the_snapshot_changes_at_once_and_one_taken_before_stays_as_it_was(world: World) -> None:
    before = world.state.current()
    await world.applied()
    assert before == Configuration.empty()
    assert world.state.current() == configuration()


async def test_the_export_is_ordered_without_secrets_addresses_or_codes_and_applying_it_changes_nothing(
    world: World,
) -> None:
    await world.applied()
    async with world.engine.begin() as connection:
        await connection.execute(
            text(
                "UPDATE channels SET address = :address, verified_at = :at "
                "WHERE contact = 'contacto_demo' AND instance = 'family_bot'"
            ),
            {"address": CHAT, "at": datetime(2026, 11, 14, 10, 0, tzinfo=UTC)},
        )
        await connection.execute(
            text(
                "UPDATE channels SET code_hash = :code, code_expires_at = :at "
                "WHERE contact = 'contacto_fake' AND instance = 'test_fake'"
            ),
            {"code": b"\x01" * 32, "at": datetime(2026, 11, 14, 10, 30, tzinfo=UTC)},
        )
    exported = await world.admin.export()
    assert CHAT not in exported
    assert "verified" not in exported
    assert "code" not in exported
    assert BOT_TOKEN not in exported
    assert BOT_TOKEN_REF in exported
    document = yaml.safe_load(exported)
    assert [contact["name"] for contact in document["contacts"]] == ["contacto_demo", "contacto_fake", "dos_canales"]
    assert [instance["name"] for instance in document["instances"]] == ["family_bot", "test_fake"]
    assert [version["version"] for version in document["templates"][0]["versions"]] == [1, 2]
    assert (await world.admin.plan(exported)).changes == ()
    assert len(await world.rows("SELECT * FROM channels WHERE verified_at IS NOT NULL")) == 1


async def test_the_audit_keeps_the_changes_without_values_of_secrets(world: World) -> None:
    await world.applied()
    [row] = await world.rows("SELECT * FROM audit_log WHERE kind = 'config_applied'")
    assert row["actor"] == "admin"
    changes = row["outcome"]["changes"]  # type: ignore[index]
    assert {"action": "create", "kind": "instance", "name": "family_bot", "detail": None} in changes
    assert BOT_TOKEN not in str(row)
