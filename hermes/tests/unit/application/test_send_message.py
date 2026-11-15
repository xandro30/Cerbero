"""The use case chat.send_message (T032), with doubles of every port."""

from datetime import timedelta
from typing import Any

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import AuditKind
from hermes.application.send_message import SendMessage
from hermes.domain.services.chat.contract import SendRequest
from hermes.domain.services.chat.provider import ProviderFailure
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, DATA, LINK, TITLE_V1, configuration
from support.doubles import CALLER, MemoryAudit, MemoryChannels, Providers, RecordingProvider, StaticSecrets

pytestmark = pytest.mark.anyio

KEY = "case-1234-notify"


class World:
    def __init__(self, provider_failure: ProviderFailure | Exception | None = None) -> None:
        self.clock = FakeClock()
        self.channels = MemoryChannels()
        self.channels.verify("contacto_demo", "telegram", "family_bot", CHAT)
        self.audit = MemoryAudit()
        self.secrets = StaticSecrets({BOT_TOKEN_REF: BOT_TOKEN})
        self.provider = RecordingProvider(read_manifest("telegram"), provider_failure)
        self.state = ConfigurationState(configuration())
        self.use_case = SendMessage(
            configuration=self.state,
            channels=self.channels,
            providers=Providers(telegram=self.provider),
            secrets=self.secrets,
            audit=self.audit,
            clock=self.clock,
        )


def request(**overrides: Any) -> SendRequest:
    fields: dict[str, Any] = {
        "recipients": ("contacto_demo",),
        "template_id": "device_unknown",
        "template_version": 1,
        "data": DATA,
    }
    fields.update(overrides)
    return SendRequest(**fields)


async def test_a_send_draws_the_message_sends_it_and_gives_the_result() -> None:
    world = World()
    result = await world.use_case.send(CALLER, request(), KEY)
    [delivery] = world.provider.deliveries
    assert delivery.address == CHAT
    assert delivery.instance.name == "family_bot"
    drawn = delivery.text
    assert drawn.startswith(TITLE_V1)
    assert "• MAC: AA:BB:CC:DD:EE:FF" in drawn
    assert drawn.endswith(LINK)
    assert result.recipient == "contacto_demo"
    assert result.instance == "family_bot"
    assert result.provider == "telegram"
    assert result.provider_message_ref == "ref-1"
    assert result.sent_at == world.clock.now()
    assert result.truncated is False


async def test_the_token_of_the_bot_comes_from_the_secret_source() -> None:
    world = World()
    await world.use_case.send(CALLER, request(), KEY)
    assert world.secrets.asked == [BOT_TOKEN_REF]
    assert world.provider.deliveries[0].credentials == {"bot_token": SecretValue(BOT_TOKEN)}


async def test_the_prepared_send_knows_its_provider_before_sending() -> None:
    world = World()
    prepared = await world.use_case.prepare(CALLER, request(), KEY)
    assert prepared.provider_name == "telegram"
    assert world.provider.deliveries == []
    assert CHAT not in repr(prepared)
    result = await world.use_case.execute(prepared)
    assert result.provider_message_ref == "ref-1"


@pytest.mark.parametrize(
    ("overrides", "code"),
    [
        ({"data": {**DATA, "mac": "no-es-una-mac"}}, ErrorCode.VALIDATION),
        ({"data": {"mac": "AA:BB:CC:DD:EE:FF"}}, ErrorCode.VALIDATION),
        ({"data": {**DATA, "extra": 1}}, ErrorCode.VALIDATION),
        ({"data": {**DATA, "ip": "999.1.1.1"}}, ErrorCode.VALIDATION),
        ({"template_id": "missing"}, ErrorCode.NOT_FOUND),
        ({"template_version": 9}, ErrorCode.NOT_FOUND),
        ({"instance": "missing_bot"}, ErrorCode.NOT_FOUND),
        ({"instance": "test_fake"}, ErrorCode.NOT_FOUND),
        ({"recipients": ("nadie",)}, ErrorCode.NOT_FOUND),
        ({"recipients": ("dos_canales",)}, ErrorCode.VALIDATION),
    ],
)
async def test_a_request_that_fails_before_the_provider_never_calls_it(
    overrides: dict[str, Any], code: ErrorCode
) -> None:
    world = World()
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(**overrides), KEY)
    assert raised.value.code is code
    assert raised.value.from_provider is False
    assert world.provider.deliveries == []
    assert world.audit.entries == []
    assert world.secrets.asked == []


