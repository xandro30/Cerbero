"""The line of each request to the network API (T132, research R19): when it ends, one `api.request` line with its
`request_id`, its operation, the caller, its `Idempotency-Key`, its result, its HTTP status and its duration; and every
other line that the request leaves carries its `request_id` too. The local channel leaves its own line per operation
instead, with the `request_id` of its request."""

import json
import logging
import re
from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest
from fastapi.routing import APIRoute, iter_route_contexts

from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Idempotency
from hermes.application.send_message import SendMessage
from hermes.infrastructure.admin.app import create_admin_app
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.logging import JsonFormatter
from hermes.infrastructure.providers.fake.adapter import FakeFailure, FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import DATA, FAKE_CHAT, configuration
from support.contract import contract
from support.doubles import (
    CALL_TOKEN,
    CALLER,
    AcceptingAuthenticator,
    MemoryAudit,
    MemoryChannels,
    MemoryIdempotency,
    Providers,
    StaticSecrets,
)

pytestmark = pytest.mark.anyio

PATH = "/chat/v1/send_message"
KEY = "case-1234-notify"
HEADERS = {"Authorization": f"Bearer {CALL_TOKEN}", "Idempotency-Key": KEY}
BODY: dict[str, Any] = {
    "recipients": ["contacto_fake"],
    "template": {"id": "device_unknown", "version": 1},
    "data": DATA,
}
FIELDS = {"request_id", "operation", "caller", "idempotency_key", "result", "status", "duration_ms"}


class Api:
    """The network API with a double of the authenticator and the fake provider, whose waits take no time."""

    def __init__(self) -> None:
        self.clock = FakeClock()
        self.fake = FakeProvider(read_manifest("fake"), self.clock, sleep=self.clock.sleep, jitter=lambda: 1.0)
        channels = MemoryChannels()
        channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
        audit = MemoryAudit()
        send = SendMessage(
            configuration=ConfigurationState(configuration()),
            channels=channels,
            providers=Providers(fake=self.fake),
            secrets=StaticSecrets({}),
            audit=audit,
            clock=self.clock,
        )
        self.app = create_network_api()
        install_authentication(
            self.app, Authentication(authenticator=AcceptingAuthenticator(), ready=lambda: send.ready)
        )
        idempotency = Idempotency(MemoryIdempotency(audit), self.clock)
        install_chat(self.app, ChatServices(send_message=send, idempotency=idempotency))


@pytest.fixture
def api() -> Api:
    return Api()


@pytest.fixture
async def client(api: Api, caplog: pytest.LogCaptureFixture) -> AsyncIterator[httpx.AsyncClient]:
    caplog.set_level(logging.INFO, logger="hermes")
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as http:
        yield http


def logged(caplog: pytest.LogCaptureFixture) -> list[dict[str, Any]]:
    """Every line that the tests left, as the JSON that Hermes writes."""
    formatter = JsonFormatter()
    return [json.loads(formatter.format(record)) for record in caplog.records]


def request_lines(caplog: pytest.LogCaptureFixture) -> list[dict[str, Any]]:
    return [line for line in logged(caplog) if line["event"] == "api.request"]


def the_line(caplog: pytest.LogCaptureFixture) -> dict[str, Any]:
    (line,) = request_lines(caplog)
    assert set(line) >= FIELDS
    assert isinstance(line["duration_ms"], int)
    assert line["duration_ms"] >= 0
    return line


