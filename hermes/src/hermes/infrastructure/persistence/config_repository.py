"""The configuration in its tables: loaded whole, replaced whole in one transaction (FR-024, FR-025)."""

from __future__ import annotations

from collections import defaultdict
from typing import Any

from sqlalchemy import delete, select, text, tuple_
from sqlalchemy.dialects.postgresql import insert
from sqlalchemy.ext.asyncio import AsyncConnection, AsyncEngine

from hermes.application.ports import AuditEntry
from hermes.domain.configuration.digest import configuration_digest, version_digest
from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    Contact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
    Variable,
)
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.infrastructure.persistence.audit_repository import insert_entry
from hermes.infrastructure.persistence.engine import DATABASE_DOWN, database_unavailable
from hermes.infrastructure.persistence.schema import (
    channels,
    config_contacts,
    config_instances,
    config_templates,
    config_variables,
)

CONFIG_LOCK = 0x436F6E666967


class SqlConfigRepository:
    def __init__(self, engine: AsyncEngine) -> None:
        self._engine = engine

    async def load(self) -> Configuration:
        try:
            async with self._engine.connect() as connection:
                return await self._load(connection)
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def _load(self, connection: AsyncConnection) -> Configuration:
        instances = tuple(
            Instance(
                name=row.name,
                provider=row.provider,
                params=row.params,
                secrets={field: SecretReference.parse(value) for field, value in row.secrets.items()},
                requests_per_minute=row.requests_per_minute,
            )
            for row in await connection.execute(select(config_instances).order_by(config_instances.c.name))
        )
        versions: dict[str, list[TemplateVersion]] = defaultdict(list)
        template_rows = await connection.execute(
            select(config_templates).order_by(config_templates.c.template_id, config_templates.c.version)
        )
        for row in template_rows:
            versions[row.template_id].append(
                TemplateVersion(row.version, row.data_schema, MessageTexts.from_json(row.message))
            )
        variables = tuple(
            Variable(row.name, row.value)
            for row in await connection.execute(select(config_variables).order_by(config_variables.c.name))
        )
        specs: dict[str, list[ChannelSpec]] = defaultdict(list)
        for row in await connection.execute(select(channels).order_by(channels.c.id)):
            specs[row.contact].append(ChannelSpec(row.service, row.provider, row.instance))
        contact_names = (
            await connection.execute(select(config_contacts.c.name).order_by(config_contacts.c.name))
        ).scalars()
        return Configuration(
            instances=instances,
            templates=tuple(Template(id=key, versions=tuple(value)) for key, value in versions.items()),
            variables=variables,
            contacts=tuple(Contact(name=name, channels=tuple(specs[name])) for name in contact_names),
        )

    async def replace(self, configuration: Configuration) -> None:
        """Deletes what is gone, upserts the rest and inserts the new channels; the others keep their runtime."""
        try:
            async with self._engine.begin() as connection:
                await connection.execute(text("SELECT pg_advisory_xact_lock(:key)"), {"key": CONFIG_LOCK})
                await self._replace(connection, configuration)
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def replace_if(self, expected: bytes, configuration: Configuration, audit: AuditEntry) -> Configuration:
        try:
            async with self._engine.begin() as connection:
                await connection.execute(text("SELECT pg_advisory_xact_lock(:key)"), {"key": CONFIG_LOCK})
                if configuration_digest(await self._load(connection)) != expected:
                    raise HermesError(ErrorCode.CONFLICT, "the configuration changed since the plan: plan it again")
                await self._replace(connection, configuration)
                await insert_entry(connection, audit)
                return await self._load(connection)
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def _replace(self, connection: AsyncConnection, configuration: Configuration) -> None:
        ordered = [
            (contact.name, spec.service, spec.provider, spec.instance)
            for contact in configuration.contacts
            for spec in contact.channels
        ]
        wanted = set(ordered)
        existing = await connection.execute(
            select(channels.c.id, channels.c.contact, channels.c.service, channels.c.provider, channels.c.instance)
        )
        stale = [row.id for row in existing if (row.contact, row.service, row.provider, row.instance) not in wanted]
        if stale:
            await connection.execute(delete(channels).where(channels.c.id.in_(stale)))
        names = [contact.name for contact in configuration.contacts]
        await connection.execute(delete(config_contacts).where(config_contacts.c.name.not_in(names)))
        variables = [variable.name for variable in configuration.variables]
        await connection.execute(delete(config_variables).where(config_variables.c.name.not_in(variables)))
        versions = [
            (template.id, version.version) for template in configuration.templates for version in template.versions
        ]
        key = tuple_(config_templates.c.template_id, config_templates.c.version)
        await connection.execute(delete(config_templates).where(key.not_in(versions)))
        instances = [instance.name for instance in configuration.instances]
        await connection.execute(delete(config_instances).where(config_instances.c.name.not_in(instances)))

        if configuration.instances:
            rows: list[dict[str, Any]] = [
                {
                    "name": instance.name,
                    "provider": instance.provider,
                    "params": dict(instance.params),
                    "secrets": {field: str(reference) for field, reference in instance.secrets.items()},
                    "requests_per_minute": instance.requests_per_minute,
                }
                for instance in configuration.instances
            ]
            statement = insert(config_instances).values(rows)
            await connection.execute(
                statement.on_conflict_do_update(
                    index_elements=[config_instances.c.name],
                    set_={
                        "provider": statement.excluded.provider,
                        "params": statement.excluded.params,
                        "secrets": statement.excluded.secrets,
                        "requests_per_minute": statement.excluded.requests_per_minute,
                    },
                )
            )
        if configuration.variables:
            statement = insert(config_variables).values(
                [{"name": variable.name, "value": variable.value} for variable in configuration.variables]
            )
            await connection.execute(
                statement.on_conflict_do_update(
                    index_elements=[config_variables.c.name], set_={"value": statement.excluded.value}
                )
            )
        if versions:
            statement = insert(config_templates).values(
                [
                    {
                        "template_id": template.id,
                        "version": version.version,
                        "data_schema": dict(version.data_schema),
                        "message": version.message.as_json(),
                        "digest": version_digest(version),
                    }
                    for template in configuration.templates
                    for version in template.versions
                ]
            )
            await connection.execute(
                statement.on_conflict_do_update(
                    index_elements=[config_templates.c.template_id, config_templates.c.version],
                    set_={
                        "data_schema": statement.excluded.data_schema,
                        "message": statement.excluded.message,
                        "digest": statement.excluded.digest,
                    },
                )
            )
        if names:
            await connection.execute(
                insert(config_contacts).values([{"name": name} for name in names]).on_conflict_do_nothing()
            )
        if wanted:
            await connection.execute(
                insert(channels)
                .values(
                    [
                        {"contact": contact, "service": service, "provider": provider, "instance": instance}
                        for contact, service, provider, instance in ordered
                    ]
                )
                .on_conflict_do_nothing(constraint="uq_channels_contact_service_provider_instance")
            )
