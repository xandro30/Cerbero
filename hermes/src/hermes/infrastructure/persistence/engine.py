"""The asynchronous engine on the shared PostgreSQL (ADR 0012); the password never travels in the URL."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from sqlalchemy import event
from sqlalchemy.exc import InterfaceError, OperationalError
from sqlalchemy.exc import TimeoutError as PoolTimeout
from sqlalchemy.ext.asyncio import AsyncEngine, create_async_engine

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue

DATABASE_DOWN = (OperationalError, InterfaceError, PoolTimeout, OSError)


def database_unavailable() -> HermesError:
    """The database does not answer: `unavailable`, worth retrying; Hermes sends nothing it cannot record."""
    return HermesError(ErrorCode.UNAVAILABLE, "the database does not answer")


def create_engine(url: str, password: Callable[[], SecretValue]) -> AsyncEngine:
    """An engine whose every new connection asks `password` for the current value of the secret (FR-009, FR-023)."""
    engine = create_async_engine(url, pool_pre_ping=True, pool_size=5, max_overflow=5)

    @event.listens_for(engine.sync_engine, "do_connect")
    def _with_password(dialect: Any, record: Any, cargs: Any, cparams: dict[str, Any]) -> None:
        cparams["password"] = password().reveal()

    return engine
