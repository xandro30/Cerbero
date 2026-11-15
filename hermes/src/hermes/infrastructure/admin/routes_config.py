"""`config.plan`, `config.apply`, `config.export` and `contacts.code` of the local channel (contracts/admin-channel.md).

Each operation leaves one line of log with its name and its result, never with a code or an address. Until Hermes has
its database, they answer `unavailable`.
"""

from __future__ import annotations

import logging
from collections.abc import Mapping
from dataclasses import asdict
from typing import Any, Protocol

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from hermes.application.configuration import Plan
from hermes.application.verification import IssuedCode
from hermes.domain.configuration.diff import Change
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.envelope import completed_response
from hermes.infrastructure.api.middleware import read_json

DATE = "%Y-%m-%dT%H:%M:%SZ"

logger = logging.getLogger(__name__)


class ConfigurationOperations(Protocol):
    async def plan(self, text: str) -> Plan: ...

    async def apply(self, text: str, digest: str) -> tuple[Change, ...]: ...

    async def export(self) -> str: ...


class CodeOperations(Protocol):
    async def issue_code(self, contact: str, instance: str) -> IssuedCode: ...


def _service(request: Request, name: str) -> Any:
    services = getattr(request.app.state, "admin", None)
    service = getattr(services, name, None) if services is not None else None
    if service is None:
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes is starting: it does not have its database yet")
    return service


async def _body(request: Request, fields: tuple[str, ...]) -> Mapping[str, str]:
    body = await read_json(request)
    if not isinstance(body, dict) or set(body) != set(fields) or not all(isinstance(body[f], str) for f in fields):
        rule = "the body must have exactly the text fields " + ", ".join(fields)
        raise HermesError(ErrorCode.VALIDATION, rule, details=[{"path": ".", "message": rule}])
    return body


def _changes(changes: tuple[Change, ...]) -> list[dict[str, Any]]:
    return [asdict(change) for change in changes]


def _logged(operation: str, result: str, **fields: Any) -> None:
    logger.info(operation, extra={"fields": {"result": result, **fields}})


router = APIRouter()


@router.post("/config/plan")
async def config_plan(request: Request) -> JSONResponse:
    try:
        body = await _body(request, ("yaml",))
        plan = await _service(request, "configuration").plan(body["yaml"])
    except HermesError as error:
        _logged("config.plan", error.code.value)
        raise
    _logged("config.plan", "completed", changes=len(plan.changes))
    return completed_response(request, {"changes": _changes(plan.changes), "plan_digest": plan.digest})


@router.post("/config/apply")
async def config_apply(request: Request) -> JSONResponse:
    try:
        body = await _body(request, ("yaml", "plan_digest"))
        applied = await _service(request, "configuration").apply(body["yaml"], body["plan_digest"])
    except HermesError as error:
        _logged("config.apply", error.code.value)
        raise
    _logged("config.apply", "completed", changes=len(applied))
    return completed_response(request, {"changes": _changes(applied)})


@router.get("/config/export")
async def config_export(request: Request) -> JSONResponse:
    try:
        exported = await _service(request, "configuration").export()
    except HermesError as error:
        _logged("config.export", error.code.value)
        raise
    _logged("config.export", "completed")
    return completed_response(request, {"yaml": exported})


@router.post("/contacts/code")
async def contacts_code(request: Request) -> JSONResponse:
    try:
        body = await _body(request, ("contact", "instance"))
        issued = await _service(request, "verification").issue_code(body["contact"], body["instance"])
    except HermesError as error:
        _logged("contacts.code", error.code.value)
        raise
    _logged("contacts.code", "completed")
    return completed_response(request, {"code": issued.code, "expires_at": issued.expires_at.strftime(DATE)})
