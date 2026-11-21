"""chat.send_message (contracts/api.md#orden-y-errores).

`prepare` resolves the template, the contact and its channel, validates the data, composes the message and checks that
the channel is verified, without calling the provider. `execute` takes a token of the limit of the instance, resolves
its secrets, draws the message and sends it; each send that reaches the provider leaves its audit entry, completed or
failed (FR-029). `execute_once` does the same with the row of its `Idempotency-Key`: reserved right before the
provider with the entry of an unknown outcome, written if a cut leaves the send without its record; completed with
what was sent or may have been sent, its response and its audit entry in one transaction (research R11); and released
with its audit entry after a failure that guarantees nothing was sent, which is raised like a failure before the
provider (research R2 of 012). An unexpected failure while calling the provider is an unknown outcome: Hermes does not
know whether the message left (research R3 of 012).
"""

from __future__ import annotations

import logging
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Protocol

from jsonschema import Draft202012Validator
from jsonschema.exceptions import ValidationError

from hermes.application.capabilities import InstanceStatuses
from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Reservation
from hermes.application.ports import (
    AuditEntry,
    AuditKind,
    AuditLog,
    Caller,
    ChannelKey,
    ChannelRepository,
    SecretSource,
)
from hermes.application.rate_limits import RateLimits
from hermes.domain.configuration.model import Instance
from hermes.domain.idempotency.record import StoredResponse
from hermes.domain.messages.neutral import NeutralMessage, compose
from hermes.domain.services.chat.channels import choose_channel
from hermes.domain.services.chat.contract import SEND_MESSAGE, SERVICE, SendRequest, resolve_template
from hermes.domain.services.chat.provider import ChatProvider, ChatProviders, Delivery, ProviderFailure
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError

COMPLETED: Mapping[str, Any] = {"status": "completed", "code": None, "outcome_unknown": False}
UNKNOWN: Mapping[str, Any] = {"status": "failed", "code": ErrorCode.TIMEOUT.value, "outcome_unknown": True}
UNEXPECTED_WHILE_SENDING = "an unexpected failure while sending: the message may have arrived"

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class PreparedSend:
    """A send that passed every check before the provider; the address of the channel never shows."""

    caller: Caller
    idempotency_key: str
    contact: str
    instance: Instance
    message: NeutralMessage
    provider: ChatProvider = field(repr=False)
    address: str = field(repr=False)

    @property
    def provider_name(self) -> str:
        return self.instance.provider


@dataclass(frozen=True)
class SendResult:
    recipient: str
    instance: str
    provider: str
    provider_message_ref: str
    sent_at: datetime
    truncated: bool


def _path(error: ValidationError) -> str:
    return "data" + "".join(f"[{part}]" if isinstance(part, int) else f".{part}" for part in error.absolute_path)


def _check_data(schema: Mapping[str, Any], data: Mapping[str, Any]) -> None:
    validator = Draft202012Validator(schema, format_checker=Draft202012Validator.FORMAT_CHECKER)
    errors = sorted(validator.iter_errors(data), key=lambda error: [str(part) for part in error.absolute_path])
    if errors:
        details = [{"path": _path(error), "message": error.message} for error in errors]
        raise HermesError(ErrorCode.VALIDATION, "the data do not fit the schema of the template", details=details)


def _failed(error: HermesError) -> Mapping[str, Any]:
    return {"status": "failed", "code": error.code.value, "outcome_unknown": bool(error.outcome_unknown)}


Render = Callable[[SendResult | HermesError], StoredResponse]


class SendRecord(Protocol):
    """Where a send leaves its trace: right before the provider, with the entry of an unknown outcome, and with its
    result and its audit entry."""

    async def before_send(self, unknown: AuditEntry) -> None: ...

    async def after_send(self, outcome: SendResult | HermesError, entry: AuditEntry) -> None: ...


class _Audited:
    def __init__(self, audit: AuditLog) -> None:
        self._audit = audit

    async def before_send(self, unknown: AuditEntry) -> None:
        return None

    async def after_send(self, outcome: SendResult | HermesError, entry: AuditEntry) -> None:
        await self._audit.append(entry)


class _Reserved:
    def __init__(self, reservation: Reservation, render: Render) -> None:
        self._reservation = reservation
        self._render = render

    async def before_send(self, unknown: AuditEntry) -> None:
        await self._reservation.reserve(unknown)

    async def after_send(self, outcome: SendResult | HermesError, entry: AuditEntry) -> None:
        if isinstance(outcome, HermesError) and not outcome.outcome_unknown:
            await self._reservation.release(entry)
            return
        await self._reservation.complete(self._render(outcome), entry)


