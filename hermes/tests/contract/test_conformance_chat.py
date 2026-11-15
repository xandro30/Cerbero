"""The conformance battery of the chat service type (T035, T078, T091, T100, contracts/conformance.md): C1 to C12.

Every chat provider passes the same cases: the fake with its simulated failures and Telegram with recorded answers of
the Bot API (respx). A new provider adds its harness: how it receives, how its format shows a text and how it refuses.

Every case ends looking for leaks (T124): the logs of Hermes, in debug, and what the provider returned or raised never
show the URL of the provider, its token, the address of the recipient or the texts that the provider answered, and the
logs never show what reached the bot.
"""

from __future__ import annotations

import contextlib
import html
import json
import logging
import re
from collections.abc import AsyncIterator, Awaitable, Iterator, Mapping, Sequence
from typing import Any, Protocol

import httpx
import pytest
import respx

from hermes.application.capabilities import InstanceStatuses
from hermes.domain.configuration.model import Fact, Instance
from hermes.domain.instances.status import InstanceStatus
from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.provider import (
    ChatProvider,
    Delivery,
    IncomingMessage,
    ProviderFailure,
    Rendered,
    SentMessage,
)
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.errors import ErrorCode
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.logging import JsonFormatter
from hermes.infrastructure.providers.fake.adapter import FakeFailure, FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.retry import RetryTiming
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, FAKE_CHAT
from support.telegram import API, recording

pytestmark = pytest.mark.anyio

LINK = "https://cerbero.home/cases/case-1234"


class Harness(Protocol):
    provider: ChatProvider
    secrets: tuple[str, ...]
    """The URL of the provider, its token and the address of the recipient."""
    arrived: list[str]
    """The chats and the texts of the messages that reached the bot in this case."""

    def answered(self) -> list[str]:
        """The texts that the provider answered in this case."""
        ...

    def delivery(self, text: str) -> Delivery: ...

    def received(self) -> list[str]: ...

    def shown(self, text: str) -> str: ...

    def refuse_recipient(self) -> None: ...

    def keeps_its_format(self, text: str) -> bool: ...

    def arrive(self, messages: list[tuple[str, str]]) -> None:
        """Private messages that reach the bot: (chat, text)."""
        ...

    def unreachable(self, attempts: int | None) -> None:
        """The provider does not connect in so many attempts, or in any with None."""
        ...

    def limit(self, retry_after_s: int) -> None:
        """The provider limits the next attempt and asks for this wait."""
        ...

    def cut_after_send(self) -> None: ...

    def reject_credentials(self) -> None:
        """The provider refuses the credentials of the instance from now on."""
        ...

    def delivered(self) -> int:
        """How many messages reached their recipient."""
        ...

    def waited(self) -> float:
        """The seconds that the retries waited."""
        ...


class FakeHarness:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.fake = FakeProvider(read_manifest("fake"), self.clock, sleep=self.clock.sleep, jitter=lambda: 1.0)
        self.provider: ChatProvider = self.fake
        self.instance = Instance("test_fake", "fake", {}, {})
        self.secrets = (FAKE_CHAT,)
        self.arrived: list[str] = []

    def answered(self) -> list[str]:
        return []

    def delivery(self, text: str) -> Delivery:
        return Delivery(instance=self.instance, credentials={}, address=FAKE_CHAT, text=text)

    def received(self) -> list[str]:
        return [item.text for item in self.fake.sent()]

    def shown(self, text: str) -> str:
        return text

    def refuse_recipient(self) -> None:
        self.fake.fail_next(FakeFailure.RECIPIENT_REFUSES)

    def keeps_its_format(self, text: str) -> bool:
        return True

    def arrive(self, messages: list[tuple[str, str]]) -> None:
        for chat, text in messages:
            self.arrived += [chat, text]
            self.fake.deliver(self.instance.name, chat=chat, text=text)

    def unreachable(self, attempts: int | None) -> None:
        self.fake.fail_next(FakeFailure.UNREACHABLE, times=attempts if attempts is not None else 1000)

    def limit(self, retry_after_s: int) -> None:
        self.fake.fail_next(FakeFailure.RATE_LIMITED, retry_after_s=retry_after_s)

    def cut_after_send(self) -> None:
        self.fake.fail_next(FakeFailure.CUT_AFTER_SEND)

    def reject_credentials(self) -> None:
        self.fake.fail_next(FakeFailure.CREDENTIALS_REJECTED, times=10)

    def delivered(self) -> int:
        return len(self.fake.sent())

    def waited(self) -> float:
        return sum(self.clock.slept)


