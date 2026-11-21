"""The network API of Hermes with doubles of its ports: the authenticator, the channels, the secrets and the keys in
memory (or the repository of keys that the test gives it, like the PostgreSQL one), the fake provider and Telegram
against recorded answers (respx)."""

from __future__ import annotations

from typing import Any

import httpx
import respx

from hermes.application.capabilities import Capabilities, InstanceStatuses
from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Idempotency
from hermes.application.ports import IdempotencyRepository
from hermes.application.send_message import SendMessage
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_capabilities import CapabilitiesServices, install_capabilities
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.retry import RetryTiming
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, DATA, FAKE_CHAT, configuration
from support.doubles import (
    CALL_TOKEN,
    AcceptingAuthenticator,
    MemoryAudit,
    MemoryChannels,
    MemoryIdempotency,
    Providers,
    StaticSecrets,
)

HEADERS = {"Authorization": f"Bearer {CALL_TOKEN}"}
GET_ME = f"/bot{BOT_TOKEN}/getMe"
SEND = f"/bot{BOT_TOKEN}/sendMessage"


class NetworkApi:
    def __init__(
        self,
        http: httpx.AsyncClient,
        router: respx.MockRouter,
        *,
        loaded: bool = True,
        keys: IdempotencyRepository | None = None,
    ) -> None:
        self.clock = FakeClock()
        self.router = router
        self.fake = FakeProvider(read_manifest("fake"), self.clock, sleep=self.clock.sleep, jitter=lambda: 1.0)
        timing = RetryTiming(sleep=self.clock.sleep, monotonic=self.clock.monotonic, jitter=lambda: 1.0)
        telegram = TelegramProvider(read_manifest("telegram"), http, timing=timing)
        self.channels = MemoryChannels()
        self.channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
        self.channels.verify("contacto_demo", "telegram", "family_bot", CHAT)
        self.audit = MemoryAudit()
        self.statuses = InstanceStatuses()
        state = ConfigurationState(configuration() if loaded else None)
        providers = Providers(fake=self.fake, telegram=telegram)
        secrets = StaticSecrets({BOT_TOKEN_REF: BOT_TOKEN})
        send = SendMessage(
            configuration=state,
            channels=self.channels,
            providers=providers,
            secrets=secrets,
            audit=self.audit,
            clock=self.clock,
            statuses=self.statuses,
        )
        capabilities = Capabilities(
            configuration=state, providers=providers, statuses=self.statuses, secrets=secrets, clock=self.clock
        )
        self.app = create_network_api()
        install_authentication(
            self.app, Authentication(authenticator=AcceptingAuthenticator(), ready=lambda: send.ready)
        )
        idempotency = Idempotency(keys if keys is not None else MemoryIdempotency(self.audit), self.clock)
        install_chat(self.app, ChatServices(send_message=send, idempotency=idempotency))
        install_capabilities(self.app, CapabilitiesServices(capabilities=capabilities))


async def send(client: httpx.AsyncClient, key: str, recipient: str = "contacto_fake") -> httpx.Response:
    body: dict[str, Any] = {"recipients": [recipient], "template": {"id": "device_unknown", "version": 1}, "data": DATA}
    return await client.post("/chat/v1/send_message", json=body, headers={**HEADERS, "Idempotency-Key": key})


async def capabilities(client: httpx.AsyncClient) -> httpx.Response:
    return await client.get("/v1/capabilities", headers=HEADERS)


async def status_of(client: httpx.AsyncClient, instance: str) -> str:
    services = (await capabilities(client)).json()["data"]["services"]
    for service in services:
        for provider in service["providers"]:
            for item in provider["instances"]:
                if item["name"] == instance:
                    status: str = item["status"]
                    return status
    raise AssertionError(f"{instance} is not in the capabilities")


async def test_instance(client: httpx.AsyncClient, instance: str) -> httpx.Response:
    return await client.post(f"/v1/instances/{instance}/test", headers=HEADERS)


test_instance.__test__ = False  # type: ignore[attr-defined]  # a helper, not a test
