"""The cache of the secrets (T058, FR-009, SC-004): 300 s without asking; up to 3600 s with Hécate sealed or down.
On the identity service, as `hermes serve` wires it, a refused identity fails as a refusal of Hécate (change 011)."""

from __future__ import annotations

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.identity import IdentityService
from hermes.application.secrets import SecretCache
from hermes.application.send_message import SendMessage
from hermes.domain.services.chat.contract import SendRequest
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, DATA, configuration
from support.doubles import CALLER, MemoryAudit, MemoryChannels, MemoryIdentityStore, Providers, RecordingProvider
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = pytest.mark.anyio

TOKEN = SecretReference.parse(BOT_TOKEN_REF)
PASSWORD = SecretReference.parse("secret://hermes/postgres/password")


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.hecate = FakeHecate(TestCa(now=self.clock.now()), self.clock)
        self.hecate.secrets = {BOT_TOKEN_REF: BOT_TOKEN, str(PASSWORD): "db-password"}
        self.cache = SecretCache(self.hecate, self.clock)


async def test_a_value_is_used_for_300_seconds_without_asking_again() -> None:
    world = World()
    assert await world.cache.resolve(TOKEN) == SecretValue(BOT_TOKEN)
    world.clock.advance(299)
    assert await world.cache.resolve(TOKEN) == SecretValue(BOT_TOKEN)
    assert world.hecate.resolutions == 1


async def test_after_300_seconds_it_is_refreshed() -> None:
    world = World()
    await world.cache.resolve(TOKEN)
    world.clock.advance(300)
    world.hecate.secrets[BOT_TOKEN_REF] = "rotated-token"
    assert await world.cache.resolve(TOKEN) == SecretValue("rotated-token")
    assert world.hecate.resolutions == 2


@pytest.mark.parametrize("trouble", ["sealed", "down"])
async def test_with_hecate_sealed_or_down_the_value_lasts_until_3600_seconds(trouble: str) -> None:
    world = World()
    await world.cache.resolve(TOKEN)
    setattr(world.hecate, trouble, True)
    world.clock.advance(3599)
    assert await world.cache.resolve(TOKEN) == SecretValue(BOT_TOKEN)
    world.clock.advance(1)
    with pytest.raises(HermesError) as raised:
        await world.cache.resolve(TOKEN)
    assert raised.value.code is ErrorCode.INTERNAL


async def test_a_secret_that_hecate_does_not_resolve_is_not_used_any_more() -> None:
    world = World()
    await world.cache.resolve(TOKEN)
    world.clock.advance(300)
    del world.hecate.secrets[BOT_TOKEN_REF]
    with pytest.raises(HermesError) as raised:
        await world.cache.resolve(TOKEN)
    assert raised.value.code is ErrorCode.NOT_FOUND
    with pytest.raises(HermesError):
        world.cache.current(TOKEN)


async def test_the_password_of_the_database_comes_from_the_current_value() -> None:
    world = World()
    with pytest.raises(HermesError):
        world.cache.current(PASSWORD)
    await world.cache.resolve(PASSWORD)
    assert world.cache.current(PASSWORD) == SecretValue("db-password")
    world.clock.advance(300)
    world.hecate.secrets[str(PASSWORD)] = "rotated-password"
    await world.cache.resolve(PASSWORD)
    assert world.cache.current(PASSWORD) == SecretValue("rotated-password")
    world.hecate.down = True
    world.clock.advance(3600)
    with pytest.raises(HermesError) as raised:
        world.cache.current(PASSWORD)
    assert raised.value.code is ErrorCode.INTERNAL


async def test_past_its_hard_time_the_send_fails_with_internal_without_reaching_the_provider() -> None:
    world = World()
    provider = RecordingProvider(read_manifest("telegram"))
    channels = MemoryChannels()
    channels.verify("contacto_demo", "telegram", "family_bot", CHAT)
    send = SendMessage(
        configuration=ConfigurationState(configuration()),
        channels=channels,
        providers=Providers(telegram=provider),
        secrets=world.cache,
        audit=MemoryAudit(),
        clock=world.clock,
    )
    request = SendRequest(recipients=("contacto_demo",), template_id="device_unknown", data=DATA)
    await send.send(CALLER, request, "k1")
    world.hecate.sealed = True
    world.clock.advance(3599)
    await send.send(CALLER, request, "k2")
    assert len(provider.deliveries) == 2
    world.clock.advance(1)
    with pytest.raises(HermesError) as raised:
        await send.send(CALLER, request, "k3")
    assert raised.value.code is ErrorCode.INTERNAL
    assert len(provider.deliveries) == 2


async def test_the_values_never_show() -> None:
    world = World()
    await world.cache.resolve(TOKEN)
    assert BOT_TOKEN not in repr(world.cache)
    cached = vars(world.cache)["_values"]
    assert BOT_TOKEN not in repr(cached)
    assert "SecretValue(***)" in repr(cached)


class OnTheIdentity:
    """The cache on the identity service of Hermes, which resolves with its identity (change 011, research R2)."""

    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.hecate = FakeHecate(self.ca, self.clock)
        self.hecate.secrets = {str(PASSWORD): "db-password"}
        self.service = IdentityService(
            store=MemoryIdentityStore(), hecate=self.hecate, clock=self.clock, new_keys=generate_keys
        )
        self.hecate.caller_serial = self.serial
        self.cache = SecretCache(self.service, self.clock)

    def serial(self) -> str | None:
        identity = self.service.identity
        return identity.certificate.serial if identity is not None else None


async def test_with_the_identity_refused_the_value_holds_its_soft_time_and_then_goes_without_asking() -> None:
    """Research R4: the cache behaves as with a refusal of Hécate, and the blocked resolution never reaches it."""
    world = OnTheIdentity()
    await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert await world.cache.resolve(PASSWORD) == SecretValue("db-password")
    serial = world.serial()
    assert serial is not None
    world.hecate.revoke_urgently(serial)
    await world.service.refresh_publications()
    assert world.service.problem() is not None
    calls = world.hecate.authenticated_calls
    world.clock.advance(299)
    assert await world.cache.resolve(PASSWORD) == SecretValue("db-password")
    world.clock.advance(1)
    with pytest.raises(HermesError) as raised:
        await world.cache.resolve(PASSWORD)
    assert raised.value.code is ErrorCode.AUTH
    with pytest.raises(HermesError) as raised:
        world.cache.current(PASSWORD)
    assert raised.value.code is ErrorCode.INTERNAL
    assert world.hecate.authenticated_calls == calls
