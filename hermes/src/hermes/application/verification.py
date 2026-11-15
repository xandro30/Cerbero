"""The verification of the channels (FR-022, research R12).

The admin asks for the code of a channel, which is left unverified, without address and with the SHA-256 of the new
code. The polling of its instance reads what reaches the bot: a private text that matches a pending code of that
instance before it expires binds the channel to that chat, verifies it and gets «Canal verificado.». Anything else gets
no answer and leaves nothing behind. The code and the binding go with their audit entry in one transaction (FR-029).
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass, field
from datetime import datetime

from hermes.application.configuration import ADMIN, ConfigurationState
from hermes.application.ports import AuditEntry, AuditKind, ChannelKey, ChannelRepository, SecretSource
from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.contract import SERVICE
from hermes.domain.services.chat.provider import ChatProvider, ChatProviders, Delivery, IncomingMessage
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.domain.verification.code import VerificationCode, digest_of, expires_at

VERIFIED_REPLY = "Canal verificado."
PRIVATE = "private"
POLL_RETRY_S = 5.0
POLL_PAUSE_S = 1.0

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class IssuedCode:
    contact: str
    instance: str
    code: str = field(repr=False)
    expires_at: datetime


class Verification:
    def __init__(
        self,
        *,
        configuration: ConfigurationState,
        channels: ChannelRepository,
        providers: ChatProviders,
        secrets: SecretSource,
        clock: Clock,
        code_minutes: int,
        new_code: Callable[[], VerificationCode] = VerificationCode.new,
    ) -> None:
        self._configuration = configuration
        self._channels = channels
        self._providers = providers
        self._secrets = secrets
        self._clock = clock
        self._code_minutes = code_minutes
        self._new_code = new_code

    def _library(self, name: str) -> tuple[Instance, ChatProvider]:
        instance = self._configuration.current().instance(name)
        if instance is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the instance does not exist")
        provider = self._providers.chat(instance.provider)
        if provider is None:
            raise HermesError(ErrorCode.INTERNAL, "hermes has no library for the provider of the instance")
        return instance, provider

    async def _credentials(self, instance: Instance) -> Mapping[str, SecretValue]:
        return {name: await self._secrets.resolve(reference) for name, reference in instance.secrets.items()}

    # --- The code --------------------------------------------------------------------------------------------------

    async def issue_code(self, contact: str, instance: str) -> IssuedCode:
        configuration = self._configuration.current()
        found = configuration.contact(contact)
        if found is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the contact does not exist")
        spec = next((c for c in found.channels if c.service == SERVICE and c.instance == instance), None)
        if spec is None or configuration.instance(instance) is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the contact has no chat channel in that instance")
        now = self._clock.now()
        code = self._new_code()
        expires = expires_at(now, self._code_minutes)
        entry = AuditEntry(
            at=now,
            kind=AuditKind.VERIFICATION_CODE_ISSUED,
            actor=ADMIN,
            outcome={"contact": contact, "instance": instance},
        )
        await self._channels.issue_code(
            ChannelKey(contact, spec.service, spec.provider, spec.instance), code.digest, expires, entry
        )
        logger.info("contacts.code", extra={"fields": {"contact": contact, "instance": instance}})
        return IssuedCode(contact=contact, instance=instance, code=code.shown, expires_at=expires)

    # --- What reaches the bot --------------------------------------------------------------------------------------

    async def collect(self, instance: str, message: IncomingMessage) -> bool:
        """True when the message verified a channel; a message that does not, gets no answer."""
        if message.chat_type != PRIVATE:
            return False
        digest = digest_of(message.text)
        if digest is None:
            return False
        now = self._clock.now()
        for state in await self._channels.pending(instance):
            if state.code_hash != digest or state.code_expires_at is None or now >= state.code_expires_at:
                continue
            entry = AuditEntry(
                at=now,
                kind=AuditKind.CHANNEL_VERIFIED,
                actor=ADMIN,
                outcome={"contact": state.key.contact, "instance": instance},
            )
            if not await self._channels.bind(state.key, digest, message.chat, now, entry):
                return False
            logger.info("channel.verified", extra={"fields": {"contact": state.key.contact, "instance": instance}})
            await self._reply(instance, message.chat)
            return True
        return False

    async def _reply(self, instance_name: str, chat: str) -> None:
        try:
            instance, provider = self._library(instance_name)
            credentials = await self._credentials(instance)
            await provider.send(Delivery(instance=instance, credentials=credentials, address=chat, text=VERIFIED_REPLY))
        except HermesError as error:
            logger.warning(
                "channel.reply_failed", extra={"fields": {"instance": instance_name, "reason": error.code.value}}
            )

    async def poll_once(self, instance_name: str) -> int:
        """Reads what reached the bot of an instance and collects each message; gives how many arrived."""
        instance, provider = self._library(instance_name)
        messages = await provider.receive(instance, await self._credentials(instance))
        for message in messages:
            await self.collect(instance_name, message)
        return len(messages)

    async def poll_forever(
        self, instance_name: str, *, sleep: Callable[[float], Awaitable[None]] = asyncio.sleep
    ) -> None:
        """The polling of an instance: a failure waits and retries; an empty answer waits a little."""
        while True:
            try:
                received = await self.poll_once(instance_name)
            except Exception as error:
                reason = error.code.value if isinstance(error, HermesError) else type(error).__name__
                logger.warning("polling.failed", extra={"fields": {"instance": instance_name, "reason": reason}})
                await sleep(POLL_RETRY_S)
                continue
            if received == 0:
                await sleep(POLL_PAUSE_S)
