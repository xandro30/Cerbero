"""The port of a chat provider library: it draws the neutral message, sends it, gives what reaches its bot and tests
the connection of an instance."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Protocol

from hermes.domain.configuration.model import Instance
from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue


@dataclass(frozen=True)
class Rendered:
    """The text that the provider receives, and whether its detail was cut to fit (FR-018)."""

    text: str
    truncated: bool


@dataclass(frozen=True)
class Delivery:
    """One message to one channel; the address is personal data and the credentials are secrets."""

    instance: Instance
    credentials: Mapping[str, SecretValue]
    address: str = field(repr=False)
    text: str = field(repr=False)


@dataclass(frozen=True)
class SentMessage:
    provider_message_ref: str


@dataclass(frozen=True)
class IncomingMessage:
    """A message that reached the bot, in the order it arrived."""

    chat: str = field(repr=False)
    chat_type: str
    text: str = field(repr=False)


class ProviderFailure(HermesError):
    """A failure of the provider: it goes inside the envelope with HTTP 200 (contracts/api.md#estado-http).

    Its message never carries the URL of the provider, a secret, an address or the text that the provider answered.
    """

    def __init__(
        self,
        code: ErrorCode,
        message: str,
        *,
        retryable: bool | None = None,
        retry_after_s: int | None = None,
        outcome_unknown: bool | None = None,
        credentials_rejected: bool = False,
    ) -> None:
        super().__init__(
            code,
            message,
            retryable=retryable,
            retry_after_s=retry_after_s,
            outcome_unknown=outcome_unknown,
            from_provider=True,
        )
        self.credentials_rejected = credentials_rejected


class ChatProvider(Protocol):
    @property
    def manifest(self) -> Manifest: ...

    def render(self, message: NeutralMessage) -> Rendered:
        """The message in the format of the provider, within its maximum length."""
        ...

    async def send(self, delivery: Delivery) -> SentMessage:
        """The message sent, or a `ProviderFailure`."""
        ...

    async def receive(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> list[IncomingMessage]:
        """What reached the bot of an instance since the last call, each message once and in order."""
        ...

    async def check_connection(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> None:
        """Nothing if the provider accepts the credentials of the instance (FR-014), or a `ProviderFailure`."""
        ...


class ChatProviders(Protocol):
    def chat(self, provider: str) -> ChatProvider | None:
        """The library of a provider, if Hermes loaded it."""
        ...

    def manifests(self) -> tuple[Manifest, ...]:
        """The manifests of the libraries that Hermes loaded, by the name of their provider."""
        ...
