"""`identity.enroll` of the local channel (contracts/admin-channel.md): the token of the admin, never in a log."""

from __future__ import annotations

import logging
from typing import Protocol

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from hermes.application.identity import TOKEN, TOKEN_RULE
from hermes.application.ports import IssuedCertificate
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.api.envelope import completed_response
from hermes.infrastructure.api.middleware import read_json

DATE = "%Y-%m-%dT%H:%M:%SZ"

logger = logging.getLogger(__name__)


class Enroller(Protocol):
    async def enroll(self, token: SecretValue) -> IssuedCertificate: ...


def _enroller(request: Request) -> Enroller:
    services = getattr(request.app.state, "admin", None)
    if services is None:
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes is starting")
    identity: Enroller = services.identity
    return identity


router = APIRouter()


@router.post("/identity/enroll")
async def enroll(request: Request) -> JSONResponse:
    body = await read_json(request)
    token = body.get("token") if isinstance(body, dict) else None
    if not isinstance(token, str) or set(body) != {"token"} or TOKEN.fullmatch(token) is None:
        logger.info("identity.enroll", extra={"fields": {"result": ErrorCode.VALIDATION.value}})
        raise HermesError(ErrorCode.VALIDATION, TOKEN_RULE, details=[{"path": "token", "message": TOKEN_RULE}])
    try:
        issued = await _enroller(request).enroll(SecretValue(token))
    except HermesError as error:
        logger.info("identity.enroll", extra={"fields": {"result": error.code.value}})
        raise
    logger.info("identity.enroll", extra={"fields": {"result": "completed", "spiffe_id": str(issued.spiffe_id)}})
    data = {"spiffe_id": str(issued.spiffe_id), "serial": issued.serial, "not_after": issued.not_after.strftime(DATE)}
    return completed_response(request, data)
