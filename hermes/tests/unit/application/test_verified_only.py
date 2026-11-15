"""Only verified channels receive messages (T079, SC-008): a state machine with apply, ask for a code, verify and
send."""

from __future__ import annotations

import asyncio
from collections.abc import Coroutine
from datetime import datetime, timedelta
from typing import Any

from hypothesis import settings
from hypothesis import strategies as st
from hypothesis.stateful import RuleBasedStateMachine, rule

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import ChannelKey
from hermes.application.send_message import SendMessage
from hermes.application.verification import Verification
from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    Contact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
)
from hermes.domain.services.chat.contract import SendRequest
from hermes.domain.services.chat.provider import IncomingMessage
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.doubles import CALLER, MemoryAudit, MemoryConfigStore, Providers, StaticSecrets

CONTACTS = ("ana", "bea")
TEMPLATE = Template("aviso", (TemplateVersion(1, {"type": "object"}, MessageTexts(title="Aviso", severity="info")),))
INSTANCES = (Instance("fake_a", "fake", {}, {}), Instance("fake_b", "fake", {}, {}))


def configuration(ana_on: str) -> Configuration:
    return Configuration(
        instances=INSTANCES,
        templates=(TEMPLATE,),
        variables=(),
        contacts=(
            Contact("ana", (ChannelSpec("chat", "fake", ana_on),)),
            Contact("bea", (ChannelSpec("chat", "fake", "fake_a"),)),
        ),
    )


def run[T](coroutine: Coroutine[Any, Any, T]) -> T:
    return asyncio.run(coroutine)


class VerifiedOnly(RuleBasedStateMachine):
    def __init__(self) -> None:
        super().__init__()
        self.clock = FakeClock()
        self.current = configuration("fake_a")
        audit = MemoryAudit()
        self.store = MemoryConfigStore(self.current, audit=audit)
        self.state = ConfigurationState(self.current)
        fake = FakeProvider(read_manifest("fake"), self.clock)
        self.verification = Verification(
            configuration=self.state,
            channels=self.store,
            providers=Providers(fake=fake),
            secrets=StaticSecrets({}),
            clock=self.clock,
            code_minutes=30,
        )
        self.sender = SendMessage(
            configuration=self.state,
            channels=self.store,
            providers=Providers(fake=fake),
            secrets=StaticSecrets({}),
            audit=audit,
            clock=self.clock,
        )
        self.verified: set[ChannelKey] = set()
        self.codes: dict[ChannelKey, tuple[str, datetime]] = {}
        self.sends = 0

    def instance_of(self, contact: str) -> str:
        found = self.current.contact(contact)
        assert found is not None
        return found.channels[0].instance

    def key(self, contact: str) -> ChannelKey:
        return ChannelKey(contact, "chat", "fake", self.instance_of(contact))

    @rule()
    def apply_another_configuration(self) -> None:
        moved = "fake_b" if self.instance_of("ana") == "fake_a" else "fake_a"
        new = configuration(moved)
        run(self.store.replace(new))
        self.state.replace(new)
        self.current = new
        keys = {self.key(contact) for contact in CONTACTS}
        self.verified &= keys
        self.codes = {key: code for key, code in self.codes.items() if key in keys}

    @rule(contact=st.sampled_from(CONTACTS))
    def ask_for_a_code(self, contact: str) -> None:
        issued = run(self.verification.issue_code(contact, self.instance_of(contact)))
        key = self.key(contact)
        self.verified.discard(key)
        self.codes[key] = (issued.code, issued.expires_at)

    @rule(contact=st.sampled_from(CONTACTS))
    def send_the_code_to_the_bot(self, contact: str) -> None:
        key = self.key(contact)
        if key not in self.codes:
            return
        code, expires = self.codes[key]
        message = IncomingMessage(chat=f"chat-{contact}", chat_type="private", text=code)
        verified = run(self.verification.collect(key.instance, message))
        assert verified == (self.clock.now() < expires)
        if verified:
            self.verified.add(key)
            del self.codes[key]

    @rule(minutes=st.integers(min_value=0, max_value=40))
    def wait(self, minutes: int) -> None:
        self.clock.advance(timedelta(minutes=minutes))

    @rule(contact=st.sampled_from(CONTACTS))
    def send_a_message(self, contact: str) -> None:
        self.sends += 1
        request = SendRequest(recipients=(contact,), template_id="aviso", data={})
        try:
            run(self.sender.send(CALLER, request, f"key-{self.sends}"))
            delivered = True
        except HermesError as error:
            assert error.code is ErrorCode.CONFLICT
            delivered = False
        assert delivered == (self.key(contact) in self.verified)


VerifiedOnly.TestCase.settings = settings(max_examples=60, stateful_step_count=30, deadline=None)
TestVerifiedOnly = VerifiedOnly.TestCase
