"""The keys of idempotency in their table (data-model.md#idempotency_keys, research R11): one row per caller and key,
`in_progress` from right before the provider and `completed` with its response, together with the audit entry.

While a row is `in_progress` it keeps the audit entry of its send with an unknown outcome: if a cut leaves the send
without its record, the repetition or the purge writes that entry (FR-029).
"""

from __future__ import annotations

from collections.abc import Mapping
from datetime import datetime
from typing import Any

from sqlalchemy import delete, null, select, update
from sqlalchemy.dialects.postgresql import insert
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.ports import AuditEntry, AuditKind
from hermes.domain.idempotency.record import KeyRecord, KeyState, StoredResponse
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.persistence.audit_repository import insert_entry
from hermes.infrastructure.persistence.engine import DATABASE_DOWN, database_unavailable
from hermes.infrastructure.persistence.schema import idempotency_keys

TAKEN = "another request with this Idempotency-Key is in progress"


def _pending(entry: AuditEntry) -> dict[str, Any]:
    """The audit entry that a reservation keeps, as JSON; it carries no secret, code or address (FR-029)."""
    return {
        "at": entry.at.isoformat(),
        "kind": entry.kind.value,
        "actor": entry.actor,
        "outcome": dict(entry.outcome),
        "function": entry.function,
        "instance": entry.instance,
        "recipients": list(entry.recipients) if entry.recipients is not None else None,
        "side_effect": entry.side_effect,
        "discloses_data": entry.discloses_data,
        "audit_context": dict(entry.audit_context) if entry.audit_context is not None else None,
        "idempotency_key": entry.idempotency_key,
    }


def _entry(pending: Mapping[str, Any]) -> AuditEntry:
    recipients = pending["recipients"]
    return AuditEntry(
        at=datetime.fromisoformat(pending["at"]),
        kind=AuditKind(pending["kind"]),
        actor=pending["actor"],
        outcome=pending["outcome"],
        function=pending["function"],
        instance=pending["instance"],
        recipients=tuple(recipients) if recipients is not None else None,
        side_effect=pending["side_effect"],
        discloses_data=pending["discloses_data"],
        audit_context=pending["audit_context"],
        idempotency_key=pending["idempotency_key"],
    )


class SqlIdempotencyRepository:
    def __init__(self, engine: AsyncEngine) -> None:
        self._engine = engine

    def _key(self, caller: str, key: str) -> Any:
        return (idempotency_keys.c.caller == caller) & (idempotency_keys.c.key == key)

    async def find(self, caller: str, key: str) -> KeyRecord | None:
        statement = select(
            idempotency_keys.c.fingerprint,
            idempotency_keys.c.state,
            idempotency_keys.c.http_status,
            idempotency_keys.c.response,
        ).where(self._key(caller, key))
        try:
            async with self._engine.connect() as connection:
                row = (await connection.execute(statement)).first()
        except DATABASE_DOWN:
            raise database_unavailable() from None
        if row is None:
            return None
        response = None
        if row.http_status is not None and row.response is not None:
            response = StoredResponse(row.http_status, row.response)
        return KeyRecord(fingerprint=bytes(row.fingerprint), state=KeyState(row.state), response=response)

    async def reserve(self, caller: str, key: str, fingerprint: bytes, unknown: AuditEntry) -> None:
        statement = (
            insert(idempotency_keys)
            .values(
                caller=caller,
                key=key,
                fingerprint=fingerprint,
                state=KeyState.IN_PROGRESS.value,
                created_at=unknown.at,
                pending_audit=_pending(unknown),
            )
            .on_conflict_do_nothing(index_elements=[idempotency_keys.c.caller, idempotency_keys.c.key])
            .returning(idempotency_keys.c.key)
        )
        try:
            async with self._engine.begin() as connection:
                reserved = (await connection.execute(statement)).first() is not None
        except DATABASE_DOWN:
            raise database_unavailable() from None
        if not reserved:
            raise HermesError(ErrorCode.CONFLICT, TAKEN)

    async def complete(self, caller: str, key: str, response: StoredResponse, audit: AuditEntry) -> None:
        statement = (
            update(idempotency_keys)
            .where(self._key(caller, key))
            .values(
                state=KeyState.COMPLETED.value,
                http_status=response.http_status,
                response=dict(response.body),
                pending_audit=null(),
            )
        )
        try:
            async with self._engine.begin() as connection:
                await connection.execute(statement)
                await insert_entry(connection, audit)
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def complete_unknown(self, caller: str, key: str, response: StoredResponse) -> None:
        in_progress = self._key(caller, key) & (idempotency_keys.c.state == KeyState.IN_PROGRESS.value)
        try:
            async with self._engine.begin() as connection:
                pending = (
                    await connection.execute(
                        select(idempotency_keys.c.pending_audit).where(in_progress).with_for_update()
                    )
                ).scalar_one_or_none()
                if pending is None:
                    return
                await connection.execute(
                    update(idempotency_keys)
                    .where(in_progress)
                    .values(
                        state=KeyState.COMPLETED.value,
                        http_status=response.http_status,
                        response=dict(response.body),
                        pending_audit=null(),
                    )
                )
                await insert_entry(connection, _entry(pending))
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def purge(self, before: datetime) -> int:
        old = idempotency_keys.c.created_at < before
        left = (
            select(idempotency_keys.c.pending_audit)
            .where(old, idempotency_keys.c.state == KeyState.IN_PROGRESS.value)
            .order_by(idempotency_keys.c.created_at)
            .with_for_update()
        )
        try:
            async with self._engine.begin() as connection:
                for pending in (await connection.execute(left)).scalars().all():
                    await insert_entry(connection, _entry(pending))
                result = await connection.execute(delete(idempotency_keys).where(old))
                return result.rowcount
        except DATABASE_DOWN:
            raise database_unavailable() from None
