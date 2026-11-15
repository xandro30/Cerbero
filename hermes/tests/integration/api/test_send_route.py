"""The route POST /chat/v1/send_message (T034), with a double of the authenticator and the fake provider."""

from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Idempotency
from hermes.application.send_message import SendMessage
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.providers.fake.adapter import FakeFailure, FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import DATA, FAKE_CHAT, configuration
from support.contract import contract
from support.doubles import (
    CALL_TOKEN,
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


class Api:
    def __init__(self, *, loaded: bool = True) -> None:
        self.clock = FakeClock()
        self.fake = FakeProvider(read_manifest("fake"), self.clock, sleep=self.clock.sleep, jitter=lambda: 1.0)
        self.channels = MemoryChannels()
        self.channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
        self.audit = MemoryAudit()
        send = SendMessage(
            configuration=ConfigurationState(configuration() if loaded else None),
            channels=self.channels,
            providers=Providers(fake=self.fake),
            secrets=StaticSecrets({}),
            audit=self.audit,
            clock=self.clock,
        )
        self.app = create_network_api()
        install_authentication(
            self.app, Authentication(authenticator=AcceptingAuthenticator(), ready=lambda: send.ready)
        )
        idempotency = Idempotency(MemoryIdempotency(self.audit), self.clock)
        install_chat(self.app, ChatServices(send_message=send, idempotency=idempotency))


@pytest.fixture
def api() -> Api:
    return Api()


@pytest.fixture
async def client(api: Api) -> AsyncIterator[httpx.AsyncClient]:
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as http:
        yield http


async def post(client: httpx.AsyncClient, body: Any = BODY, headers: dict[str, Any] | None = None) -> httpx.Response:
    return await client.post(PATH, json=body, headers=HEADERS if headers is None else headers)


def checked(response: httpx.Response) -> dict[str, Any]:
    body: dict[str, Any] = response.json()
    contract().assert_valid("sendMessage", response.status_code, body)
    return body


async def test_a_send_completes_with_its_result_and_its_meta(api: Api, client: httpx.AsyncClient) -> None:
    response = await post(client)
    assert response.status_code == 200
    body = checked(response)
    assert body["status"] == "completed"
    [sent] = api.fake.sent()
    assert body["data"] == {
        "recipient": "contacto_fake",
        "instance": "test_fake",
        "provider_message_ref": sent.provider_message_ref,
        "sent_at": "2026-11-14T10:00:00.000000Z",
        "truncated": False,
    }
    assert body["meta"]["service"] == "chat"
    assert body["meta"]["provider"] == "fake"
    assert body["meta"]["idempotency_key"] == KEY
    assert FAKE_CHAT not in response.text
    assert [entry.idempotency_key for entry in api.audit.entries] == [KEY]


@pytest.mark.parametrize("key", [None, "", "x" * 129, "clé".encode(), "tab\tkey"])
async def test_the_idempotency_key_is_required_with_1_to_128_printable_ascii_characters(
    api: Api, client: httpx.AsyncClient, key: str | bytes | None
) -> None:
    headers: dict[str, Any] = {"Authorization": f"Bearer {CALL_TOKEN}"}
    if key is not None:
        headers["Idempotency-Key"] = key
    response = await post(client, headers=headers)
    assert response.status_code == 400
    body = checked(response)
    assert body["error"]["code"] == "validation"
    assert "idempotency_key" not in body["meta"]
    assert api.fake.sent() == []


async def test_a_key_of_128_printable_characters_is_accepted(client: httpx.AsyncClient) -> None:
    key = "".join(chr(32 + index % 95) for index in range(128))
    response = await post(client, headers={**HEADERS, "Idempotency-Key": key})
    assert response.status_code == 200
    assert checked(response)["meta"]["idempotency_key"] == key


@pytest.mark.parametrize(
    ("body", "path"),
    [
        ({**BODY, "priority": "high"}, "priority"),
        ({**BODY, "template": {"id": "device_unknown", "version": 1, "locale": "es"}}, "template.locale"),
        ({**BODY, "template": {"id": "device_unknown", "version": "1"}}, "template.version"),
        ({**BODY, "instance": None}, "instance"),
        ({"recipients": ["contacto_fake"], "data": DATA}, "template"),
        ({**BODY, "recipients": ["contacto_fake", "contacto_demo"]}, "recipients"),
        ({**BODY, "recipients": []}, "recipients"),
        ({**BODY, "data": ["no", "es", "un", "objeto"]}, "data"),
    ],
)
async def test_a_body_out_of_its_contract_is_validation(
    api: Api, client: httpx.AsyncClient, body: dict[str, Any], path: str
) -> None:
    response = await post(client, body=body)
    assert response.status_code == 400
    failed = checked(response)
    assert failed["error"]["code"] == "validation"
    assert path in [detail["path"] for detail in failed["data"]["errors"]]
    assert api.fake.sent() == []


async def test_a_body_that_is_not_an_object_is_validation(client: httpx.AsyncClient) -> None:
    response = await post(client, body=["contacto_fake"])
    assert response.status_code == 400
    assert checked(response)["error"]["code"] == "validation"


@pytest.mark.parametrize(
    ("failure", "times", "code", "retryable"),
    [
        (FakeFailure.UNREACHABLE, 20, "provider_unavailable", True),
        (FakeFailure.RATE_LIMITED, 1, "rate_limited", True),
        (FakeFailure.CUT_AFTER_SEND, 1, "timeout", False),
        (FakeFailure.RECIPIENT_REFUSES, 1, "conflict", False),
    ],
)
async def test_a_failure_of_the_provider_answers_200_inside_the_envelope(
    api: Api, client: httpx.AsyncClient, failure: FakeFailure, times: int, code: str, retryable: bool
) -> None:
    api.fake.fail_next(failure, times=times)
    response = await post(client)
    assert response.status_code == 200
    body = checked(response)
    assert body["status"] == "failed"
    assert (body["error"]["code"], body["error"]["retryable"]) == (code, retryable)
    assert body["meta"]["provider"] == "fake"
    assert body["meta"]["idempotency_key"] == KEY
    assert "Retry-After" not in response.headers
    assert len(api.audit.entries) == 1


async def test_a_timeout_says_that_the_outcome_is_unknown(api: Api, client: httpx.AsyncClient) -> None:
    api.fake.fail_next(FakeFailure.CUT_AFTER_SEND)
    body = checked(await post(client))
    assert body["error"]["outcome_unknown"] is True


async def test_a_channel_that_is_not_verified_answers_409(api: Api, client: httpx.AsyncClient) -> None:
    api.channels.unverified("contacto_fake", "fake", "test_fake")
    response = await post(client)
    assert response.status_code == 409
    assert checked(response)["error"]["code"] == "conflict"


@pytest.mark.parametrize(
    ("body", "status", "code"),
    [
        ({**BODY, "recipients": ["nadie"]}, 404, "not_found"),
        ({**BODY, "template": {"id": "missing"}}, 404, "not_found"),
        ({**BODY, "instance": "family_bot"}, 404, "not_found"),
        ({**BODY, "data": {**DATA, "mac": "no-es-una-mac"}}, 400, "validation"),
    ],
)
async def test_a_failure_before_the_provider_answers_with_the_status_of_its_code(
    api: Api, client: httpx.AsyncClient, body: dict[str, Any], status: int, code: str
) -> None:
    response = await post(client, body=body)
    assert response.status_code == status
    failed = checked(response)
    assert failed["error"]["code"] == code
    assert failed["meta"]["idempotency_key"] == KEY
    assert api.fake.sent() == []
    assert api.audit.entries == []


@pytest.mark.parametrize("authorization", [None, "Bearer otro-token", "Basic abc"])
async def test_a_call_that_is_not_authenticated_answers_401(
    api: Api, client: httpx.AsyncClient, authorization: str | None
) -> None:
    headers = {"Idempotency-Key": KEY}
    if authorization is not None:
        headers["Authorization"] = authorization
    response = await post(client, headers=headers)
    assert response.status_code == 401
    assert checked(response)["error"]["code"] == "auth"
    assert api.fake.sent() == []


async def test_hermes_without_its_configuration_is_unavailable_before_anything_else() -> None:
    api = Api(loaded=False)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as client:
        response = await post(client, headers={})
    assert response.status_code == 503
    assert checked(response)["error"]["code"] == "unavailable"


async def test_the_route_without_its_services_is_unavailable() -> None:
    app = create_network_api()
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://hermes") as client:
        response = await post(client)
    assert response.status_code == 503
    assert checked(response)["error"]["code"] == "unavailable"
