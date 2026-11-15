"""sendMessage of the Telegram library (T031) against recorded answers of the Bot API (contracts/telegram.md)."""

import json
import logging
from collections.abc import AsyncIterator

import httpx
import pytest
import respx

from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.provider import Delivery, ProviderFailure
from hermes.domain.shared.errors import ErrorCode
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT
from support.telegram import API, description, recording

pytestmark = pytest.mark.anyio

INSTANCE = Instance("family_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)})
SEND = f"/bot{BOT_TOKEN}/sendMessage"
TEXT = "<b>Dispositivo desconocido</b>\nGravedad: high"


@pytest.fixture
async def telegram() -> AsyncIterator[tuple[TelegramProvider, respx.MockRouter]]:
    async with httpx.AsyncClient() as client:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            yield TelegramProvider(read_manifest("telegram"), client), router


def delivery() -> Delivery:
    return Delivery(instance=INSTANCE, credentials={"bot_token": SecretValue(BOT_TOKEN)}, address=CHAT, text=TEXT)


async def test_a_sent_message_gives_its_message_id_as_the_reference(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    route = router.post(SEND).mock(return_value=recording("send_message_ok"))
    sent = await provider.send(delivery())
    assert sent.provider_message_ref == "4521"
    assert route.call_count == 1
    assert json.loads(route.calls.last.request.content) == {
        "chat_id": CHAT,
        "text": TEXT,
        "parse_mode": "HTML",
        "link_preview_options": {"is_disabled": True},
    }


@pytest.mark.parametrize(
    ("name", "code", "credentials_rejected"),
    [
        ("send_message_403_blocked", ErrorCode.CONFLICT, False),
        ("send_message_400_chat_not_found", ErrorCode.PROVIDER_UNAVAILABLE, False),
        ("send_message_401", ErrorCode.PROVIDER_UNAVAILABLE, True),
        ("send_message_404", ErrorCode.PROVIDER_UNAVAILABLE, True),
    ],
)
async def test_each_refusal_of_telegram_has_its_code_and_no_retry(
    telegram: tuple[TelegramProvider, respx.MockRouter], name: str, code: ErrorCode, credentials_rejected: bool
) -> None:
    provider, router = telegram
    route = router.post(SEND).mock(return_value=recording(name))
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    error = raised.value
    assert (error.code, error.retryable, error.from_provider) == (code, False, True)
    assert error.credentials_rejected is credentials_rejected
    assert error.outcome_unknown is None
    assert route.call_count == 1


async def test_a_server_error_after_sending_leaves_the_outcome_unknown(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    route = router.post(SEND).mock(return_value=recording("send_message_502"))
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    assert (raised.value.code, raised.value.retryable, raised.value.outcome_unknown) == (ErrorCode.TIMEOUT, False, True)
    assert route.call_count == 1


async def test_a_cut_while_reading_the_answer_leaves_the_outcome_unknown(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    route = router.post(SEND).mock(side_effect=httpx.ReadTimeout("read timed out"))
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    assert (raised.value.code, raised.value.retryable, raised.value.outcome_unknown) == (ErrorCode.TIMEOUT, False, True)
    assert route.call_count == 1


async def test_a_limit_that_does_not_fit_the_timeout_is_rate_limited(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    router.post(SEND).mock(return_value=recording("send_message_429"))
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    assert (raised.value.code, raised.value.retryable, raised.value.retry_after_s) == (ErrorCode.RATE_LIMITED, True, 60)


@pytest.mark.parametrize(
    "name",
    [
        "send_message_400_chat_not_found",
        "send_message_401",
        "send_message_403_blocked",
        "send_message_404",
        "send_message_429",
        "send_message_502",
    ],
)
async def test_no_error_carries_the_url_the_token_the_chat_or_the_text_of_telegram(
    telegram: tuple[TelegramProvider, respx.MockRouter], name: str, caplog: pytest.LogCaptureFixture
) -> None:
    provider, router = telegram
    router.post(SEND).mock(return_value=recording(name))
    caplog.set_level(logging.DEBUG, logger="hermes")
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    said = " ".join([str(raised.value), raised.value.message, repr(raised.value.details), caplog.text])
    for leak in (BOT_TOKEN, CHAT, "api.telegram.org", description(name)):
        assert leak not in said
    assert raised.value.__cause__ is None
    assert raised.value.__suppress_context__ is True


async def test_a_transport_error_does_not_carry_the_url_either(
    telegram: tuple[TelegramProvider, respx.MockRouter],
) -> None:
    provider, router = telegram
    router.post(SEND).mock(side_effect=httpx.ReadTimeout(f"timed out reading {API}{SEND}"))
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    assert BOT_TOKEN not in str(raised.value)
    assert raised.value.__cause__ is None
    assert raised.value.__suppress_context__ is True
