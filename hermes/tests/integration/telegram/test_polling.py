"""The polling of the Telegram library (T078, contracts/telegram.md#uso-de-la-bot-api), with recorded answers."""

from __future__ import annotations

import asyncio
import json
import logging
from collections.abc import AsyncIterator

import httpx
import pytest
import respx

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import ChannelKey
from hermes.application.verification import POLL_PAUSE_S, POLL_RETRY_S, Verification
from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.provider import ProviderFailure
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.domain.verification.code import VerificationCode
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, configuration
from support.doubles import MemoryConfigStore, Providers, StaticSecrets
from support.telegram import API, recording

pytestmark = pytest.mark.anyio

INSTANCE = Instance("family_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)})
CREDENTIALS = {"bot_token": SecretValue(BOT_TOKEN)}
UPDATES = f"/bot{BOT_TOKEN}/getUpdates"
SEND = f"/bot{BOT_TOKEN}/sendMessage"


@pytest.fixture
async def telegram() -> AsyncIterator[tuple[TelegramProvider, respx.MockRouter]]:
    async with httpx.AsyncClient() as client:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            yield TelegramProvider(read_manifest("telegram"), client), router


async def test_get_updates_asks_with_its_offset_a_long_poll_and_only_messages(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    route = router.post(UPDATES).mock(side_effect=[recording("get_updates_messages"), recording("get_updates_empty")])
    await provider.receive(INSTANCE, CREDENTIALS)
    await provider.receive(INSTANCE, CREDENTIALS)
    first, second = (json.loads(call.request.content) for call in route.calls)
    assert first == {"offset": 0, "timeout": 25, "allowed_updates": ["message"]}
    assert second == {"offset": 105, "timeout": 25, "allowed_updates": ["message"]}


async def test_each_text_message_comes_once_in_order_with_its_chat(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    router.post(UPDATES).mock(side_effect=[recording("get_updates_messages"), recording("get_updates_empty")])
    received = await provider.receive(INSTANCE, CREDENTIALS)
    assert [(item.chat, item.chat_type, item.text) for item in received] == [
        (CHAT, "private", "ABCD-2345"),
        ("-100200300", "group", "WXYZ-6789"),
        (CHAT, "private", "hola"),
    ]
    assert await provider.receive(INSTANCE, CREDENTIALS) == []


async def test_each_instance_has_its_own_offset(telegram: tuple[TelegramProvider, respx.MockRouter]) -> None:
    provider, router = telegram
    other = Instance("other_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)})
    route = router.post(UPDATES).mock(side_effect=[recording("get_updates_messages"), recording("get_updates_empty")])
    await provider.receive(INSTANCE, CREDENTIALS)
    await provider.receive(other, CREDENTIALS)
    assert json.loads(route.calls[1].request.content)["offset"] == 0


async def test_a_transport_failure_waits_and_retries_and_the_code_verifies(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    store = MemoryConfigStore(configuration())
    verification = Verification(
        configuration=ConfigurationState(configuration()),
        channels=store,
        providers=Providers(telegram=provider),
        secrets=StaticSecrets({BOT_TOKEN_REF: BOT_TOKEN}),
        clock=FakeClock(),
        code_minutes=30,
        new_code=lambda: VerificationCode("ABCD2345"),
    )
    await verification.issue_code("contacto_demo", "family_bot")
    router.post(UPDATES).mock(
        side_effect=[httpx.ConnectError("no route"), recording("get_updates_messages"), recording("get_updates_empty")]
    )
    reply = router.post(SEND).mock(return_value=recording("send_message_ok"))
    pauses: list[float] = []

    async def sleep(seconds: float) -> None:
        pauses.append(seconds)
        if len(pauses) == 2:
            raise asyncio.CancelledError

    with pytest.raises(asyncio.CancelledError):
        await verification.poll_forever("family_bot", sleep=sleep)
    assert pauses == [POLL_RETRY_S, POLL_PAUSE_S]
    state = await store.state(ChannelKey("contacto_demo", "chat", "telegram", "family_bot"))
    assert state is not None
    assert state.address == CHAT
    assert json.loads(reply.calls.last.request.content)["text"] == "Canal verificado."


async def test_the_content_of_the_messages_never_reaches_the_log(
    telegram: tuple[TelegramProvider, respx.MockRouter], caplog: pytest.LogCaptureFixture
) -> None:
    provider, router = telegram
    caplog.set_level(logging.DEBUG, logger="hermes")
    router.post(UPDATES).mock(side_effect=[recording("get_updates_messages"), httpx.ReadTimeout("slow")])
    await provider.receive(INSTANCE, CREDENTIALS)
    with pytest.raises(ProviderFailure) as raised:
        await provider.receive(INSTANCE, CREDENTIALS)
    said = caplog.text + str(raised.value)
    for leak in (CHAT, "ABCD-2345", "hola", BOT_TOKEN, "api.telegram.org"):
        assert leak not in said
