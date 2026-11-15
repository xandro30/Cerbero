"""The fake chat provider, for tests only: it keeps what it sends and fails on demand (research R17, contracts/fake.md).

With `HERMES_FAKE_DIR`, each send goes to `<dir>/<instance>/outbox.jsonl` and each file left in
`<dir>/<instance>/inbox/` is delivered once, in the order of its name; without it, everything stays in memory. Each
failure that it is asked for is one attempt, and what did not send is retried as in Telegram (research R10).
"""

from __future__ import annotations

import asyncio
import itertools
import json
import logging
import random
from collections import defaultdict, deque
from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path

from hermes.domain.configuration.model import Instance
from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.contract import SEND_MESSAGE, SERVICE
from hermes.domain.services.chat.drawing import PLAIN, draw
from hermes.domain.services.chat.provider import Delivery, IncomingMessage, ProviderFailure, Rendered, SentMessage
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.providers.retry import NotSent, RetryTiming, send_with_retries

INSTANT = "%Y-%m-%dT%H:%M:%S.%fZ"
RETRY_AFTER_S = 60

logger = logging.getLogger(__name__)


class FakeFailure(StrEnum):
    UNREACHABLE = "unreachable"
    RATE_LIMITED = "rate_limited"
    CUT_AFTER_SEND = "cut_after_send"
    CREDENTIALS_REJECTED = "credentials_rejected"
    RECIPIENT_REFUSES = "recipient_refuses"


_FAILURES = {
    FakeFailure.CUT_AFTER_SEND: lambda: ProviderFailure(
        ErrorCode.TIMEOUT,
        "the answer of the fake provider was lost after sending: the message may have arrived",
        retryable=False,
        outcome_unknown=True,
    ),
    FakeFailure.CREDENTIALS_REJECTED: lambda: ProviderFailure(
        ErrorCode.PROVIDER_UNAVAILABLE,
        "the fake provider does not accept the credentials of the instance",
        retryable=False,
        credentials_rejected=True,
    ),
    FakeFailure.RECIPIENT_REFUSES: lambda: ProviderFailure(
        ErrorCode.CONFLICT, "the person does not accept messages from the bot", retryable=False
    ),
}


@dataclass(frozen=True)
class FakeDelivery:
    instance: str
    chat: str = field(repr=False)
    text: str = field(repr=False)
    provider_message_ref: str
    at: datetime


