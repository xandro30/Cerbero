"""The persistence (T013): migration 0001, its constraints, the append-only audit and the configuration repository."""

import asyncio

import psycopg
import pytest
from alembic.autogenerate import compare_metadata
from alembic.migration import MigrationContext
from sqlalchemy import Connection, text
from sqlalchemy.exc import DBAPIError, IntegrityError
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.domain.configuration.digest import configuration_digest
from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    Contact,
    Fact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
    Variable,
)
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.persistence.config_repository import SqlConfigRepository
from hermes.infrastructure.persistence.engine import create_engine
from hermes.infrastructure.persistence.migrations import apply_migrations
from hermes.infrastructure.persistence.schema import metadata
from support.postgres import OTHER_DATABASE, Database

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

TABLES = {
    "config_instances",
    "config_templates",
    "config_variables",
    "config_contacts",
    "channels",
    "idempotency_keys",
    "used_call_tokens",
    "audit_log",
}


async def run(engine: AsyncEngine, sql: str, **params: object) -> None:
    async with engine.begin() as connection:
        await connection.execute(text(sql), params)


async def rejects(engine: AsyncEngine, sql: str, **params: object) -> None:
    with pytest.raises((IntegrityError, DBAPIError)):
        await run(engine, sql, **params)


async def test_migration_0001_creates_the_eight_tables(migrated: AsyncEngine) -> None:
    async with migrated.connect() as connection:
        names = set(
            (
                await connection.execute(
                    text("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'")
                )
            ).scalars()
        )
        version = (await connection.execute(text("SELECT version_num FROM alembic_version"))).scalar_one()
    assert names == TABLES | {"alembic_version"}
    assert version == "0001"


async def test_the_schema_of_the_code_is_the_one_of_the_migration(migrated: AsyncEngine) -> None:
    def differences(connection: Connection) -> list[object]:
        context = MigrationContext.configure(connection, opts={"compare_type": True})
        return list(compare_metadata(context, metadata))

    async with migrated.connect() as connection:
        assert await connection.run_sync(differences) == []


async def test_the_templates_and_the_instances_keep_their_rules(migrated: AsyncEngine) -> None:
    template = "INSERT INTO config_templates VALUES (:t, :v, '{}', '{}', '\\x00')"
    await run(migrated, template, t="device_unknown", v=1)
    await rejects(migrated, template, t="device_unknown", v=1)
    await rejects(migrated, template, t="other", v=0)
    instance = "INSERT INTO config_instances (name, provider, requests_per_minute) VALUES (:n, 'telegram', :r)"
    await run(migrated, instance, n="a", r=None)
    await run(migrated, instance, n="b", r=1)
    await run(migrated, instance, n="c", r=600)
    await rejects(migrated, instance, n="d", r=0)
    await rejects(migrated, instance, n="e", r=601)


async def test_the_channels_keep_their_rules(migrated: AsyncEngine) -> None:
    await run(migrated, "INSERT INTO config_instances (name, provider) VALUES ('family_bot', 'telegram')")
    await run(migrated, "INSERT INTO config_contacts VALUES ('contacto_demo')")
    channel = (
        "INSERT INTO channels (contact, service, provider, instance, address, verified_at, code_hash, code_expires_at) "
        "VALUES ('contacto_demo', 'chat', 'telegram', 'family_bot', :address, :verified, :code, :expires)"
    )
    await rejects(migrated, channel, address=None, verified="2026-11-14T10:00:00Z", code=None, expires=None)
    await rejects(migrated, channel, address=None, verified=None, code=b"h", expires=None)
    await rejects(
        migrated, channel, address="1", verified="2026-11-14T10:00:00Z", code=b"h", expires="2026-11-14T11:00:00Z"
    )
    await run(migrated, channel, address=None, verified=None, code=b"h", expires="2026-11-14T11:00:00Z")
    await rejects(migrated, channel, address=None, verified=None, code=None, expires=None)
    await run(migrated, "DELETE FROM config_contacts WHERE name = 'contacto_demo'")
    async with migrated.connect() as connection:
        assert (await connection.execute(text("SELECT count(*) FROM channels"))).scalar_one() == 0


