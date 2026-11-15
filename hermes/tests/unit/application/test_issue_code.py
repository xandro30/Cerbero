"""Asking for the code of a channel (T076, FR-022, contracts/admin-channel.md `contacts.code`)."""

from __future__ import annotations

from datetime import timedelta

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import AuditKind, ChannelKey
from hermes.application.verification import Verification
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.verification.code import VerificationCode, digest_of
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


async def test_the_channel_is_left_unverified_without_address_and_with_the_new_code() -> None:
    world = World()
    world.store.verify("contacto_fake", "fake", "test_fake", "fake-chat-1")
    issued = await world.verification.issue_code("contacto_fake", "test_fake")
    state = await world.store.state(KEY)
    assert state is not None
    assert state.address is None
    assert state.verified_at is None
    assert state.code_hash == digest_of(issued.code)
    assert state.code_expires_at == world.clock.now() + timedelta(minutes=30)
    assert issued.expires_at == state.code_expires_at
    assert len(issued.code) == 9
    assert issued.code[4] == "-"


async def test_the_audit_keeps_the_request_without_the_code() -> None:
    world = World()
    issued = await world.verification.issue_code("contacto_fake", "test_fake")
    [entry] = world.audit.entries
    assert entry.kind is AuditKind.VERIFICATION_CODE_ISSUED
    assert entry.actor == "admin"
    assert entry.outcome == {"contact": "contacto_fake", "instance": "test_fake"}
    assert issued.code not in repr(entry)
    assert issued.code.replace("-", "") not in repr(entry)


@pytest.mark.parametrize(
    ("contact", "instance"), [("nadie", "test_fake"), ("contacto_demo", "test_fake"), ("contacto_fake", "missing")]
)
async def test_a_contact_or_a_channel_that_does_not_exist_is_not_found(contact: str, instance: str) -> None:
    world = World()
    with pytest.raises(HermesError) as raised:
        await world.verification.issue_code(contact, instance)
    assert raised.value.code is ErrorCode.NOT_FOUND
    assert world.audit.entries == []


async def test_a_new_code_replaces_the_previous_one() -> None:
    world = World()
    codes = iter([VerificationCode("AAAA1111"), VerificationCode("BBBB2222")])
    world.verification = Verification(
        configuration=ConfigurationState(configuration()),
        channels=world.store,
        providers=Providers(fake=world.fake),
        secrets=StaticSecrets({}),
        clock=world.clock,
        code_minutes=30,
        new_code=lambda: next(codes),
    )
    first = await world.verification.issue_code("contacto_fake", "test_fake")
    second = await world.verification.issue_code("contacto_fake", "test_fake")
    assert (first.code, second.code) == ("AAAA-1111", "BBBB-2222")
    state = await world.store.state(KEY)
    assert state is not None
    assert state.code_hash == digest_of("BBBB2222")


async def test_the_code_never_shows_in_the_issued_code_repr() -> None:
    world = World()
    issued = await world.verification.issue_code("contacto_fake", "test_fake")
    assert issued.code not in repr(issued)