class SendMessage:
    def __init__(
        self,
        *,
        configuration: ConfigurationState,
        channels: ChannelRepository,
        providers: ChatProviders,
        secrets: SecretSource,
        audit: AuditLog,
        clock: Clock,
        limits: RateLimits | None = None,
        statuses: InstanceStatuses | None = None,
    ) -> None:
        self._configuration = configuration
        self._channels = channels
        self._providers = providers
        self._secrets = secrets
        self._audit = audit
        self._clock = clock
        self._limits = limits if limits is not None else RateLimits(clock)
        self._statuses = statuses if statuses is not None else InstanceStatuses()

    @property
    def ready(self) -> bool:
        return self._configuration.loaded

    async def send(self, caller: Caller, request: SendRequest, idempotency_key: str) -> SendResult:
        return await self.execute(await self.prepare(caller, request, idempotency_key))

    async def prepare(self, caller: Caller, request: SendRequest, idempotency_key: str) -> PreparedSend:
        configuration = self._configuration.current()
        version = resolve_template(configuration, request)
        if request.instance is not None and configuration.instance(request.instance) is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the instance does not exist")
        contact = configuration.contact(request.recipient)
        if contact is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the contact does not exist")
        channel = choose_channel(contact, request.instance)
        instance = configuration.instance(channel.instance)
        if instance is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the instance does not exist")
        provider = self._providers.chat(instance.provider)
        if provider is None:
            raise HermesError(ErrorCode.INTERNAL, "hermes has no library for the provider of the instance")
        _check_data(version.data_schema, request.data)
        message = compose(version.message, request.data, configuration.variable_values())
        state = await self._channels.state(ChannelKey(contact.name, SERVICE, channel.provider, channel.instance))
        if state is None or not state.verified or state.address is None:
            raise HermesError(ErrorCode.CONFLICT, "the channel of the contact is not verified")
        return PreparedSend(
            caller=caller,
            idempotency_key=idempotency_key,
            contact=contact.name,
            instance=instance,
            message=message,
            provider=provider,
            address=state.address,
        )

    async def execute(self, prepared: PreparedSend) -> SendResult:
        return await self._execute(prepared, _Audited(self._audit))

    async def execute_once(self, prepared: PreparedSend, reservation: Reservation, render: Render) -> StoredResponse:
        """The response that the key keeps: a completed send or an unknown outcome. A failure that guarantees nothing
        was sent releases the key and is raised, like a failure before the provider, which leaves no row."""
        try:
            await self._execute(prepared, _Reserved(reservation, render))
        except Exception:
            if reservation.response is None:
                raise
        if reservation.response is None:
            raise HermesError(ErrorCode.INTERNAL, "the send left no response")
        return reservation.response

    async def _execute(self, prepared: PreparedSend, record: SendRecord) -> SendResult:
        per_minute = prepared.instance.requests_per_minute or prepared.provider.manifest.requests_per_minute
        self._limits.take(prepared.instance.name, per_minute)
        try:
            credentials = {
                name: await self._secrets.resolve(reference) for name, reference in prepared.instance.secrets.items()
            }
        except HermesError as error:
            if error.code is ErrorCode.INTERNAL:
                raise
            raise HermesError(ErrorCode.INTERNAL, "the secrets of the instance are not available") from None
        rendered = prepared.provider.render(prepared.message)
        delivery = Delivery(
            instance=prepared.instance, credentials=credentials, address=prepared.address, text=rendered.text
        )
        await record.before_send(self._entry(prepared, UNKNOWN))
        try:
            sent = await prepared.provider.send(delivery)
        except HermesError as error:
            self._statuses.record_call(prepared.instance.name, error)
            await record.after_send(error, self._entry(prepared, _failed(error)))
            raise
        except Exception as error:
            logger.error("send.failed", extra={"fields": {"exception": type(error).__name__}})
            failure = ProviderFailure(
                ErrorCode.TIMEOUT, UNEXPECTED_WHILE_SENDING, retryable=False, outcome_unknown=True
            )
            await record.after_send(failure, self._entry(prepared, _failed(failure)))
            raise
        self._statuses.record_call(prepared.instance.name, None)
        sent_at = self._clock.now()
        result = SendResult(
            recipient=prepared.contact,
            instance=prepared.instance.name,
            provider=prepared.provider_name,
            provider_message_ref=sent.provider_message_ref,
            sent_at=sent_at,
            truncated=rendered.truncated,
        )
        await record.after_send(result, self._entry(prepared, COMPLETED, at=sent_at))
        return result

    def _entry(self, prepared: PreparedSend, outcome: Mapping[str, Any], *, at: datetime | None = None) -> AuditEntry:
        values = {
            "recipient": prepared.contact,
            "title": prepared.message.title,
            "instance": prepared.instance.name,
            "provider": prepared.provider_name,
        }
        return AuditEntry(
            at=at or self._clock.now(),
            kind=AuditKind.SEND,
            actor=str(prepared.caller.spiffe_id),
            outcome=outcome,
            function=SEND_MESSAGE.qualified_name,
            instance=prepared.instance.name,
            recipients=(prepared.contact,),
            side_effect=SEND_MESSAGE.side_effect_for(values),
            discloses_data=SEND_MESSAGE.discloses_data_for(values),
            audit_context=prepared.caller.audit_context,
            idempotency_key=prepared.idempotency_key,
        )