async def test_the_failures_of_the_data_say_where_they_fail() -> None:
    world = World()
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(data={**DATA, "mac": "no-es-una-mac"}), KEY)
    assert [detail["path"] for detail in raised.value.details] == ["data.mac"]


async def test_a_channel_that_is_not_verified_is_a_conflict() -> None:
    world = World()
    world.channels.unverified("contacto_demo", "telegram", "family_bot")
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(), KEY)
    assert raised.value.code is ErrorCode.CONFLICT
    assert world.provider.deliveries == []
    assert world.audit.entries == []


async def test_a_channel_that_the_database_does_not_know_is_a_conflict() -> None:
    world = World()
    world.channels = MemoryChannels()
    world.use_case = SendMessage(
        configuration=world.state,
        channels=world.channels,
        providers=Providers(telegram=world.provider),
        secrets=world.secrets,
        audit=world.audit,
        clock=world.clock,
    )
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(), KEY)
    assert raised.value.code is ErrorCode.CONFLICT


async def test_without_a_configuration_hermes_is_unavailable() -> None:
    world = World()
    world.use_case = SendMessage(
        configuration=ConfigurationState(),
        channels=world.channels,
        providers=Providers(telegram=world.provider),
        secrets=world.secrets,
        audit=world.audit,
        clock=world.clock,
    )
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(), KEY)
    assert raised.value.code is ErrorCode.UNAVAILABLE


async def test_a_secret_past_its_hard_time_stops_the_send_before_the_provider() -> None:
    world = World()
    world.secrets = StaticSecrets({})
    world.use_case = SendMessage(
        configuration=world.state,
        channels=world.channels,
        providers=Providers(telegram=world.provider),
        secrets=world.secrets,
        audit=world.audit,
        clock=world.clock,
    )
    with pytest.raises(HermesError) as raised:
        await world.use_case.send(CALLER, request(), KEY)
    assert raised.value.code is ErrorCode.INTERNAL
    assert world.provider.deliveries == []
    assert world.audit.entries == []


async def test_a_send_that_reaches_the_provider_leaves_its_audit_entry() -> None:
    world = World()
    world.clock.advance(timedelta(seconds=5))
    await world.use_case.send(CALLER, request(), KEY)
    [entry] = world.audit.entries
    assert entry.kind is AuditKind.SEND
    assert entry.at == world.clock.now()
    assert entry.actor == "spiffe://cerbero.test/hades"
    assert entry.function == "chat.send_message"
    assert entry.instance == "family_bot"
    assert entry.recipients == ("contacto_demo",)
    assert entry.side_effect == f"Envía a contacto_demo el mensaje «{TITLE_V1}» por la instancia family_bot"
    assert entry.discloses_data == "telegram recibe el texto del mensaje y el chat de contacto_demo"
    assert entry.audit_context == {"source": "test", "playbook": "pb-1"}
    assert entry.idempotency_key == KEY
    assert entry.outcome == {"status": "completed", "code": None, "outcome_unknown": False}
    assert BOT_TOKEN not in repr(entry)
    assert CHAT not in repr(entry)


@pytest.mark.parametrize(
    ("failure", "outcome"),
    [
        (
            ProviderFailure(ErrorCode.CONFLICT, "the person does not accept messages from the bot", retryable=False),
            {"status": "failed", "code": "conflict", "outcome_unknown": False},
        ),
        (
            ProviderFailure(ErrorCode.TIMEOUT, "it may have arrived", retryable=False, outcome_unknown=True),
            {"status": "failed", "code": "timeout", "outcome_unknown": True},
        ),
    ],
)
async def test_a_failure_of_the_provider_is_audited_and_goes_inside_the_envelope(
    failure: ProviderFailure, outcome: dict[str, Any]
) -> None:
    world = World(provider_failure=failure)
    with pytest.raises(ProviderFailure) as raised:
        await world.use_case.send(CALLER, request(), KEY)
    assert raised.value is failure
    assert raised.value.from_provider is True
    [entry] = world.audit.entries
    assert entry.outcome == outcome


async def test_an_unexpected_failure_while_sending_is_audited_too() -> None:
    world = World(provider_failure=RuntimeError("boom"))
    with pytest.raises(RuntimeError):
        await world.use_case.send(CALLER, request(), KEY)
    [entry] = world.audit.entries
    assert entry.outcome == {"status": "failed", "code": "internal", "outcome_unknown": False}
