"""`GET /v1/capabilities` and `POST /v1/instances/{name}/test` (contracts/api.md, FR-013, FR-014).

Both go through the authentication of the network API. The test changes nothing outside Hermes, so it takes no
`Idempotency-Key`; a failure of the provider goes inside the envelope with HTTP 200 and leaves the instance unavailable.
Until `install_capabilities` gives the routes their services, they answer `unavailable`.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC
from typing import Annotated

from fastapi import APIRouter, Depends, FastAPI, Request
from fastapi.responses import JSONResponse

from hermes.application.capabilities import Capabilities
from hermes.application.ports import Caller
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.names import is_name
from hermes.infrastructure.api.auth import authenticated
from hermes.infrastructure.api.envelope import completed_response, set_meta
from hermes.infrastructure.api.middleware import recorded_operation

INSTANT = "%Y-%m-%dT%H:%M:%S.%fZ"
NAME_RULE = "the name of an instance follows ^[a-z][a-z0-9_]{0,62}$"


@dataclass(frozen=True)
class CapabilitiesServices:
    capabilities: Capabilities


def install_capabilities(app: FastAPI, services: CapabilitiesServices) -> None:
    app.state.capabilities = services


def _services(request: Request) -> CapabilitiesServices:
    services = getattr(request.app.state, "capabilities", None)
    if not isinstance(services, CapabilitiesServices) or not services.capabilities.ready:
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes is not ready")
    return services


router = APIRouter(dependencies=[Depends(recorded_operation)])


@router.get("/v1/capabilities", operation_id="getCapabilities")
async def get_capabilities(request: Request, caller: Annotated[Caller, Depends(authenticated)]) -> JSONResponse:
    return completed_response(request, _services(request).capabilities.describe())


@router.post("/v1/instances/{name}/test", operation_id="testInstance")
async def instance_test(request: Request, name: str, caller: Annotated[Caller, Depends(authenticated)]) -> JSONResponse:
    services = _services(request)
    if not is_name(name):
        raise HermesError(ErrorCode.VALIDATION, NAME_RULE, details=[{"path": "name", "message": NAME_RULE}])
    set_meta(request, provider=services.capabilities.provider_of(name))
    result = await services.capabilities.test(name)
    data = {
        "instance": result.instance,
        "status": result.status.value,
        "checked_at": result.checked_at.astimezone(UTC).strftime(INSTANT),
    }
    return completed_response(request, data)
