"""The send with its Idempotency-Key (T090, research R11, FR-016, SC-003): repeating a request never sends twice."""

import asyncio
from collections.abc import AsyncIterator
from dataclasses import replace
from datetime import datetime, timedelta
from typing import Any

import httpx
import pytest

from hermes.application.configuration import ConfigurationState
from hermes.application.idempotency import Idempotency
from hermes.application.ports import AuditEntry, AuditKind, CallCredentials, Caller
from hermes.application.send_message import SendMessage
from hermes.domain.idempotency.record import KeyRecord, KeyState, StoredResponse
from hermes.domain.services.chat.contract import SEND_MESSAGE
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.providers.fake.adapter import FakeFailure, FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from support.clock import FakeClock
from support.configuration import DATA, FAKE_CHAT, configuration
from support.contract import contract
from support.doubles import CALLER, MemoryAudit, MemoryChannels, MemoryIdempotency, Providers, StaticSecrets

pytestmark = pytest.mark.anyio

PATH = "/chat/v1/send_message"
KEY = "case-1234-notify"
OTHER = Caller(SpiffeId.parse("spiffe://cerbero.test/tenaro"))
TOKENS = {"token-hades": CALLER, "token-tenaro": OTHER}
CUT = {"status": "failed", "code": "timeout", "outcome_unknown": True}
BODY: dict[str, Any] = {
    "recipients": ["contacto_fake"],
    "template": {"id": "device_unknown", "version": 1},
    "data": DATA,
}


class Callers:
    """Each token is one caller; the real authenticator takes it from the certificate (US2)."""

    async def authenticate(self, credentials: CallCredentials) -> Caller:
        caller = TOKENS.get((credentials.authorization or "").removeprefix("Bearer "))
        if caller is None:
            raise HermesError(ErrorCode.AUTH, "the call is not authenticated")
        return caller


class CutKeys(MemoryIdempotency):
    """The keys in memory, where a cut can stop the record of the next send after the provider."""

    def __init__(self, audit: MemoryAudit) -> None:
        super().__init__(audit)
        self.cut_next = False

    async def complete(self, caller: str, key: str, response: StoredResponse, audit: AuditEntry) -> None:
        if self.cut_next:
            self.cut_next = False
            raise HermesError(ErrorCode.UNAVAILABLE, "the database does not answer")
        await super().complete(caller, key, response, audit)


class Api:
    def __init__(self, *, requests_per_minute: int | None = None) -> None:
        self.clock = FakeClock()
        self.fake = FakeProvider(read_manifest("fake"), self.clock, sleep=self.clock.sleep, jitter=lambda: 1.0)
        self.channels = MemoryChannels()
        self.channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
        self.audit = MemoryAudit()
        self.keys = CutKeys(self.audit)
        config = configuration()
        instances = tuple(
            replace(item, requests_per_minute=requests_per_minute) if item.name == "test_fake" else item
            for item in config.instances
        )
        send = SendMessage(
            configuration=ConfigurationState(replace(config, instances=instances)),
            channels=self.channels,
            providers=Providers(fake=self.fake),
            secrets=StaticSecrets({}),
            audit=self.audit,
            clock=self.clock,
        )
        self.app = create_network_api()
        self.idempotency = Idempotency(self.keys, self.clock)
        install_authentication(self.app, Authentication(authenticator=Callers(), ready=lambda: send.ready))
        install_chat(self.app, ChatServices(send_message=send, idempotency=self.idempotency))

    def row(self, caller: Caller = CALLER, key: str = KEY) -> KeyRecord | None:
        found = self.keys.rows.get((str(caller.spiffe_id), key))
        return found[0] if found is not None else None


@pytest.fixture
def api() -> Api:
    return Api()


@pytest.fixture
async def client(api: Api) -> AsyncIterator[httpx.AsyncClient]:
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as http:
        yield http


async def post(
    client: httpx.AsyncClient,
    body: Any = BODY,
    *,
    key: str = KEY,
    bearer: str = "token-hades",
    request_id: str | None = None,
) -> httpx.Response:
    headers = {"Authorization": f"Bearer {bearer}", "Idempotency-Key": key}
    if request_id is not None:
        headers["X-Request-Id"] = request_id
    return await client.post(PATH, json=body, headers=headers)


def checked(response: httpx.Response) -> dict[str, Any]:
    body: dict[str, Any] = response.json()
    contract().assert_valid("sendMessage", response.status_code, body)
    return body


def without_timing(body: dict[str, Any]) -> dict[str, Any]:
    meta = {name: value for name, value in body["meta"].items() if name not in ("request_id", "duration_ms")}
    return {**body, "meta": meta}


async def test_the_same_key_and_body_give_the_same_response_without_sending_again(
    api: Api, client: httpx.AsyncClient
) -> None:
    first = await post(client, request_id="req-first")
    again = await post(client, request_id="req-again")
    assert (first.status_code, again.status_code) == (200, 200)
    assert without_timing(checked(again)) == without_timing(checked(first))
    assert again.json()["meta"]["request_id"] == "req-again"
    assert len(api.fake.sent()) == 1
    assert len(api.audit.entries) == 1
    row = api.row()
    assert row is not None
    assert row.state is KeyState.COMPLETED


async def test_the_same_key_with_another_body_is_a_conflict(api: Api, client: httpx.AsyncClient) -> None:
    await post(client)
    response = await post(client, {**BODY, "data": {**DATA, "ip": "192.168.1.99"}})
    assert response.status_code == 409
    body = checked(response)
    assert body["error"]["code"] == "conflict"
    assert body["meta"]["idempotency_key"] == KEY
    assert len(api.fake.sent()) == 1


