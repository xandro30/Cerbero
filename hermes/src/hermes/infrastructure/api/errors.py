"""The answer of each failure in the envelope, with the HTTP status of its code (contracts/api.md)."""

from __future__ import annotations

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from starlette.exceptions import HTTPException

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.envelope import failed_response


def _state(request: Request) -> dict[str, object]:
    state: dict[str, object] = request.scope.setdefault("state", {})
    return state


async def on_hermes_error(request: Request, error: Exception) -> JSONResponse:
    failure = error if isinstance(error, HermesError) else HermesError(ErrorCode.INTERNAL, "an unexpected failure")
    return failed_response(_state(request), failure)


async def on_request_validation(request: Request, error: Exception) -> JSONResponse:
    items = error.errors() if isinstance(error, RequestValidationError) else ()
    details = [
        {"path": ".".join(str(part) for part in item.get("loc", ())), "message": str(item.get("msg", ""))}
        for item in items
    ]
    failure = HermesError(ErrorCode.VALIDATION, "the request does not follow its contract", details=details)
    return failed_response(_state(request), failure)


async def on_http_error(request: Request, error: Exception) -> JSONResponse:
    status = error.status_code if isinstance(error, HTTPException) else 500
    if status in (404, 405):
        failure = HermesError(ErrorCode.NOT_FOUND, "no such operation")
    elif status in (400, 413, 422):
        failure = HermesError(ErrorCode.VALIDATION, "the request does not follow its contract")
    else:
        failure = HermesError(ErrorCode.INTERNAL, "an unexpected failure")
    return failed_response(_state(request), failure)


def install_handlers(app: FastAPI) -> None:
    app.add_exception_handler(HermesError, on_hermes_error)
    app.add_exception_handler(RequestValidationError, on_request_validation)
    app.add_exception_handler(HTTPException, on_http_error)
