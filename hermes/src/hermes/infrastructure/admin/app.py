"""The app of the local administration channel: the common envelope, without API Contract (principle II)."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

from fastapi import APIRouter, FastAPI

from hermes.infrastructure.admin.routes_config import CodeOperations, ConfigurationOperations
from hermes.infrastructure.admin.routes_config import router as config_router
from hermes.infrastructure.admin.routes_health import HealthReporter
from hermes.infrastructure.admin.routes_health import router as health_router
from hermes.infrastructure.admin.routes_identity import Enroller
from hermes.infrastructure.admin.routes_identity import router as identity_router
from hermes.infrastructure.api.app import create_api

MAX_BODY = 8 * 1024 * 1024


@dataclass(frozen=True)
class AdminServices:
    identity: Enroller
    configuration: ConfigurationOperations | None = None
    verification: CodeOperations | None = None
    health: HealthReporter | None = None


def install_admin(app: FastAPI, services: AdminServices) -> None:
    app.state.admin = services


def create_admin_app(*, routers: Sequence[APIRouter] = ()) -> FastAPI:
    """Served on `HERMES_ADMIN_DIR/admin.sock`; its security is the local access to that directory. It takes bodies of
    up to 8 MiB, because a configuration file of up to 1 MiB goes inside JSON."""
    return create_api(routers=[identity_router, config_router, health_router, *routers], max_body=MAX_BODY)
