"""The `jti` already used, one row per `(iss, jti)` until `exp` plus 30 s (FR-003, research R3), so a repeated token is
refused also after a restart; a background task deletes the rows past their expiry."""

from __future__ import annotations

from datetime import datetime

from sqlalchemy import delete
from sqlalchemy.dialects.postgresql import insert
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.infrastructure.persistence.engine import DATABASE_DOWN, database_unavailable
from hermes.infrastructure.persistence.schema import used_call_tokens


class SqlUsedTokens:
    def __init__(self, engine: AsyncEngine) -> None:
        self._engine = engine

    async def remember(self, issuer: str, jti: str, expires_at: datetime) -> bool:
        statement = (
            insert(used_call_tokens)
            .values(issuer=issuer, jti=jti, expires_at=expires_at)
            .on_conflict_do_nothing(index_elements=[used_call_tokens.c.issuer, used_call_tokens.c.jti])
            .returning(used_call_tokens.c.jti)
        )
        try:
            async with self._engine.begin() as connection:
                return (await connection.execute(statement)).first() is not None
        except DATABASE_DOWN:
            raise database_unavailable() from None

    async def purge(self, now: datetime) -> int:
        try:
            async with self._engine.begin() as connection:
                result = await connection.execute(delete(used_call_tokens).where(used_call_tokens.c.expires_at < now))
                return result.rowcount
        except DATABASE_DOWN:
            raise database_unavailable() from None
