"""The provider registry and the fake provider (T030, contracts/telegram.md#manifiesto and contracts/fake.md)."""

import json
from collections.abc import AsyncIterator
from pathlib import Path

import httpx
import pytest

from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.provider import Delivery, ProviderFailure
from hermes.domain.shared.errors import ErrorCode
from hermes.infrastructure.providers.fake.adapter import FakeFailure, FakeProvider
from hermes.infrastructure.providers.registry import load_registry, read_manifest
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock

pytestmark = pytest.mark.anyio

TELEGRAM_MANIFEST = {
    "provider": "telegram",
    "version": "1.0.0",
    "implements": [
        {
            "service": "chat",
            "contract": "1",
            "functions": {"send_message": {"kind": "command", "mode": "sync", "timeout": "10s"}},
        }
    ],
    "capabilities": {"max_length": 4096, "formats": ["text", "html"]},
    "limits": {"requests_per_minute": 30},
    "permissions": [
        "Recibir los mensajes privados que se envían al bot (getUpdates)",
        "Enviar mensajes a los chats privados que han escrito al bot (sendMessage)",
    ],
    "instance_config": {"bot_token": {"type": "secret", "required": True}},
}
INSTANCE = Instance("test_fake", "fake", {}, {})


def package_manifest(provider: str) -> object:
    path = Path(__file__).resolve().parents[3] / "src" / "hermes" / "infrastructure" / "providers" / provider
    return json.loads((path / "manifest.json").read_text(encoding="utf-8"))


def test_the_telegram_manifest_is_the_one_of_the_contract() -> None:
    assert package_manifest("telegram") == TELEGRAM_MANIFEST
    manifest = read_manifest("telegram")
    assert (manifest.provider, manifest.version) == ("telegram", "1.0.0")
    function = manifest.function("chat", "send_message")
    assert function is not None
    assert (function.kind, function.mode, function.timeout_s) == ("command", "sync", 10)
    assert manifest.max_length == 4096
    assert manifest.formats == ("text", "html")
    assert manifest.requests_per_minute == 30
    assert len(manifest.permissions) == 2
    assert manifest.secret_fields == ("bot_token",)


def test_the_fake_manifest_has_send_message_and_no_secrets() -> None:
    manifest = read_manifest("fake")
    function = manifest.function("chat", "send_message")
    assert manifest.provider == "fake"
    assert function is not None
    assert (function.mode, function.timeout_s) == ("sync", 10)
    assert manifest.max_length == 4096
    assert manifest.secret_fields == ()


@pytest.fixture
async def http() -> AsyncIterator[httpx.AsyncClient]:
    async with httpx.AsyncClient() as client:
        yield client


async def test_the_registry_only_loads_the_fake_when_it_is_enabled(http: httpx.AsyncClient) -> None:
    without = load_registry(fake_enabled=False, http=http, clock=FakeClock())
    assert isinstance(without.chat("telegram"), TelegramProvider)
    assert without.chat("fake") is None
    assert [manifest.provider for manifest in without.manifests()] == ["telegram"]
    enabled = load_registry(fake_enabled=True, http=http, clock=FakeClock())
    assert isinstance(enabled.chat("fake"), FakeProvider)
    assert [manifest.provider for manifest in enabled.manifests()] == ["fake", "telegram"]
    assert enabled.chat("whatsapp") is None


def fake(directory: Path | None = None) -> FakeProvider:
    clock = FakeClock()
    return FakeProvider(read_manifest("fake"), clock, directory=directory, sleep=clock.sleep, jitter=lambda: 1.0)


def delivery(text: str = "Hola", address: str = "fake-chat-1") -> Delivery:
    return Delivery(instance=INSTANCE, credentials={}, address=address, text=text)


async def test_the_fake_keeps_what_it_sends() -> None:
    provider = fake()
    first = await provider.send(delivery("uno"))
    second = await provider.send(delivery("dos", address="fake-chat-2"))
    assert first.provider_message_ref != second.provider_message_ref
    sent = provider.sent()
    assert [(item.instance, item.chat, item.text) for item in sent] == [
        ("test_fake", "fake-chat-1", "uno"),
        ("test_fake", "fake-chat-2", "dos"),
    ]
    assert [item.provider_message_ref for item in sent] == [first.provider_message_ref, second.provider_message_ref]
    assert "fake-chat-1" not in repr(sent[0])


