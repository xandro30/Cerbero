"""The limit of use of each instance (T092, research R13, FR-028): one bucket of tokens per instance, in memory, with
the requests per minute of its library or of the instance; without tokens, `rate_limited` before the provider."""

from dataclasses import replace

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.rate_limits import RateLimits
from hermes.application.send_message import SendMessage
from hermes.domain.services.chat.contract import SendRequest
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.envelope import failed_response
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, DATA, configuration
from support.doubles import CALLER, MemoryAudit, MemoryChannels, Providers, RecordingProvider, StaticSecrets

pytestmark = pytest.mark.anyio


def limited(limits: RateLimits, instance: str, per_minute: int) -> HermesError:
    with pytest.raises(HermesError) as raised:
        limits.take(instance, per_minute)
    return raised.value


def test_an_instance_takes_as_many_sends_per_minute_as_its_limit_and_then_waits_for_the_next_token() -> None:
    clock = FakeClock()
    limits = RateLimits(clock)
    for _ in range(30):
        limits.take("family_bot", 30)
    error = limited(limits, "family_bot", 30)
    assert (error.code, error.retryable, error.retry_after_s) == (ErrorCode.RATE_LIMITED, True, 2)
    clock.advance(2)
    limits.take("family_bot", 30)
    assert limited(limits, "family_bot", 30).retry_after_s == 2


def test_the_wait_is_rounded_up_to_whole_seconds() -> None:
    clock = FakeClock()
    limits = RateLimits(clock)
    limits.take("family_bot", 1)
    clock.advance(0.5)
    assert limited(limits, "family_bot", 1).retry_after_s == 60
    clock.advance(59)
    assert limited(limits, "family_bot", 1).retry_after_s == 1
    clock.advance(0.5)
    limits.take("family_bot", 1)


def test_each_instance_has_its_own_bucket() -> None:
    limits = RateLimits(FakeClock())
    limits.take("family_bot", 1)
    limits.take("test_fake", 1)
    assert limited(limits, "family_bot", 1).code is ErrorCode.RATE_LIMITED


def test_a_lower_limit_never_leaves_more_tokens_than_it_allows() -> None:
    limits = RateLimits(FakeClock())
    limits.take("family_bot", 600)
    limits.take("family_bot", 1)
    assert limited(limits, "family_bot", 1).code is ErrorCode.RATE_LIMITED


def test_without_tokens_the_answer_is_429_with_retry_after() -> None:
    limits = RateLimits(FakeClock())
    limits.take("family_bot", 1)
    response = failed_response({}, limited(limits, "family_bot", 1))
    assert response.status_code == 429
    assert response.headers["Retry-After"] == "60"


class World:
    def __init__(self, requests_per_minute: int | None = None) -> None:
        self.clock = FakeClock()
        channels = MemoryChannels()
        channels.verify("contacto_demo", "telegram", "family_bot", CHAT)
        self.audit = MemoryAudit()
        self.provider = RecordingProvider(read_manifest("telegram"))
        config = configuration()
        instances = tuple(
            replace(item, requests_per_minute=requests_per_minute) if item.name == "family_bot" else item
            for item in config.instances
        )
        self.use_case = SendMessage(
            configuration=ConfigurationState(replace(config, instances=instances)),
            channels=channels,
            providers=Providers(telegram=self.provider),
            secrets=StaticSecrets({BOT_TOKEN_REF: BOT_TOKEN}),
            audit=self.audit,
            clock=self.clock,
        )

    async def send(self) -> None:
        request = SendRequest(
            recipients=("contacto_demo",), template_id="device_unknown", template_version=1, data=DATA
        )
        await self.use_case.send(CALLER, request, "case-1234-notify")


async def test_the_limit_of_the_library_applies_when_the_instance_has_none() -> None:
    world = World()
    for _ in range(30):
        await world.send()
    with pytest.raises(HermesError) as raised:
        await world.send()
    assert (raised.value.code, raised.value.retry_after_s) == (ErrorCode.RATE_LIMITED, 2)
    assert len(world.provider.deliveries) == 30


async def test_the_limit_of_the_instance_replaces_the_one_of_its_library_and_the_provider_is_not_called() -> None:
    world = World(requests_per_minute=1)
    await world.send()
    with pytest.raises(HermesError) as raised:
        await world.send()
    assert (raised.value.code, raised.value.retry_after_s) == (ErrorCode.RATE_LIMITED, 60)
    assert raised.value.from_provider is False
    assert len(world.provider.deliveries) == 1
    assert len(world.audit.entries) == 1