async def test_the_keys_tokens_and_audit_keep_their_rules(migrated: AsyncEngine) -> None:
    key = (
        "INSERT INTO idempotency_keys (caller, key, fingerprint, state, created_at, pending_audit) "
        "VALUES ('c', :k, '\\x00', :s, now(), CAST(:p AS jsonb))"
    )
    await run(migrated, key, k="k1", s="in_progress", p="{}")
    await rejects(migrated, key, k="k1", s="completed", p=None)
    await rejects(migrated, key, k="k2", s="other", p=None)
    await rejects(migrated, key, k="k3", s="in_progress", p=None)
    await rejects(migrated, key, k="k4", s="completed", p="{}")
    await run(migrated, key, k="k5", s="completed", p=None)
    token = "INSERT INTO used_call_tokens VALUES ('spiffe://cerbero.test/hades', 'j1', now())"
    await run(migrated, token)
    await rejects(migrated, token)
    audit = "INSERT INTO audit_log (at, kind, actor, outcome) VALUES (now(), :kind, 'admin', '{}')"
    for kind in ("send", "config_applied", "verification_code_issued", "channel_verified"):
        await run(migrated, audit, kind=kind)
    await rejects(migrated, audit, kind="other")


async def test_the_audit_log_is_append_only(migrated: AsyncEngine) -> None:
    await run(migrated, "INSERT INTO audit_log (at, kind, actor, outcome) VALUES (now(), 'send', 'admin', '{}')")
    for change in ("UPDATE audit_log SET actor = 'other'", "DELETE FROM audit_log", "TRUNCATE audit_log"):
        await rejects(migrated, change)
    async with migrated.connect() as connection:
        assert (await connection.execute(text("SELECT actor FROM audit_log"))).scalar_one() == "admin"


async def test_hermes_cannot_connect_to_another_database(fresh_database: Database) -> None:
    with pytest.raises(psycopg.OperationalError):
        await asyncio.to_thread(psycopg.connect, fresh_database.conninfo(dbname=OTHER_DATABASE))


async def test_the_migrations_apply_twice_and_at_the_same_time(fresh_database: Database) -> None:
    password = SecretValue(fresh_database.password)
    first = create_engine(fresh_database.url, lambda: password)
    second = create_engine(fresh_database.url, lambda: password)
    try:
        await asyncio.gather(apply_migrations(first), apply_migrations(second))
        await apply_migrations(first)
        async with first.connect() as connection:
            version = (await connection.execute(text("SELECT version_num FROM alembic_version"))).scalar_one()
        assert version == "0001"
    finally:
        await first.dispose()
        await second.dispose()


TOKEN = SecretReference.parse("secret://hermes/telegram/family_bot/bot_token")


def configuration(*, instance: str = "family_bot", variables: tuple[Variable, ...] = ()) -> Configuration:
    message = MessageTexts(
        title="Dispositivo desconocido en la red",
        severity="high",
        summary="Se ha conectado ${data.mac}.",
        facts=(Fact("MAC", "${data.mac}"),),
        actions_done=("Bloqueado",),
        expected="Aprobar o denegar",
        link="${vars.case_base_url}/${data.case_id}",
    )
    return Configuration(
        instances=(
            Instance(name="family_bot", provider="telegram", params={}, secrets={"bot_token": TOKEN}),
            Instance(
                name="admin_bot",
                provider="telegram",
                params={"x": 1},
                secrets={"bot_token": TOKEN},
                requests_per_minute=20,
            ),
        ),
        templates=(
            Template(
                id="device_unknown",
                versions=(
                    TemplateVersion(1, {"type": "object"}, message),
                    TemplateVersion(2, {"type": "object", "required": ["mac"]}, message),
                ),
            ),
        ),
        variables=(Variable("case_base_url", "https://tenaro.cerbero.home/cases"), *variables),
        contacts=(Contact(name="contacto_demo", channels=(ChannelSpec("chat", "telegram", instance),)),),
    )


async def test_the_repository_replaces_and_loads_the_whole_configuration(migrated: AsyncEngine) -> None:
    repository = SqlConfigRepository(migrated)
    stored = configuration()
    await repository.replace(stored)
    assert configuration_digest(await repository.load()) == configuration_digest(stored)
    smaller = Configuration(instances=(), templates=(), variables=(), contacts=())
    await repository.replace(smaller)
    assert configuration_digest(await repository.load()) == configuration_digest(smaller)


async def test_a_channel_that_does_not_change_keeps_its_address_and_verification(migrated: AsyncEngine) -> None:
    repository = SqlConfigRepository(migrated)
    await repository.replace(configuration())
    await run(migrated, "UPDATE channels SET address = '987654321', verified_at = now() WHERE instance = 'family_bot'")
    await repository.replace(configuration(variables=(Variable("extra", "1"),)))
    async with migrated.connect() as connection:
        kept = (await connection.execute(text("SELECT address, verified_at IS NOT NULL FROM channels"))).one()
    assert tuple(kept) == ("987654321", True)
    await repository.replace(configuration(instance="admin_bot"))
    async with migrated.connect() as connection:
        rows = (await connection.execute(text("SELECT instance, address, verified_at FROM channels"))).all()
    assert [tuple(row) for row in rows] == [("admin_bot", None, None)]
