"""Authentication on the API (T046, SC-002 on the API): every route answers `auth` to each invalid credential of
story 2, with the same message and without reaching the provider; no route is served without it."""

import json
import logging
import re
from collections.abc import AsyncIterator, Callable
from datetime import timedelta
from typing import Any

import httpx
import pytest
from fastapi.routing import APIRoute, iter_route_contexts

from hermes.application.authentication import AUTH_MESSAGE, CallAuthenticator
from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Idempotency
from hermes.application.send_message import SendMessage
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, authenticated, install_authentication
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.logging import JsonFormatter
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import DATA, FAKE_CHAT, configuration
from support.contract import contract
from support.doubles import (
    MemoryAudit,
    MemoryChannels,
    MemoryIdempotency,
    MemoryUsedTokens,
    Providers,
    StaticSecrets,
    StaticTrust,
)
from support.pki import TestCa, spiffe
from support.xfcc import HERMES, xfcc

pytestmark = pytest.mark.anyio

PATH = "/chat/v1/send_message"
BODY: dict[str, Any] = {
    "recipients": ["contacto_fake"],
    "template": {"id": "device_unknown", "version": 1},
    "data": DATA,
}


class Api:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.leaf = self.ca.issue("hades")
        self.key = self.ca.call_key("hades")
        self.revoked: list[int] = []
        self.keys: list[dict[str, Any]] = [self.key.jwk()]
        self.fake = FakeProvider(read_manifest("fake"), self.clock)
        channels = MemoryChannels()
        channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
        self.trust = StaticTrust(self.ca.root, HERMES, [], self.keys)
        state = ConfigurationState(configuration())
        audit = MemoryAudit()
        send = SendMessage(
            configuration=state,
            channels=channels,
            providers=Providers(fake=self.fake),
            secrets=StaticSecrets({}),
            audit=audit,
            clock=self.clock,
        )
        self.app = create_network_api()
        authenticator = CallAuthenticator(trust=self.trust, used_tokens=MemoryUsedTokens(), clock=self.clock)
        install_authentication(self.app, Authentication(authenticator=authenticator, ready=lambda: send.ready))
        idempotency = Idempotency(MemoryIdempotency(audit), self.clock)
        install_chat(self.app, ChatServices(send_message=send, idempotency=idempotency))

    def refresh_trust(self) -> None:
        self.trust.revocations = (self.ca.crl(self.revoked),)
        self.trust.keys = tuple(self.keys)

    def headers(self, **overrides: Any) -> dict[str, str]:
        self.refresh_trust()
        leaf = overrides.pop("leaf", self.leaf)
        ca = overrides.pop("ca", self.ca)
        token = overrides.pop(
            "token", self.key.token(issuer=spiffe("hades"), audience=HERMES, issued_at=self.clock.now())
        )
        values = {
            "X-Forwarded-Client-Cert": xfcc(leaf.certificate, [ca.intermediate]),
            "Authorization": f"Bearer {token}",
            "Idempotency-Key": "case-1234-notify",
        }
        values.update(overrides)
        return {name: value for name, value in values.items() if value is not None}


def other_ca(api: Api) -> dict[str, str]:
    other = TestCa(now=api.clock.now())
    return api.headers(leaf=other.issue("hades"), ca=other)


def expired_certificate(api: Api) -> dict[str, str]:
    leaf = api.ca.issue("hades", not_before=api.clock.now() - timedelta(days=4), lifetime=timedelta(days=3))
    return api.headers(leaf=leaf)


def revoked_certificate(api: Api) -> dict[str, str]:
    api.revoked.append(api.leaf.serial)
    return api.headers()


def token(api: Api, **claims: Any) -> str:
    fields: dict[str, Any] = {"issuer": spiffe("hades"), "audience": HERMES, "issued_at": api.clock.now()}
    fields.update(claims)
    key = fields.pop("key", api.key)
    return key.token(**fields)


def inactive_key(api: Api) -> dict[str, str]:
    api.keys[:] = [api.key.jwk(status="verify_only")]
    return api.headers()


def forged_signature(api: Api) -> dict[str, str]:
    forger = api.ca.call_key("hades")
    return api.headers(token=token(api, key=forger, kid=api.key.kid))