class FakeProvider:
    def __init__(
        self,
        manifest: Manifest,
        clock: Clock,
        *,
        directory: Path | None = None,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        jitter: Callable[[], float] = random.random,
    ) -> None:
        self.manifest = manifest
        self._clock = clock
        self._directory = directory
        self._sent: list[FakeDelivery] = []
        self._failures: deque[tuple[FakeFailure, int]] = deque()
        self._inbox: defaultdict[str, deque[IncomingMessage]] = defaultdict(deque)
        self._references = itertools.count(1)
        function = manifest.function(SERVICE, SEND_MESSAGE.name)
        self._timeout = float(function.timeout_s if function is not None else SEND_MESSAGE.timeout_s)
        self._timing = RetryTiming(sleep=sleep, monotonic=lambda: clock.now().timestamp(), jitter=jitter)

    def render(self, message: NeutralMessage) -> Rendered:
        return draw(message, max_length=self.manifest.max_length, markup=PLAIN)

    def fail_next(self, failure: FakeFailure, *, times: int = 1, retry_after_s: int = RETRY_AFTER_S) -> None:
        """The next `times` attempts fail this way, and the ones after them work again; a limit asks for
        `retry_after_s`."""
        self._failures.extend([(failure, retry_after_s)] * times)

    def clear_failures(self) -> None:
        self._failures.clear()

    async def send(self, delivery: Delivery) -> SentMessage:
        return await send_with_retries(
            lambda timeout_s: self._attempt(delivery),
            timeout_s=self._timeout,
            provider=self.manifest.provider,
            timing=self._timing,
        )

    async def _attempt(self, delivery: Delivery) -> SentMessage:
        failure, retry_after_s = self._failures.popleft() if self._failures else (None, RETRY_AFTER_S)
        if failure is FakeFailure.UNREACHABLE:
            raise NotSent
        if failure is FakeFailure.RATE_LIMITED:
            raise NotSent(retry_after_s=retry_after_s)
        if failure is not None and failure is not FakeFailure.CUT_AFTER_SEND:
            raise _FAILURES[failure]()
        sent = FakeDelivery(
            instance=delivery.instance.name,
            chat=delivery.address,
            text=delivery.text,
            provider_message_ref=f"fake-{next(self._references)}",
            at=self._clock.now(),
        )
        self._sent.append(sent)
        if self._directory is not None:
            await asyncio.to_thread(self._write_outbox, self._directory, sent)
        if failure is FakeFailure.CUT_AFTER_SEND:
            raise _FAILURES[failure]()
        return SentMessage(sent.provider_message_ref)

    def sent(self, instance: str | None = None) -> list[FakeDelivery]:
        return [item for item in self._sent if instance is None or item.instance == instance]

    def deliver(self, instance: str, *, chat: str, text: str, chat_type: str = "private") -> None:
        """A message that reaches the bot of an instance, from inside the process."""
        self._inbox[instance].append(IncomingMessage(chat=chat, chat_type=chat_type, text=text))

    async def check_connection(
        self, instance: Instance | str, credentials: Mapping[str, SecretValue] | None = None
    ) -> None:
        """The connection test works unless a failure was asked for: rejected credentials, no connection or a limit;
        the others do not apply to a test and are used up."""
        failure, retry_after_s = self._failures.popleft() if self._failures else (None, RETRY_AFTER_S)
        if failure is FakeFailure.CREDENTIALS_REJECTED:
            raise _FAILURES[failure]()
        if failure is FakeFailure.UNREACHABLE:
            raise ProviderFailure(
                ErrorCode.PROVIDER_UNAVAILABLE, "the fake provider did not answer the connection test"
            )
        if failure is FakeFailure.RATE_LIMITED:
            raise ProviderFailure(
                ErrorCode.RATE_LIMITED, "the fake provider limits the instance", retry_after_s=retry_after_s
            )

    async def receive(
        self, instance: Instance | str, credentials: Mapping[str, SecretValue] | None = None
    ) -> list[IncomingMessage]:
        """What reached the bot since the last call, each message once and in order."""
        name = instance if isinstance(instance, str) else instance.name
        received = list(self._inbox.pop(name, ()))
        if self._directory is not None:
            received += await asyncio.to_thread(self._read_inbox, self._directory / name / "inbox")
        return received

    @staticmethod
    def _write_outbox(directory: Path, sent: FakeDelivery) -> None:
        folder = directory / sent.instance
        folder.mkdir(parents=True, exist_ok=True)
        line = {
            "at": sent.at.astimezone(UTC).strftime(INSTANT),
            "chat": sent.chat,
            "text": sent.text,
            "provider_message_ref": sent.provider_message_ref,
        }
        with (folder / "outbox.jsonl").open("a", encoding="utf-8") as outbox:
            outbox.write(json.dumps(line, ensure_ascii=False) + "\n")

    @staticmethod
    def _read_inbox(inbox: Path) -> list[IncomingMessage]:
        if not inbox.is_dir():
            return []
        received: list[IncomingMessage] = []
        for path in sorted(inbox.glob("*.json")):
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except ValueError:
                data = None
            finally:
                path.unlink(missing_ok=True)
            chat_type = data.get("chat_type", "private") if isinstance(data, dict) else None
            if (
                isinstance(data, dict)
                and isinstance(data.get("chat"), str)
                and isinstance(data.get("text"), str)
                and isinstance(chat_type, str)
            ):
                received.append(IncomingMessage(chat=data["chat"], chat_type=chat_type, text=data["text"]))
            else:
                logger.warning("fake.inbox.ignored", extra={"fields": {"file": path.name}})
        return received
