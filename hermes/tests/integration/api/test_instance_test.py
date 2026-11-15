"""`POST /v1/instances/{name}/test` (T100, contracts/api.md, FR-014): the connection test of an instance, with `getMe`
in Telegram; its failure goes inside the envelope and leaves the instance unavailable until a test works."""

from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest
import respx

from hermes.infrastructure.providers.fake.adapter import FakeFailure
from support.contract import contract
from support.network_api import GET_ME, HEADERS, NetworkApi, status_of, test_instance
from support.telegram import API, recording

pytestmark = pytest.mark.anyio


@pytest.fixture
async def world() -> AsyncIterator[tuple[NetworkApi, httpx.AsyncClient]]:
    async with httpx.AsyncClient() as http:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            api = NetworkApi(http, router)
            transport = httpx.ASGITransport(app=api.app)
            async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as client:
                yield api, client


def checked(response: httpx.Response) -> dict[str, Any]:
    body: dict[str, Any] = response.json()
    contract().assert_valid("testInstance", response.status_code, body)
    return body


async def test_a_working_get_me_leaves_the_instance_healthy(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    api, client = world
    route = api.router.post(GET_ME).mock(return_value=recording("get_me_ok"))
    response = await test_instance(client, "family_bot")
    assert response.status_code == 200
    body = checked(response)
    assert body["data"] == {"instance": "family_bot", "status": "healthy", "checked_at": "2026-11-14T10:00:00.000000Z"}
    assert body["meta"]["provider"] == "telegram"
    assert route.call_count == 1


@pytest.mark.parametrize(
    ("answer", "retryable"),
    [
        (recording("get_me_401"), False),
        (httpx.ConnectError("refused"), True),
        (httpx.ReadTimeout("slow"), True),
        (httpx.Response(502, json={"ok": False, "error_code": 502, "description": "Bad Gateway"}), True),
    ],
    ids=["401", "ConnectError", "ReadTimeout", "502"],
)
async def test_a_failed_test_answers_inside_the_envelope_and_leaves_the_instance_unavailable(
    world: tuple[NetworkApi, httpx.AsyncClient], answer: httpx.Response | Exception, retryable: bool
) -> None:
    api, client = world
    api.router.post(GET_ME).mock(side_effect=[answer])
    response = await test_instance(client, "family_bot")
    assert response.status_code == 200
    error = checked(response)["error"]
    assert (error["code"], error["retryable"]) == ("provider_unavailable", retryable)
    assert await status_of(client, "family_bot") == "unavailable"
    api.router.post(GET_ME).mock(return_value=recording("get_me_ok"))
    assert checked(await test_instance(client, "family_bot"))["data"]["status"] == "healthy"


async def test_a_limit_of_the_provider_during_the_test_is_rate_limited_inside_the_envelope(
    world: tuple[NetworkApi, httpx.AsyncClient],
) -> None:
    api, client = world
    api.router.post(GET_ME).mock(return_value=recording("send_message_429"))
    response = await test_instance(client, "family_bot")
    assert response.status_code == 200
    assert "Retry-After" not in response.headers
    error = checked(response)["error"]
    assert (error["code"], error["retry_after_s"]) == ("rate_limited", 60)


async def test_the_fake_instance_answers_its_test(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    api, client = world
    assert checked(await test_instance(client, "test_fake"))["data"]["status"] == "healthy"
    api.fake.fail_next(FakeFailure.CREDENTIALS_REJECTED)
    error = checked(await test_instance(client, "test_fake"))["error"]
    assert (error["code"], error["retryable"]) == ("provider_unavailable", False)
    assert await status_of(client, "test_fake") == "unavailable"


async def test_an_unknown_instance_is_not_found(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    _, client = world
    response = await test_instance(client, "otro_bot")
    assert response.status_code == 404
    assert checked(response)["error"]["code"] == "not_found"


async def test_a_name_that_is_not_a_name_is_validation(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    _, client = world
    response = await client.post("/v1/instances/Family-Bot/test", headers=HEADERS)
    assert response.status_code == 400
    assert checked(response)["error"]["code"] == "validation"


async def test_the_test_takes_no_idempotency_key(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    api, client = world
    api.router.post(GET_ME).mock(return_value=recording("get_me_ok"))
    response = await test_instance(client, "family_bot")
    assert "Idempotency-Key" not in HEADERS
    assert response.status_code == 200
    assert "idempotency_key" not in response.json()["meta"]