CASES: dict[str, Callable[[Api], dict[str, str]]] = {
    "without the header of the proxy": lambda api: api.headers(**{"X-Forwarded-Client-Cert": None}),
    "with an unreadable header": lambda api: api.headers(**{"X-Forwarded-Client-Cert": "Hash=00;URI=x"}),
    "with a certificate of another CA": other_ca,
    "with an expired certificate": expired_certificate,
    "with a revoked certificate": revoked_certificate,
    "without a token": lambda api: api.headers(Authorization=None),
    "with an expired token": lambda api: api.headers(token=token(api, issued_at=api.clock.now() - timedelta(hours=1))),
    "with a token for another audience": lambda api: api.headers(token=token(api, audience=spiffe("hecate"))),
    "with a token of another issuer": lambda api: api.headers(token=token(api, issuer=spiffe("otro"))),
    "with a key that Hécate does not publish": lambda api: api.headers(token=token(api, key=api.ca.call_key("hades"))),
    "with a key that is not active": inactive_key,
    "with a forged signature": forged_signature,
}


@pytest.fixture
def api() -> Api:
    return Api()


@pytest.fixture
async def client(api: Api) -> AsyncIterator[httpx.AsyncClient]:
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as http:
        yield http


async def test_a_call_with_valid_credentials_is_served(api: Api, client: httpx.AsyncClient) -> None:
    response = await client.post(PATH, json=BODY, headers=api.headers())
    assert response.status_code == 200
    contract().assert_valid("sendMessage", 200, response.json())
    assert len(api.fake.sent()) == 1


@pytest.mark.parametrize("case", list(CASES), ids=list(CASES))
async def test_each_invalid_credential_answers_401_with_the_same_message_and_no_effect(
    api: Api, client: httpx.AsyncClient, case: str
) -> None:
    response = await client.post(PATH, json=BODY, headers=CASES[case](api))
    assert response.status_code == 401
    body = response.json()
    contract().assert_valid("sendMessage", 401, body)
    assert body["error"] == {"code": "auth", "message": AUTH_MESSAGE, "retryable": False}
    assert api.fake.sent() == []


async def test_the_reason_of_a_refusal_carries_the_request_id_of_its_call(
    api: Api, client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.INFO, logger="hermes")
    response = await client.post(PATH, json=BODY, headers={**forged_signature(api), "X-Request-Id": "req-forged"})
    assert response.status_code == 401
    formatter = JsonFormatter()
    lines = [json.loads(formatter.format(record)) for record in caplog.records]
    [refused] = [line for line in lines if line["event"] == "auth.refused"]
    [request] = [line for line in lines if line["event"] == "api.request"]
    assert (refused["request_id"], refused["reason"]) == ("req-forged", "signature")
    assert (request["request_id"], request["result"], request["caller"]) == ("req-forged", "auth", None)


async def test_a_token_used_twice_is_refused_the_second_time(api: Api, client: httpx.AsyncClient) -> None:
    headers = api.headers()
    assert (await client.post(PATH, json=BODY, headers=headers)).status_code == 200
    again = await client.post(PATH, json=BODY, headers={**headers, "Idempotency-Key": "otra-clave"})
    assert again.status_code == 401
    assert again.json()["error"]["message"] == AUTH_MESSAGE
    assert len(api.fake.sent()) == 1


def network_routes() -> list[tuple[str, set[str], APIRoute]]:
    """Every operation of the network API, also those of the routers it includes."""
    routes = []
    for context in iter_route_contexts(create_network_api().routes):
        if isinstance(context.original_route, APIRoute):
            routes.append((str(context.path), set(context.methods or ()), context.original_route))
    return routes


def test_no_route_of_the_network_api_is_served_without_authentication() -> None:
    routes = network_routes()
    assert {path for path, _, _ in routes} >= {PATH}
    for path, _, route in routes:
        assert authenticated in {dependency.call for dependency in route.dependant.dependencies}, path


async def test_every_route_answers_401_without_credentials(api: Api, client: httpx.AsyncClient) -> None:
    for template, methods, _ in network_routes():
        path = re.sub(r"\{[^}]+\}", "x", template)
        for method in methods:
            response = await client.request(method, path, json=BODY)
            assert response.status_code == 401, f"{method} {path}"
            assert response.json()["error"]["message"] == AUTH_MESSAGE