class TelegramHarness:
    def __init__(self, client: httpx.AsyncClient, router: respx.MockRouter) -> None:
        self.clock = FakeClock()
        timing = RetryTiming(sleep=self.clock.sleep, monotonic=self.clock.monotonic, jitter=lambda: 1.0)
        self.provider: ChatProvider = TelegramProvider(read_manifest("telegram"), client, timing=timing)
        self.instance = Instance("family_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)})
        self.route = router.post(f"/bot{BOT_TOKEN}/sendMessage").mock(return_value=recording("send_message_ok"))
        self.updates = router.post(f"/bot{BOT_TOKEN}/getUpdates").mock(return_value=recording("get_updates_empty"))
        self.get_me = router.post(f"/bot{BOT_TOKEN}/getMe").mock(return_value=recording("get_me_ok"))
        self.secrets = (BOT_TOKEN, CHAT, "api.telegram.org")
        self.arrived: list[str] = []

    def answered(self) -> list[str]:
        """The `description` of each answer of the Bot API in this case: what Telegram says when it refuses."""
        texts = []
        for route in (self.route, self.updates, self.get_me):
            for call in route.calls:
                body = call.response.json() if call.has_response else None
                if isinstance(body, dict) and body.get("description"):
                    texts.append(str(body["description"]))
        return texts

    def delivery(self, text: str) -> Delivery:
        return Delivery(
            instance=self.instance, credentials={"bot_token": SecretValue(BOT_TOKEN)}, address=CHAT, text=text
        )

    def received(self) -> list[str]:
        return [json.loads(call.request.content)["text"] for call in self.route.calls]

    def shown(self, text: str) -> str:
        return html.unescape(re.sub(r"</?b>", "", text))

    def refuse_recipient(self) -> None:
        self.route.mock(return_value=recording("send_message_403_blocked"))

    def keeps_its_format(self, text: str) -> bool:
        return re.findall(r"</?[A-Za-z][^>]*>", text) == ["<b>", "</b>"]

    def arrive(self, messages: list[tuple[str, str]]) -> None:
        for chat, text in messages:
            self.arrived += [chat, text]
        updates = [
            {
                "update_id": 500 + index,
                "message": {"message_id": index, "chat": {"id": int(chat), "type": "private"}, "date": 0, "text": text},
            }
            for index, (chat, text) in enumerate(messages)
        ]
        self.updates.mock(
            side_effect=[httpx.Response(200, json={"ok": True, "result": updates}), recording("get_updates_empty")]
        )

    def unreachable(self, attempts: int | None) -> None:
        if attempts is None:

            def refuse(request: httpx.Request) -> httpx.Response:
                raise httpx.ConnectError("refused")

            self.route.mock(side_effect=refuse)
            return
        failures: list[Exception | httpx.Response] = [httpx.ConnectError("refused")] * attempts
        self.route.mock(side_effect=[*failures, recording("send_message_ok")])

    def limit(self, retry_after_s: int) -> None:
        limited = httpx.Response(
            429,
            json={
                "ok": False,
                "error_code": 429,
                "description": f"Too Many Requests: retry after {retry_after_s}",
                "parameters": {"retry_after": retry_after_s},
            },
        )
        self.route.mock(side_effect=[limited, recording("send_message_ok")])

    def cut_after_send(self) -> None:
        self.route.mock(side_effect=httpx.ReadTimeout("the answer was lost"))

    def reject_credentials(self) -> None:
        self.route.mock(return_value=recording("send_message_401"))
        self.get_me.mock(return_value=recording("get_me_401"))

    def delivered(self) -> int:
        return sum(1 for call in self.route.calls if call.has_response and call.response.status_code == 200)

    def waited(self) -> float:
        return sum(self.clock.slept)


class Recorded:
    """The provider of a harness, keeping what each call returned or raised, to look for leaks when the case ends.

    What reaches the bot is not kept: it carries its chat and its text by contract.
    """

    def __init__(self, provider: ChatProvider) -> None:
        self._provider = provider
        self.outcomes: list[str] = []

    @property
    def manifest(self) -> Manifest:
        return self._provider.manifest

    def render(self, message: NeutralMessage) -> Rendered:
        return self._provider.render(message)

    async def send(self, delivery: Delivery) -> SentMessage:
        sent = await self._kept(self._provider.send(delivery))
        self.outcomes.append(repr(sent))
        return sent

    async def receive(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> list[IncomingMessage]:
        return await self._kept(self._provider.receive(instance, credentials))

    async def check_connection(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> None:
        await self._kept(self._provider.check_connection(instance, credentials))

    async def _kept[T](self, call: Awaitable[T]) -> T:
        try:
            return await call
        except ProviderFailure as failure:
            self.outcomes.append(f"{failure.message} {failure.details!r}")
            raise


def leaks(*, hidden: Sequence[str], arrived: Sequence[str], outcomes: Sequence[str], logs: str) -> list[str]:
    """What a case showed and may not (contracts/conformance.md): what is hidden, in the logs or in an outcome of the
    provider, and what reached the bot, in the logs."""
    found = [f"the logs show {item!r}" for item in [*hidden, *arrived] if item in logs]
    found += [f"an outcome shows {item!r}" for outcome in outcomes for item in hidden if item in outcome]
    return found


@contextlib.contextmanager
def without_leaks(made: Harness, caplog: pytest.LogCaptureFixture) -> Iterator[Harness]:
    """The harness, with its provider recorded; when the case ends, its logs as Hermes writes them, with their fields,
    and the outcomes of its provider carry nothing hidden."""
    recorded = Recorded(made.provider)
    made.provider = recorded
    yield made
    formatter = JsonFormatter()
    logs = "\n".join(formatter.format(record) for record in caplog.get_records("call"))
    problems = leaks(
        hidden=[*made.secrets, *made.answered()], arrived=made.arrived, outcomes=recorded.outcomes, logs=logs
    )
    assert not problems, "\n".join(problems)


@pytest.fixture(params=["fake", "telegram"])
async def harness(request: pytest.FixtureRequest, caplog: pytest.LogCaptureFixture) -> AsyncIterator[Harness]:
    """Each case with httpx and httpcore in warning, as `configure_logging` leaves them, and Hermes in debug."""
    for name in ("httpx", "httpcore"):
        caplog.set_level(logging.WARNING, logger=name)
    caplog.set_level(logging.DEBUG, logger="hermes")
    if request.param == "fake":
        with without_leaks(FakeHarness(), caplog) as made:
            yield made
        return
    async with httpx.AsyncClient() as client:
        with (
            respx.mock(base_url=API, assert_all_called=False) as router,
            without_leaks(TelegramHarness(client, router), caplog) as made,
        ):
            yield made


def message(**overrides: Any) -> NeutralMessage:
    fields: dict[str, Any] = {
        "title": "Dispositivo desconocido en la red",
        "severity": "high",
        "summary": "Ha aparecido AA:BB:CC:DD:EE:FF con la IP 192.168.1.57.",
        "facts": (Fact("MAC", "AA:BB:CC:DD:EE:FF"),),
        "actions_done": ("Bloqueado mientras decides",),
        "expected": "Acepta o bloquea el dispositivo",
        "link": LINK,
    }
    fields.update(overrides)
    return NeutralMessage(**fields)


def length(text: str) -> int:
    return len(text.encode("utf-16-le")) // 2


async def test_c1_a_send_to_a_verified_channel_completes_with_the_drawn_text(harness: Harness) -> None:
    rendered = harness.provider.render(message())
    sent = await harness.provider.send(harness.delivery(rendered.text))
    assert sent.provider_message_ref
    assert harness.received() == [rendered.text]
    assert rendered.truncated is False


async def test_c2_a_message_longer_than_the_limit_arrives_cut_with_its_title_and_link(harness: Harness) -> None:
    long = message(
        summary="resumen " * 256,
        facts=tuple(Fact(f"hecho {index}", "v" * 250) for index in range(20)),
        actions_done=tuple("a" * 250 for _ in range(10)),
    )
    rendered = harness.provider.render(long)
    assert rendered.truncated is True
    await harness.provider.send(harness.delivery(rendered.text))
    [received] = harness.received()
    shown = harness.shown(received)
    assert length(shown) <= harness.provider.manifest.max_length
    assert shown.startswith(long.title)
    assert shown.endswith(LINK)


async def test_c3_the_special_characters_of_the_format_arrive_escaped(harness: Harness) -> None:
    special = message(title="<b>&</b> <i>x</i>", summary="1 < 2 & 3 > 2", facts=(Fact("<tag>", "a&b"),))
    rendered = harness.provider.render(special)
    await harness.provider.send(harness.delivery(rendered.text))
    [received] = harness.received()
    assert harness.keeps_its_format(received)
    shown = harness.shown(received)
    assert shown.startswith("<b>&</b> <i>x</i>")
    assert "1 < 2 & 3 > 2" in shown
    assert "<tag>: a&b" in shown


async def test_c4_a_provider_that_connects_at_the_second_attempt_delivers_one_message(harness: Harness) -> None:
    harness.unreachable(1)
    sent = await harness.provider.send(harness.delivery("Hola"))
    assert sent.provider_message_ref
    assert harness.delivered() == 1


async def test_c5_a_provider_that_never_connects_is_unavailable_within_the_timeout(harness: Harness) -> None:
    harness.unreachable(None)
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.send(harness.delivery("Hola"))
    assert (raised.value.code, raised.value.retryable) == (ErrorCode.PROVIDER_UNAVAILABLE, True)
    assert 0 < harness.waited() <= harness.provider.manifest.function("chat", "send_message").timeout_s  # type: ignore[union-attr]
    assert harness.delivered() == 0


async def test_c6_a_limit_with_a_wait_that_fits_is_waited_and_the_send_completes(harness: Harness) -> None:
    harness.limit(3)
    sent = await harness.provider.send(harness.delivery("Hola"))
    assert sent.provider_message_ref
    assert harness.waited() == 3
    assert harness.delivered() == 1


async def test_c7_a_limit_with_a_wait_that_does_not_fit_is_rate_limited(harness: Harness) -> None:
    harness.limit(60)
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.send(harness.delivery("Hola"))
    assert (raised.value.code, raised.value.retry_after_s) == (ErrorCode.RATE_LIMITED, 60)
    assert harness.waited() == 0
    assert harness.delivered() == 0


async def test_c8_a_cut_after_sending_is_a_timeout_with_an_unknown_outcome_and_no_retry(harness: Harness) -> None:
    harness.cut_after_send()
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.send(harness.delivery("Hola"))
    error = raised.value
    assert (error.code, error.retryable, error.outcome_unknown) == (ErrorCode.TIMEOUT, False, True)
    assert harness.waited() == 0
    assert harness.delivered() <= 1


async def test_c9_rejected_credentials_are_not_retried_and_leave_the_instance_unavailable(harness: Harness) -> None:
    harness.reject_credentials()
    delivery = harness.delivery("Hola")
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.send(delivery)
    error = raised.value
    assert (error.code, error.retryable, error.credentials_rejected) == (ErrorCode.PROVIDER_UNAVAILABLE, False, True)
    assert harness.waited() == 0
    statuses = InstanceStatuses()
    statuses.record_call(delivery.instance.name, error)
    assert statuses.status(delivery.instance.name) is InstanceStatus.UNAVAILABLE


async def test_c11_the_connection_test_is_healthy_with_working_credentials_and_unavailable_without(
    harness: Harness,
) -> None:
    delivery = harness.delivery("x")
    statuses = InstanceStatuses()
    await harness.provider.check_connection(delivery.instance, delivery.credentials)
    statuses.record_test(delivery.instance.name, None)
    assert statuses.status(delivery.instance.name) is InstanceStatus.HEALTHY
    harness.reject_credentials()
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.check_connection(delivery.instance, delivery.credentials)
    assert raised.value.credentials_rejected is True
    statuses.record_test(delivery.instance.name, raised.value)
    assert statuses.status(delivery.instance.name) is InstanceStatus.UNAVAILABLE


async def test_c10_a_recipient_that_does_not_accept_the_bot_is_a_conflict(harness: Harness) -> None:
    harness.refuse_recipient()
    with pytest.raises(ProviderFailure) as raised:
        await harness.provider.send(harness.delivery("Hola"))
    assert (raised.value.code, raised.value.retryable) == (ErrorCode.CONFLICT, False)


async def test_c12_each_private_message_that_arrives_is_delivered_once_in_order_with_its_chat(harness: Harness) -> None:
    harness.arrive([("4242424201", "primer mensaje al bot"), ("4242424202", "segundo mensaje al bot")])
    instance = harness.delivery("x").instance
    credentials = harness.delivery("x").credentials
    received = await harness.provider.receive(instance, credentials)
    assert [(item.chat, item.chat_type, item.text) for item in received] == [
        ("4242424201", "private", "primer mensaje al bot"),
        ("4242424202", "private", "segundo mensaje al bot"),
    ]
    assert await harness.provider.receive(instance, credentials) == []


async def test_the_leak_check_finds_what_a_case_may_not_show() -> None:
    """The check that closes each case is not empty: it finds the URL, the token and the text of the provider in the
    logs or in an outcome, and what reached the bot in the logs; and nothing in a clean case."""
    hidden = [BOT_TOKEN, CHAT, "api.telegram.org", "Forbidden: bot was blocked by the user"]
    arrived = ["4242424201", "primer mensaje al bot"]
    assert leaks(hidden=hidden, arrived=arrived, outcomes=["SentMessage(provider_message_ref='5')"], logs="{}") == []
    found = leaks(
        hidden=hidden,
        arrived=arrived,
        outcomes=["refused: Forbidden: bot was blocked by the user"],
        logs=f'{{"event": "polled", "url": "https://api.telegram.org/bot{BOT_TOKEN}", "from": "4242424201"}}',
    )
    assert found == [
        f"the logs show {BOT_TOKEN!r}",
        "the logs show 'api.telegram.org'",
        "the logs show '4242424201'",
        "an outcome shows 'Forbidden: bot was blocked by the user'",
    ]
