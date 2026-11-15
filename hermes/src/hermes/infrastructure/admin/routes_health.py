"""`health` of the local channel (contracts/admin-channel.md, FR-015): it answers while the process runs, ready or not.

The healthcheck of the container asks for it often, so its line of log goes at debug level while Hermes is ready, and
at info level with the reason when it is not.
"""

from __future__ import annotations

import logging
from typing import Any, Protocol

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from hermes.application.health import Health
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.envelope import completed_response

logger = logging.getLogger(__name__)


class HealthReporter(Protocol):
    async def check(self) -> Health: ...


router = APIRouter()


@router.get("/health")
async def health(request: Request) -> JSONResponse:
    services = getattr(request.app.state, "admin", None)
    reporter: HealthReporter | None = getattr(services, "health", None)
    if reporter is None:
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes has no health check yet")
    result = await reporter.check()
    data: dict[str, Any] = {"live": result.live, "ready": result.ready}
    if result.reason is not None:
        data["reason"] = result.reason
        logger.info("health", extra={"fields": {"result": "not_ready", "reason": result.reason}})
    else:
        logger.debug("health", extra={"fields": {"result": "ready"}})
    return completed_response(request, data)
