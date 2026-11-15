"""The append-only audit log in its table (FR-029, research R14): a trigger refuses to change or delete its rows."""

from __future__ import annotations

from typing import Any

from sqlalchemy import insert
from sqlalchemy.ext.asyncio import AsyncConnection, AsyncEngine

from hermes.application.ports import AuditEntry
from hermes.infrastructure.persistence.engine import DATABASE_DOWN, database_unavailable
from hermes.infrastructure.persistence.schema import audit_log


async def insert_entry(connection: AsyncConnection, entry: AuditEntry) -> None:
    """The entry inside a transaction that the caller opens, so it goes or fails with the rest of it."""
    row: dict[str, Any] = {
        "at": entry.at,
        "kind": entry.kind.value,
        "actor": entry.actor,
        "function": entry.function,
        "instance": entry.instance,
        "recipients": list(entry.recipients) if entry.recipients is not None else None,
        "side_effect": entry.side_effect,
        "discloses_data": entry.discloses_data,
        "audit_context": dict(entry.audit_context) if entry.audit_context is not None else None,
        "idempotency_key": entry.idempotency_key,
        "outcome": dict(entry.outcome),
    }
    await connection.execute(insert(audit_log).values(row))


class SqlAuditLog:
    def __init__(self, engine: AsyncEngine) -> None:
        self._engine = engine

    async def append(self, entry: AuditEntry) -> None:
        try:
            async with self._engine.begin() as connection:
                await insert_entry(connection, entry)
        except DATABASE_DOWN:
            raise database_unavailable() from None
