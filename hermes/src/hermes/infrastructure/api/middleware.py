"""The context of each request: its X-Request-Id, the limit of its body, the envelope of any unexpected failure and,
on the network API, its line of log when it ends (research R19)."""

from __future__ import annotations

import json
import logging
import re
import time
import uuid
from collections.abc import Callable, MutableMapping
from typing import Any, NoReturn

from fastapi import Request
from starlette.datastructures import Headers
from starlette.types import ASGIApp, Message, Receive, Scope, Send

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.envelope import failed_response
from hermes.infrastructure.logging import request_logged_as, stamp_request_ids

MAX_BODY = 64 * 1024
REQUEST_ID = re.compile(r"[\x20-\x7e]{1,128}")
REQUEST_ID_RULE = "X-Request-Id must have from 1 to 128 printable ASCII characters"
MIB = 1024 * 1024


def body_rule(max_body: int) -> str:
    size = f"{max_body // MIB} MiB" if max_body % MIB == 0 else f"{max_body // 1024} KiB"
    return f"the body must be standard JSON in UTF-8 of up to {size}"


BODY_RULE = body_rule(MAX_BODY)

logger = logging.getLogger("hermes.api")


class RequestContextMiddleware:
    """`max_body` is 64 KiB on the network API; the local channel takes more, because it carries whole files.

    Every line created during a request carries its `request_id`. With `log_requests`, which the network API uses, the
    request leaves one more line when it ends, `api.request`; the local channel leaves its own line per operation.
    """

    def __init__(self, app: ASGIApp, max_body: int = MAX_BODY, log_requests: bool = False) -> None:
        self.app = app
        self.max_body = max_body
        self.rule = body_rule(max_body)
        self.log_requests = log_requests
        stamp_request_ids()

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return
        state = scope.setdefault("state", {})
        state["started"] = time.monotonic()
        state["request_id"] = f"req-{uuid.uuid4().hex}"
        state["body_rule"] = self.rule
        headers = Headers(scope=scope)
        given = headers.get("x-request-id")
        refusal: HermesError | None = None
        if given is not None and REQUEST_ID.fullmatch(given) is None:
            refusal = HermesError(ErrorCode.VALIDATION, REQUEST_ID_RULE)
        elif given is not None:
            state["request_id"] = given
        length = headers.get("content-length")
        if refusal is None and length is not None and (not length.isdigit() or int(length) > self.max_body):
            refusal = HermesError(ErrorCode.VALIDATION, self.rule)
        status: int | None = None

        async def tracked_send(message: Message) -> None:
            nonlocal status
            if message["type"] == "http.response.start":
                status = message["status"]
            await send(message)

        with request_logged_as(state["request_id"]):
            try:
                if refusal is not None:
                    await failed_response(state, refusal)(scope, receive, tracked_send)
                else:
                    await self._serve(scope, receive, tracked_send, state, lambda: status is not None)
            finally:
                if self.log_requests:
                    _log_request(state, status)

    async def _serve(
        self, scope: Scope, receive: Receive, send: Send, state: MutableMapping[str, Any], responded: Callable[[], bool]
    ) -> None:
        received = 0

        async def limited_receive() -> Message:
            nonlocal received
            message = await receive()
            if message["type"] == "http.request":
                received += len(message.get("body", b""))
                if received > self.max_body:
                    raise HermesError(ErrorCode.VALIDATION, self.rule)
            return message

        try:
            await self.app(scope, limited_receive, send)
        except HermesError as error:
            if not responded():
                await failed_response(state, error)(scope, receive, send)
        except Exception as error:
            logger.error(
                "request.failed",
                extra={"fields": {"request_id": state["request_id"], "exception": type(error).__name__}},
            )
            if not responded():
                failure = HermesError(ErrorCode.INTERNAL, "an unexpected failure")
                await failed_response(state, failure)(scope, receive, send)


def _log_request(state: MutableMapping[str, Any], status: int | None) -> None:
    """The line of a request of the network API: who asked what and how it ended, never with a contact, an address,
    a token or a certificate; the `Idempotency-Key` only once it follows its rule."""
    fields = {
        "request_id": state["request_id"],
        "operation": state.get("operation"),
        "caller": state.get("caller"),
        "idempotency_key": state.get("meta_idempotency_key"),
        "result": state.get("result"),
        "status": status,
        "duration_ms": max(0, int((time.monotonic() - state["started"]) * 1000)),
    }
    logger.info("api.request", extra={"fields": fields})


def recorded_operation(request: Request) -> None:
    """A dependency of the routers of the network API: the operation of the route, as hermes.v1.json names it, for
    the line of the request. It runs before the authentication, so a refused call names its operation too."""
    route = request.scope.get("route")
    operation = getattr(route, "operation_id", None)
    if operation is not None:
        request.scope.setdefault("state", {})["operation"] = operation


def _no_constants(name: str) -> NoReturn:
    raise ValueError(f"{name} is not standard JSON")


def _unique_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        if key in out:
            raise ValueError("a key repeats")
        out[key] = value
    return out


async def read_json(request: Request) -> Any:
    """The body as standard JSON: UTF-8, without NaN, Infinity or repeated keys, within the limit of the app."""
    raw = await request.body()
    try:
        return json.loads(raw.decode("utf-8"), parse_constant=_no_constants, object_pairs_hook=_unique_keys)
    except UnicodeDecodeError, ValueError:
        raise HermesError(ErrorCode.VALIDATION, request.scope.get("state", {}).get("body_rule", BODY_RULE)) from None
