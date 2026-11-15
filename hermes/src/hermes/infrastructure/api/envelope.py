"""The common envelope of Cerbero (`common.v1.json` 1.1.0): status, data, error and meta."""

from __future__ import annotations

import time
from collections.abc import Mapping, MutableMapping
from typing import Any

from fastapi import Request
from fastapi.responses import JSONResponse

from hermes.domain.shared.errors import ErrorCode, HermesError

STATUS = {
    ErrorCode.VALIDATION: 400,
    ErrorCode.AUTH: 401,
    ErrorCode.NOT_FOUND: 404,
    ErrorCode.CONFLICT: 409,
    ErrorCode.RATE_LIMITED: 429,
    ErrorCode.INTERNAL: 500,
    ErrorCode.UNAVAILABLE: 503,
}
META_FIELDS = ("service", "provider", "idempotency_key")


def http_status(error: HermesError) -> int:
    """200 for a failure of the provider, which goes inside the envelope; the status of its code otherwise."""
    if error.from_provider:
        return 200
    return STATUS.get(error.code, 500)


def set_meta(
    request: Request, *, service: str | None = None, provider: str | None = None, idempotency_key: str | None = None
) -> None:
    """The optional fields of `meta` for the answer of this request."""
    for name, value in (("service", service), ("provider", provider), ("idempotency_key", idempotency_key)):
        if value is not None:
            request.scope.setdefault("state", {})[f"meta_{name}"] = value


def meta(state: Mapping[str, Any]) -> dict[str, Any]:
    started = state.get("started", time.monotonic())
    out: dict[str, Any] = {
        "request_id": state.get("request_id", "unknown"),
        "duration_ms": max(0, int((time.monotonic() - started) * 1000)),
    }
    for name in META_FIELDS:
        value = state.get(f"meta_{name}")
        if value is not None:
            out[name] = value
    return out


def failed_body(state: Mapping[str, Any], error: HermesError) -> dict[str, Any]:
    detail: dict[str, Any] = {"code": error.code.value, "message": error.message, "retryable": error.retryable}
    if error.retry_after_s is not None:
        detail["retry_after_s"] = error.retry_after_s
    if error.outcome_unknown is not None:
        detail["outcome_unknown"] = error.outcome_unknown
    data: dict[str, Any] = {"errors": error.details} if error.details else {}
    return {"status": "failed", "data": data, "error": detail, "meta": meta(state)}


def record_result(state: MutableMapping[str, Any], body: Mapping[str, Any]) -> None:
    """How the request ended, for its line of log: `completed`, or the code of its error."""
    error = body.get("error")
    code = error.get("code") if isinstance(error, Mapping) else None
    state["result"] = "completed" if body.get("status") == "completed" else code


def failed_response(state: MutableMapping[str, Any], error: HermesError) -> JSONResponse:
    """`Retry-After` goes with a 429 or a 503; a failure of the provider carries its wait inside the envelope."""
    status = http_status(error)
    headers = {"Retry-After": str(error.retry_after_s)} if error.retry_after_s is not None and status != 200 else None
    body = failed_body(state, error)
    record_result(state, body)
    return JSONResponse(body, status_code=status, headers=headers)


def completed_response(request: Request, data: Mapping[str, Any], status_code: int = 200) -> JSONResponse:
    state = request.scope.setdefault("state", {})
    body = {"status": "completed", "data": dict(data), "error": None, "meta": meta(state)}
    record_result(state, body)
    return JSONResponse(body, status_code)
