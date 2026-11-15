"""The retries of sendMessage (T091, research R10, FR-027), against recorded answers of the Bot API.

What did not leave is retried with exponential waits and full jitter (base 0.2 s, top 2 s), with the wait that Telegram
asks for, and never past the 10 s of the function; what may have left is never retried.
"""

import logging
from collections.abc import AsyncIterator, Callable
from typing import Any

import httpx
import pytest
import respx

from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.provider import Delivery, ProviderFailure
from hermes.domain.shared.errors import ErrorCode
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.retry import RetryTiming
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT
from support.telegram import API, recording

pytestmark = pytest.mark.anyio

INSTANCE = Instance("family_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)})
SEND = f"/bot{BOT_TOKEN}/sendMessage"
DELIVERY = Delivery(instance=INSTANCE, credentials={"bot_token": SecretValue(BOT_TOKEN)}, address=CHAT, text="Hola")
NOT_SENT = [httpx.ConnectError("refused"), httpx.ConnectTimeout("slow"), httpx.PoolTimeout("busy")]


class Telegram:
    def __init__(self, client: httpx.AsyncClient, router: respx.MockRouter, jitter: float) -> None:
        self.clock = FakeClock()
        timing = RetryTiming(sleep=self.clock.sleep, monotonic=self.clock.monotonic, jitter=lambda: jitter)
        self.provider = TelegramProvider(read_manifest("telegram"), client, timing=timing)
        self.route = router.post(SEND)

    def answers(self, *answers: httpx.Response | Exception) -> None:
        self.route.mock(side_effect=list(answers))

    def always(self, failure: Callable[[], Exception]) -> None:
        def fail(request: httpx.Request) -> Any:
            raise failure()

        self.route.mock(side_effect=fail)

    def timeouts(self) -> list[float]:
        return [call.request.extensions["timeout"]["connect"] for call in self.route.calls]


@pytest.fixture(params=[1.0])
async def telegram(request: pytest.FixtureRequest) -> AsyncIterator[Telegram]:
    async with httpx.AsyncClient() as client:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            yield Telegram(client, router, jitter=request.param)


@pytest.mark.parametrize("failure", NOT_SENT, ids=lambda failure: type(failure).__name__)
async def test_a_send_that_did_not_leave_is_retried_and_arrives_once(telegram: Telegram, failure: Exception) -> None:
    telegram.answers(failure, recording("send_message_ok"))
    sent = await telegram.provider.send(DELIVERY)
    assert sent.provider_message_ref == "4521"
    assert telegram.route.call_count == 2
    assert telegram.clock.slept == [0.2]


async def test_the_waits_grow_up_to_their_top_and_stop_before_the_timeout_of_the_function(telegram: Telegram) -> None:
    telegram.always(lambda: httpx.ConnectError("refused"))
    with pytest.raises(ProviderFailure) as raised:
        await telegram.provider.send(DELIVERY)
    assert (raised.value.code, raised.value.retryable) == (ErrorCode.PROVIDER_UNAVAILABLE, True)
    assert telegram.clock.slept == [0.2, 0.4, 0.8, 1.6, 2.0, 2.0, 2.0]
    assert sum(telegram.clock.slept) <= 10
    assert telegram.route.call_count == len(telegram.clock.slept) + 1


@pytest.mark.parametrize("telegram", [0.5], indirect=True)
async def test_the_jitter_takes_each_wait_between_zero_and_its_top(telegram: Telegram) -> None:
    telegram.answers(httpx.ConnectError("refused"), httpx.ConnectError("refused"), recording("send_message_ok"))
    await telegram.provider.send(DELIVERY)
    assert telegram.clock.slept == [0.1, 0.2]


async def test_each_attempt_has_only_the_time_that_is_left(telegram: Telegram) -> None:
    telegram.answers(recording("send_message_429_brief"), recording("send_message_ok"))
    await telegram.provider.send(DELIVERY)
    assert telegram.clock.slept == [3]
    assert telegram.timeouts() == [10.0, 7.0]


async def test_a_limit_with_a_wait_that_fits_is_waited_and_retried(telegram: Telegram) -> None:
    telegram.answers(recording("send_message_429_brief"), recording("send_message_ok"))
    sent = await telegram.provider.send(DELIVERY)
    assert sent.provider_message_ref == "4521"
    assert telegram.route.call_count == 2


async def test_a_limit_with_a_wait_that_does_not_fit_is_rate_limited_without_waiting(telegram: Telegram) -> None:
    telegram.answers(recording("send_message_429"))
    with pytest.raises(ProviderFailure) as raised:
        await telegram.provider.send(DELIVERY)
    error = raised.value
    assert (error.code, error.retryable, error.retry_after_s, error.from_provider) == (
        ErrorCode.RATE_LIMITED,
        True,
        60,
        True,
    )
    assert telegram.clock.slept == []


async def test_a_limit_that_stops_fitting_after_some_retries_is_rate_limited(telegram: Telegram) -> None:
    telegram.answers(*[recording("send_message_429_brief")] * 4)
    with pytest.raises(ProviderFailure) as raised:
        await telegram.provider.send(DELIVERY)
    assert (raised.value.code, raised.value.retry_after_s) == (ErrorCode.RATE_LIMITED, 3)
    assert telegram.clock.slept == [3, 3, 3]


@pytest.mark.parametrize(
    "answer",
    [
        httpx.ReadTimeout("slow answer"),
        httpx.WriteTimeout("slow body"),
        httpx.RemoteProtocolError("cut"),
        httpx.ReadError("lost"),
        recording("send_message_502"),
    ],
    ids=["ReadTimeout", "WriteTimeout", "RemoteProtocolError", "ReadError", "502"],
)
async def test_what_may_have_left_is_never_retried(telegram: Telegram, answer: httpx.Response | Exception) -> None:
    telegram.answers(answer, recording("send_message_ok"))
    with pytest.raises(ProviderFailure) as raised:
        await telegram.provider.send(DELIVERY)
    assert (raised.value.code, raised.value.retryable, raised.value.outcome_unknown) == (ErrorCode.TIMEOUT, False, True)
    assert telegram.route.call_count == 1
    assert telegram.clock.slept == []


async def test_a_retry_leaves_a_line_of_log_without_the_url_the_token_or_the_chat(
    telegram: Telegram, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.DEBUG, logger="hermes")
    telegram.answers(httpx.ConnectError(f"refused {API}{SEND}"), recording("send_message_ok"))
    await telegram.provider.send(DELIVERY)
    [line] = [record for record in caplog.records if record.getMessage() == "provider.retry"]
    assert line.fields == {"provider": "telegram", "attempt": 1, "wait_s": 0.2}  # type: ignore[attr-defined]
    for leak in (BOT_TOKEN, CHAT, "api.telegram.org"):
        assert leak not in caplog.text
