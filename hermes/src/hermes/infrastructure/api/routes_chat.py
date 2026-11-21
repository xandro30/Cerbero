"""`POST /chat/v1/send_message`, the operation of the chat service type (contracts/api.md).

The order of its checks: Hermes ready and the caller (the dependency `authenticated`), the `Idempotency-Key` and the
body, its key (research R11), and then the use case. The key keeps the response of what was sent or may have been
sent, and each repetition gives it back with its own `request_id` and duration; a failure that guarantees nothing was
sent keeps nothing, and the route answers it like a failure before the provider (research R2 of 012). Until
`install_chat` gives the route its services, it answers `unavailable`.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC
from typing import Annotated, Any

from fastapi import APIRouter, Depends, FastAPI, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

from hermes.application.idempotency import Idempotency, Reservation
from hermes.application.ports import Caller
from hermes.application.send_message import SendMessage, SendResult
from hermes.domain.idempotency.key import idempotency_key, request_fingerprint
from hermes.domain.idempotency.record import StoredResponse
from hermes.domain.services.chat.contract import SERVICE, SendRequest
from hermes.domain.services.chat.provider import ProviderFailure
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.auth import authenticated
from hermes.infrastructure.api.envelope import failed_body, http_status, meta, record_result, set_meta
from hermes.infrastructure.api.middleware import read_json, recorded_operation

PATH = "/chat/v1/send_message"
INSTANT = "%Y-%m-%dT%H:%M:%S.%fZ"
UNKNOWN_OUTCOME = "a cut left this request half done: the message may have arrived"

Name = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]{0,62}$")]


def _not_null(value: Any) -> Any:
    if value is None:
        raise ValueError("must not be null: leave it out instead")
    return value


class TemplateBody(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)

    id: Name
    version: int | None = Field(default=None, ge=1)

    _version_not_null = field_validator("version", mode="before")(_not_null)


class SendMessageBody(BaseModel):
    """The request of hermes.v1.json: no field more, and each one with its type."""

    model_config = ConfigDict(extra="forbid", strict=True)

    recipients: list[Name]
    template: TemplateBody
    data: dict[str, Any]
    instance: Name | None = None

    _instance_not_null = field_validator("instance", mode="before")(_not_null)


@dataclass(frozen=True)
class ChatServices:
    send_message: SendMessage
    idempotency: Idempotency


def install_chat(app: FastAPI, services: ChatServices) -> None:
    app.state.chat = services


def _chat_meta(request: Request) -> None:
    set_meta(request, service=SERVICE)


router = APIRouter(dependencies=[Depends(recorded_operation), Depends(_chat_meta)])


def _services(request: Request) -> ChatServices:
    services = getattr(request.app.state, "chat", None)
    if not isinstance(services, ChatServices) or not services.send_message.ready:
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes is not ready")
    return services


def _request(body: Any) -> SendRequest:
    try:
        parsed = SendMessageBody.model_validate(body)
    except ValidationError as error:
        details = [
            {"path": ".".join(str(part) for part in item["loc"]) or "body", "message": item["msg"]}
            for item in error.errors(include_url=False)
        ]
        raise HermesError(ErrorCode.VALIDATION, "the request does not follow its contract", details=details) from None
    return SendRequest(
        recipients=tuple(parsed.recipients),
        template_id=parsed.template.id,
        data=parsed.data,
        template_version=parsed.template.version,
        instance=parsed.instance,
    )


def _data(result: SendResult) -> dict[str, Any]:
    return {
        "recipient": result.recipient,
        "instance": result.instance,
        "provider_message_ref": result.provider_message_ref,
        "sent_at": result.sent_at.astimezone(UTC).strftime(INSTANT),
        "truncated": result.truncated,
    }


def _stored(request: Request, outcome: SendResult | HermesError) -> StoredResponse:
    state = request.scope.setdefault("state", {})
    if isinstance(outcome, SendResult):
        return StoredResponse(200, {"status": "completed", "data": _data(outcome), "error": None, "meta": meta(state)})
    return StoredResponse(http_status(outcome), failed_body(state, outcome))


def _unknown_outcome(request: Request) -> StoredResponse:
    cut = ProviderFailure(ErrorCode.TIMEOUT, UNKNOWN_OUTCOME, retryable=False, outcome_unknown=True)
    return _stored(request, cut)


def _answer(request: Request, stored: StoredResponse) -> JSONResponse:
    """The stored response with the `request_id` and the duration of this request."""
    state = request.scope.setdefault("state", {})
    current = meta(state)
    body = dict(stored.body)
    body["meta"] = {**body.get("meta", {}), "request_id": current["request_id"], "duration_ms": current["duration_ms"]}
    record_result(state, body)
    error = body.get("error")
    wait = error.get("retry_after_s") if isinstance(error, dict) else None
    headers = {"Retry-After": str(wait)} if wait is not None and stored.http_status != 200 else None
    return JSONResponse(body, status_code=stored.http_status, headers=headers)


@router.post(PATH, operation_id="sendMessage")
async def send_message(request: Request, caller: Annotated[Caller, Depends(authenticated)]) -> JSONResponse:
    services = _services(request)
    key = idempotency_key(request.headers.get("idempotency-key"))
    set_meta(request, idempotency_key=key)
    body = await read_json(request)
    send = _request(body)

    async def perform(reservation: Reservation) -> StoredResponse:
        prepared = await services.send_message.prepare(caller, send, key)
        set_meta(request, provider=prepared.provider_name)
        return await services.send_message.execute_once(
            prepared, reservation, lambda outcome: _stored(request, outcome)
        )

    stored = await services.idempotency.run(
        str(caller.spiffe_id),
        key,
        request_fingerprint(request.method, PATH, body),
        perform,
        unknown_outcome=lambda: _unknown_outcome(request),
    )
    return _answer(request, stored)
