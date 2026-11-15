"""Collecting the code that verifies a channel (T077, FR-022, research R12)."""

from __future__ import annotations

from datetime import timedelta

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import AuditKind, ChannelKey
from hermes.application.verification import VERIFIED_REPLY, Verification
from hermes.domain.services.chat.provider import IncomingMessage
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import configuration
from support.doubles import MemoryAudit, MemoryConfigStore, Providers, StaticSecrets

pytestmark = pytest.mark.anyio

KEY = ChannelKey("contacto_fake", "chat", "fake", "test_fake")


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.audit = MemoryAudit()
        self.store = MemoryConfigStore(configuration(), audit=self.audit)
        self.fake = FakeProvider(read_manifest("fake"), self.clock)
        self.verification = Verification(
            configuration=ConfigurationState(configuration()),
            channels=self.store,
            providers=Providers(fake=self.fake),
            secrets=StaticSecrets({}),
            clock=self.clock,
            code_minutes=30,
        )

    async def code(self, contact: str = "contacto_fake", instance: str = "test_fake") -> str:
        return (await self.verification.issue_code(contact, instance)).code


def private(text: str, chat: str = "fake-chat-9") -> IncomingMessage:
    return IncomingMessage(chat=chat, chat_type="private", text=text)


async def test_a_pending_code_from_a_private_chat_binds_verifies_and_gets_its_answer() -> None:
    world = World()
    code = await world.code()
    assert await world.verification.collect("test_fake", private(f"  {code.lower()} ")) is True
    state = await world.store.state(KEY)
    assert state is not None
    assert (state.address, state.verified_at, state.code_hash) == ("fake-chat-9", world.clock.now(), None)
    [reply] = world.fake.sent()
    assert (reply.chat, reply.text) == ("fake-chat-9", VERIFIED_REPLY)
    assert VERIFIED_REPLY == "Canal verificado."


async def test_the_audit_keeps_the_verification_without_the_address() -> None:
    world = World()
    code = await world.code()
    await world.verification.collect("test_fake", private(code))
    entry = world.audit.entries[-1]
    assert entry.kind is AuditKind.CHANNEL_VERIFIED
    assert entry.outcome == {"contact": "contacto_fake", "instance": "test_fake"}
    assert "fake-chat-9" not in repr(entry)


async def test_an_expired_code_does_nothing_and_gets_no_answer() -> None:
    world = World()
    code = await world.code()
    world.clock.advance(timedelta(minutes=30))
    assert await world.verification.collect("test_fake", private(code)) is False
    state = await world.store.state(KEY)
    assert state is not None
    assert state.verified_at is None
    assert world.fake.sent() == []


async def test_an_unknown_code_does_nothing() -> None:
    world = World()
    await world.code()
    assert await world.verification.collect("test_fake", private("ZZZZ-9999")) is False
    assert world.fake.sent() == []


async def test_the_same_message_twice_does_nothing_the_second_time() -> None:
    world = World()
    code = await world.code()
    assert await world.verification.collect("test_fake", private(code)) is True
    assert await world.verification.collect("test_fake", private(code, chat="fake-chat-other")) is False
    state = await world.store.state(KEY)
    assert state is not None
    assert state.address == "fake-chat-9"
    assert len(world.fake.sent()) == 1


@pytest.mark.parametrize(
    "message",
    [
        IncomingMessage(chat="g-1", chat_type="group", text="CODE"),
        IncomingMessage(chat="g-1", chat_type="supergroup", text="CODE"),
        IncomingMessage(chat="fake-chat-9", chat_type="private", text="hola, ¿qué tal?"),
    ],
)
async def test_groups_and_other_messages_are_ignored(message: IncomingMessage) -> None:
    world = World()
    code = await world.code()
    message = IncomingMessage(chat=message.chat, chat_type=message.chat_type, text=message.text.replace("CODE", code))
    assert await world.verification.collect("test_fake", message) is False
    assert world.fake.sent() == []
    assert [entry.kind for entry in world.audit.entries] == [AuditKind.VERIFICATION_CODE_ISSUED]


async def test_a_code_of_another_instance_does_not_verify() -> None:
    world = World()
    code = await world.code("dos_canales", "family_bot")
    assert await world.verification.collect("test_fake", private(code)) is False


async def test_the_verification_polls_the_inbox_of_its_instance() -> None:
    world = World()
    code = await world.code()
    world.fake.deliver("test_fake", chat="fake-chat-9", text=code)
    assert await world.verification.poll_once("test_fake") == 1
    state = await world.store.state(KEY)
    assert state is not None
    assert state.verified_at is not None