async def test_the_body_counts_whatever_the_order_of_its_keys(api: Api, client: httpx.AsyncClient) -> None:
    await post(client)
    reordered = {
        "data": dict(reversed(list(DATA.items()))),
        "template": BODY["template"],
        "recipients": ["contacto_fake"],
    }
    response = await post(client, reordered)
    assert response.status_code == 200
    assert len(api.fake.sent()) == 1


async def test_the_key_belongs_to_its_caller(api: Api, client: httpx.AsyncClient) -> None:
    await post(client)
    response = await post(client, {**BODY, "data": {**DATA, "ip": "192.168.1.99"}}, bearer="token-tenaro")
    assert response.status_code == 200
    assert len(api.fake.sent()) == 2


async def test_a_hundred_equal_requests_at_once_send_one_message(api: Api, client: httpx.AsyncClient) -> None:
    responses = await asyncio.gather(*(post(client) for _ in range(100)))
    assert {response.status_code for response in responses} == {200}
    assert len({response.json()["data"]["provider_message_ref"] for response in responses}) == 1
    assert len(api.fake.sent()) == 1
    assert len(api.audit.entries) == 1


async def test_a_cut_between_the_send_and_its_record_never_sends_twice_and_says_the_outcome_is_unknown(
    api: Api, client: httpx.AsyncClient
) -> None:
    api.keys.cut_next = True
    responses = await asyncio.gather(*(post(client) for _ in range(100)))
    statuses = sorted(response.status_code for response in responses)
    assert statuses == [200] * 99 + [503]
    for response in responses:
        if response.status_code == 200:
            error = checked(response)["error"]
            assert (error["code"], error["retryable"], error["outcome_unknown"]) == ("timeout", False, True)
    assert len(api.fake.sent()) == 1
    row = api.row()
    assert row is not None
    assert row.state is KeyState.COMPLETED
    assert [entry.outcome for entry in api.audit.entries] == [CUT]


def the_cut_send(at: datetime) -> AuditEntry:
    """The entry of the send that the cut left without its record: who asked, to whom, what changed outside, what
    left home and the unknown outcome, at the time of the attempt (T122, FR-029)."""
    values = {"recipient": "contacto_fake", "title": "Dispositivo desconocido en la red", "instance": "test_fake"}
    return AuditEntry(
        at=at,
        kind=AuditKind.SEND,
        actor=str(CALLER.spiffe_id),
        outcome=CUT,
        function="chat.send_message",
        instance="test_fake",
        recipients=("contacto_fake",),
        side_effect=SEND_MESSAGE.side_effect_for({**values, "provider": "fake"}),
        discloses_data=SEND_MESSAGE.discloses_data_for({**values, "provider": "fake"}),
        audit_context=CALLER.audit_context,
        idempotency_key=KEY,
    )


async def test_after_a_cut_the_repetition_answers_timeout_and_the_key_keeps_that_answer(
    api: Api, client: httpx.AsyncClient
) -> None:
    api.keys.cut_next = True
    assert (await post(client)).status_code == 503
    row = api.row()
    assert row is not None
    assert row.state is KeyState.IN_PROGRESS
    again = await post(client)
    assert again.status_code == 200
    assert checked(again)["error"]["outcome_unknown"] is True
    row = api.row()
    assert row is not None
    assert row.state is KeyState.COMPLETED
    assert without_timing((await post(client)).json()) == without_timing(again.json())
    assert len(api.fake.sent()) == 1
    assert api.audit.entries == [the_cut_send(api.clock.now())]


async def test_a_cut_that_nobody_repeats_leaves_its_entry_when_the_purge_removes_its_key(
    api: Api, client: httpx.AsyncClient
) -> None:
    api.keys.cut_next = True
    attempt = api.clock.now()
    assert (await post(client)).status_code == 503
    assert api.audit.entries == []
    api.clock.advance(timedelta(hours=24, seconds=1))
    assert await api.idempotency.purge() == 1
    assert api.row() is None
    assert api.audit.entries == [the_cut_send(attempt)]


async def test_a_request_that_fails_before_the_provider_leaves_no_key_and_can_be_repeated(
    api: Api, client: httpx.AsyncClient
) -> None:
    api.channels.unverified("contacto_fake", "fake", "test_fake")
    assert (await post(client)).status_code == 409
    assert api.row() is None
    api.channels.verify("contacto_fake", "fake", "test_fake", FAKE_CHAT)
    response = await post(client)
    assert response.status_code == 200
    assert checked(response)["status"] == "completed"
    assert len(api.fake.sent()) == 1


async def test_a_failure_of_the_provider_is_the_response_of_its_key(api: Api, client: httpx.AsyncClient) -> None:
    api.fake.fail_next(FakeFailure.RECIPIENT_REFUSES)
    first = await post(client)
    again = await post(client)
    assert (first.status_code, again.status_code) == (200, 200)
    assert checked(again)["error"]["code"] == "conflict"
    assert without_timing(again.json()) == without_timing(first.json())
    assert api.fake.sent() == []
    assert len(api.audit.entries) == 1


async def test_an_instance_at_its_limit_answers_429_with_retry_after_before_the_provider() -> None:
    api = Api(requests_per_minute=1)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=api.app), base_url="http://hermes") as client:
        assert (await post(client)).status_code == 200
        response = await post(client, key="another-key")
    assert response.status_code == 429
    assert response.headers["Retry-After"] == "60"
    error = checked(response)["error"]
    assert (error["code"], error["retry_after_s"]) == ("rate_limited", 60)
    assert len(api.fake.sent()) == 1
    assert api.row(key="another-key") is None