async def test_a_completed_send_leaves_one_line_with_its_request(
    client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    response = await client.post(PATH, json=BODY, headers={**HEADERS, "X-Request-Id": "req-done"})
    assert response.status_code == 200
    line = the_line(caplog)
    assert {name: line[name] for name in FIELDS - {"duration_ms"}} == {
        "request_id": "req-done",
        "operation": "sendMessage",
        "caller": str(CALLER.spiffe_id),
        "idempotency_key": KEY,
        "result": "completed",
        "status": 200,
    }
    assert line["level"] == "info"
    assert line["logger"] == "hermes.api"
    output = caplog.text + json.dumps(logged(caplog))
    assert FAKE_CHAT not in output
    assert "Dispositivo desconocido" not in output


async def test_a_failure_of_the_provider_leaves_its_code_and_every_line_of_the_request_its_request_id(
    api: Api, client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    api.fake.fail_next(FakeFailure.UNREACHABLE, times=20)
    response = await client.post(PATH, json=BODY, headers=HEADERS)
    assert response.status_code == 200
    request_id = response.json()["meta"]["request_id"]
    line = the_line(caplog)
    assert (line["result"], line["status"], line["request_id"]) == ("provider_unavailable", 200, request_id)
    retries = [item for item in logged(caplog) if item["event"] == "provider.retry"]
    assert retries
    assert {item.get("request_id") for item in retries} == {request_id}


async def test_a_body_out_of_its_contract_leaves_validation_with_the_caller_and_the_key(
    client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    response = await client.post(PATH, json={**BODY, "priority": "high"}, headers=HEADERS)
    assert response.status_code == 400
    line = the_line(caplog)
    assert (line["result"], line["status"]) == ("validation", 400)
    assert (line["caller"], line["idempotency_key"]) == (str(CALLER.spiffe_id), KEY)


async def test_an_idempotency_key_out_of_its_rule_is_not_written(
    client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    bad = "x" * 129
    response = await client.post(PATH, json=BODY, headers={**HEADERS, "Idempotency-Key": bad})
    assert response.status_code == 400
    line = the_line(caplog)
    assert (line["result"], line["idempotency_key"]) == ("validation", None)
    assert bad not in caplog.text + json.dumps(logged(caplog))


async def test_a_call_that_is_not_authenticated_leaves_auth_without_a_caller(
    api: Api, client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    response = await client.post(PATH, json=BODY, headers={**HEADERS, "Authorization": "Bearer otro-token"})
    assert response.status_code == 401
    line = the_line(caplog)
    assert (line["operation"], line["result"], line["status"], line["caller"]) == ("sendMessage", "auth", 401, None)
    assert "otro-token" not in caplog.text + json.dumps(logged(caplog))
    assert api.fake.sent() == []


def network_routes() -> list[tuple[str, set[str]]]:
    routes = []
    for context in iter_route_contexts(create_network_api().routes):
        if isinstance(context.original_route, APIRoute):
            routes.append((str(context.path), set(context.methods or ())))
    return routes


async def test_every_route_names_the_operation_of_the_contract(
    client: httpx.AsyncClient, caplog: pytest.LogCaptureFixture
) -> None:
    known = contract()
    for template, methods in network_routes():
        path = re.sub(r"\{[^}]+\}", "test_fake", template)
        for method in methods:
            caplog.clear()
            await client.request(method, path, json=BODY, headers=HEADERS)
            line = the_line(caplog)
            assert line["operation"] == known.operation_of(method, path), f"{method} {path}"
            assert line["operation"] is not None


@pytest.mark.parametrize(
    ("method", "path", "headers", "content", "status", "result"),
    [
        ("GET", "/v1/nothing", HEADERS, None, 404, "not_found"),
        ("GET", PATH, HEADERS, None, 404, "not_found"),
        ("POST", PATH, {**HEADERS, "X-Request-Id": "tab\tid"}, json.dumps(BODY), 400, "validation"),
        ("POST", PATH, HEADERS, b"x" * (64 * 1024 + 1), 400, "validation"),
    ],
    ids=["an unknown route", "a method that the route does not take", "a bad X-Request-Id", "a body too large"],
)
async def test_a_request_that_reaches_no_operation_leaves_its_line_without_one(
    client: httpx.AsyncClient,
    caplog: pytest.LogCaptureFixture,
    method: str,
    path: str,
    headers: dict[str, str],
    content: str | bytes | None,
    status: int,
    result: str,
) -> None:
    response = await client.request(method, path, headers=headers, content=content)
    assert response.status_code == status
    line = the_line(caplog)
    assert (line["operation"], line["caller"], line["result"], line["status"]) == (None, None, result, status)
    assert line["request_id"] == response.json()["meta"]["request_id"]


async def test_the_local_channel_leaves_its_own_line_with_the_request_id(caplog: pytest.LogCaptureFixture) -> None:
    caplog.set_level(logging.INFO, logger="hermes")
    transport = httpx.ASGITransport(app=create_admin_app())
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as admin:
        headers = {"X-Request-Id": "req-admin"}
        response = await admin.post("/config/plan", json={"yaml": "version: 1\n"}, headers=headers)
    assert response.json()["error"]["code"] == "unavailable"
    assert request_lines(caplog) == []
    [line] = [item for item in logged(caplog) if item["event"] == "config.plan"]
    assert (line["request_id"], line["result"]) == ("req-admin", "unavailable")