@pytest.mark.parametrize(
    ("failure", "times", "code", "retryable", "outcome_unknown", "credentials_rejected", "delivered"),
    [
        (FakeFailure.UNREACHABLE, 20, ErrorCode.PROVIDER_UNAVAILABLE, True, None, False, False),
        (FakeFailure.RATE_LIMITED, 1, ErrorCode.RATE_LIMITED, True, None, False, False),
        (FakeFailure.CUT_AFTER_SEND, 1, ErrorCode.TIMEOUT, False, True, False, True),
        (FakeFailure.CREDENTIALS_REJECTED, 1, ErrorCode.PROVIDER_UNAVAILABLE, False, None, True, False),
        (FakeFailure.RECIPIENT_REFUSES, 1, ErrorCode.CONFLICT, False, None, False, False),
    ],
)
async def test_the_fake_simulates_each_failure_when_it_is_asked(
    failure: FakeFailure,
    times: int,
    code: ErrorCode,
    retryable: bool,
    outcome_unknown: bool | None,
    credentials_rejected: bool,
    delivered: bool,
) -> None:
    """A failure that did not send is asked for each attempt, because the fake retries it as Telegram does."""
    provider = fake()
    provider.fail_next(failure, times=times)
    with pytest.raises(ProviderFailure) as raised:
        await provider.send(delivery())
    error = raised.value
    assert (error.code, error.retryable, error.outcome_unknown) == (code, retryable, outcome_unknown)
    assert error.credentials_rejected is credentials_rejected
    assert error.from_provider is True
    if code is ErrorCode.RATE_LIMITED:
        assert error.retry_after_s is not None
        assert error.retry_after_s > 0
    assert len(provider.sent()) == (1 if delivered else 0)
    provider.clear_failures()
    await provider.send(delivery())
    assert len(provider.sent()) == (2 if delivered else 1)


async def test_with_a_directory_each_send_goes_to_the_outbox(tmp_path: Path) -> None:
    provider = fake(tmp_path)
    sent = await provider.send(delivery("uno"))
    await provider.send(delivery("dos"))
    lines = (tmp_path / "test_fake" / "outbox.jsonl").read_text(encoding="utf-8").splitlines()
    records = [json.loads(line) for line in lines]
    assert [(record["chat"], record["text"]) for record in records] == [("fake-chat-1", "uno"), ("fake-chat-1", "dos")]
    assert records[0]["provider_message_ref"] == sent.provider_message_ref
    assert records[0]["at"] == "2026-11-14T10:00:00.000000Z"
    assert set(records[0]) == {"at", "chat", "text", "provider_message_ref"}


async def test_each_message_left_in_the_inbox_is_delivered_once_and_in_order(tmp_path: Path) -> None:
    inbox = tmp_path / "test_fake" / "inbox"
    inbox.mkdir(parents=True)
    (inbox / "002.json").write_text(json.dumps({"chat": "c2", "chat_type": "private", "text": "segundo"}))
    (inbox / "001.json").write_text(json.dumps({"chat": "c1", "chat_type": "private", "text": "primero"}))
    provider = fake(tmp_path)
    received = await provider.receive("test_fake")
    assert [(item.chat, item.chat_type, item.text) for item in received] == [
        ("c1", "private", "primero"),
        ("c2", "private", "segundo"),
    ]
    assert list(inbox.iterdir()) == []
    assert await provider.receive("test_fake") == []


async def test_without_a_directory_the_inbox_is_in_memory() -> None:
    provider = fake()
    provider.deliver("test_fake", chat="c1", text="hola")
    provider.deliver("test_fake", chat="g1", text="grupo", chat_type="group")
    received = await provider.receive("test_fake")
    assert [(item.chat, item.chat_type, item.text) for item in received] == [
        ("c1", "private", "hola"),
        ("g1", "group", "grupo"),
    ]
    assert await provider.receive("test_fake") == []
    assert await provider.receive("other") == []
