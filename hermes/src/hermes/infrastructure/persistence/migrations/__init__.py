"""The migrations of Hermes, written by hand, that `hermes serve` applies when it starts (research R6)."""

from __future__ import annotations

from pathlib import Path

from alembic import command
from alembic.config import Config
from sqlalchemy import Connection, text
from sqlalchemy.ext.asyncio import AsyncEngine

MIGRATION_LOCK = 0x4865726D6573


def _config(connection: Connection) -> Config:
    config = Config()
    config.set_main_option("script_location", str(Path(__file__).parent))
    config.attributes["connection"] = connection
    return config


def _upgrade(connection: Connection) -> None:
    command.upgrade(_config(connection), "head")


async def apply_migrations(engine: AsyncEngine) -> None:
    """Up to the latest migration, in one transaction and under an advisory lock, so two starts never collide."""
    async with engine.begin() as connection:
        await connection.execute(text("SELECT pg_advisory_xact_lock(:key)"), {"key": MIGRATION_LOCK})
        await connection.run_sync(_upgrade)
