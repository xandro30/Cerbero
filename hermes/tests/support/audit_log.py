"""The audit table as the integration tests see it: its rows, and a database that refuses to write one more."""

from __future__ import annotations

from typing import Any

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncEngine


async def audit_rows(engine: AsyncEngine) -> list[Any]:
    async with engine.connect() as connection:
        return list(await connection.execute(text("SELECT * FROM audit_log ORDER BY id")))


async def refuse_audit_entries(engine: AsyncEngine) -> None:
    """From now on every insert into `audit_log` fails, like a database that cannot write the entry; the transaction
    that tried it fails with it."""
    async with engine.begin() as connection:
        await connection.execute(
            text(
                "CREATE FUNCTION refuse_audit_entry() RETURNS trigger LANGUAGE plpgsql "
                "AS $$ BEGIN RAISE EXCEPTION 'the audit entry cannot be written'; END $$"
            )
        )
        await connection.execute(
            text(
                "CREATE TRIGGER refuse_audit_entry BEFORE INSERT ON audit_log "
                "FOR EACH ROW EXECUTE FUNCTION refuse_audit_entry()"
            )
        )
