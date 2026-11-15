"""The run-time of the channels in their table (data-model.md#channels): the address that the verification gives, when
it was verified and the code that waits. The configuration creates and deletes the rows; Hermes writes the rest, each
change with its audit entry in the same transaction (FR-029)."""

from __future__ import annotations

from collections.abc import Sequence
from datetime import datetime
from typing import Any

from sqlalchemy import and_, select, update
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.ports import AuditEntry, ChannelKey, ChannelState
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.persistence.audit_repository import insert_entry
from hermes.infrastructure.persistence.engine import DATABASE_DOWN, database_unavailable
from hermes.infrastructure.persistence.schema import channels


def _matches(key: ChannelKey) -> Any:
    return and_(
        channels.c.contact == key.contact,
        channels.c.service == key.service,
        channels.c.provider == key.provider,
        channels.c.instance == key.instance,
    )


def _state(row: Any) -> ChannelState:
    return ChannelState(
        key=ChannelKey(row.contact, row.service, row.provider, row.instance),
        address=row.address,
        verified_at=row.verified_at,
        code_hash=bytes(row.code_hash) if row.code_hash is not None else None,
        code_expires_at=row.code_expires_at,
    )


class SqlChannelRepository:
    def __init__(self, engine: AsyncEngine) -> None:
        self._engine = engine

    async def state(self, key: ChannelKey) -> ChannelState | None:
        try:
            async with self._engine.connect() as connection:
                row = (await connection.execute(select(channels).where(_matches(key)))).first()
        except DATABASE_DOWN:
            raise database_unavailable() from None
        return _state(row) if row is not None else None

    async def issue_code(self, key: ChannelKey, code_hash: bytes, expires_at: datetime, audit: AuditEntry) -> None:
        statement = (
            update(channels)
            .where(_matches(key))
            .values(address=None, verified_at=None, code_hash=code_hash, code_expires_at=expires_at)
            .returning(channels.c.id)
        )
        try:
            async with self._engine.begin() as connection:
                if (await connection.execute(statement)).first() is None:
                    raise HermesError(ErrorCode.NOT_FOUND, "the channel does not exist")
                await insert_entry(connection, audit)
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def pending(self, instance: str) -> Sequence[ChannelState]:
        statement = select(channels).where(channels.c.instance == instance, channels.c.code_hash.is_not(None))
        try:
            async with self._engine.connect() as connection:
                rows = (await connection.execute(statement.order_by(channels.c.id))).all()
        except DATABASE_DOWN:
            raise database_unavailable() from None
        return [_state(row) for row in rows]

    async def bind(
        self, key: ChannelKey, code_hash: bytes, address: str, verified_at: datetime, audit: AuditEntry
    ) -> bool:
        statement = (
            update(channels)
            .where(_matches(key), channels.c.code_hash == code_hash, channels.c.code_expires_at > verified_at)
            .values(address=address, verified_at=verified_at, code_hash=None, code_expires_at=None)
            .returning(channels.c.id)
        )
        try:
            async with self._engine.begin() as connection:
                if (await connection.execute(statement)).first() is None:
                    return False
                await insert_entry(connection, audit)
                return True
        except DATABASE_DOWN:
            raise database_unavailable() from None
