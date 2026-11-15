"""`GET /v1/capabilities` (T099, contracts/api.md#get-v1capabilities, FR-013): what Hermes offers and the state of each
instance, without contacts or addresses."""

from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest
import respx

from hermes.infrastructure.providers.fake.adapter import FakeFailure
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, FAKE_CHAT
from support.contract import contract
from support.network_api import NetworkApi, capabilities, send, status_of
from support.telegram import API

pytestmark = pytest.mark.anyio

SEND_MESSAGE = {"name": "send_message", "kind": "command", "mode": "sync", "timeout_s": 10}


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
    contract().assert_valid("getCapabilities", response.status_code, body)
    return body


async def test_the_capabilities_say_each_provider_its_functions_and_the_state_of_its_instances(
    world: tuple[NetworkApi, httpx.AsyncClient],
) -> None:
    _, client = world
    response = await capabilities(client)
    assert response.status_code == 200
    assert checked(response)["data"] == {
        "component": "hermes",
        "services": [
            {
                "category": "communication",
                "service": "chat",
                "contract": "v1",
                "providers": [
                    {
                        "provider": "fake",
                        "version": "1.0.0",
                        "functions": [SEND_MESSAGE],
                        "instances": [{"name": "test_fake", "status": "healthy"}],
                    },
                    {
                        "provider": "telegram",
                        "version": "1.0.0",
                        "functions": [SEND_MESSAGE],
                        "instances": [{"name": "family_bot", "status": "healthy"}],
                    },
                ],
            }
        ],
    }


async def test_no_contact_address_or_secret_reference_shows(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    _, client = world
    text = (await capabilities(client)).text
    for leak in ("contacto_demo", "contacto_fake", "dos_canales", CHAT, FAKE_CHAT, BOT_TOKEN, BOT_TOKEN_REF):
        assert leak not in text


async def test_the_failed_calls_of_an_instance_degrade_it_and_three_in_a_row_make_it_unavailable(
    world: tuple[NetworkApi, httpx.AsyncClient],
) -> None:
    api, client = world
    api.fake.fail_next(FakeFailure.UNREACHABLE, times=1000)
    await send(client, "first")
    assert await status_of(client, "test_fake") == "degraded"
    await send(client, "second")
    await send(client, "third")
    assert await status_of(client, "test_fake") == "unavailable"
    api.fake.clear_failures()
    assert (await send(client, "fourth")).json()["status"] == "completed"
    assert await status_of(client, "test_fake") == "degraded"
    assert await status_of(client, "family_bot") == "healthy"


async def test_a_person_that_refuses_the_bot_or_a_limit_does_not_count_against_the_instance(
    world: tuple[NetworkApi, httpx.AsyncClient],
) -> None:
    api, client = world
    api.fake.fail_next(FakeFailure.RECIPIENT_REFUSES)
    await send(client, "refused")
    api.fake.fail_next(FakeFailure.RATE_LIMITED)
    await send(client, "limited")
    assert await status_of(client, "test_fake") == "healthy"


async def test_rejected_credentials_make_the_instance_unavailable(world: tuple[NetworkApi, httpx.AsyncClient]) -> None:
    api, client = world
    api.fake.fail_next(FakeFailure.CREDENTIALS_REJECTED)
    await send(client, "rejected")
    assert await status_of(client, "test_fake") == "unavailable"


async def test_hermes_without_its_configuration_is_unavailable() -> None:
    async with httpx.AsyncClient() as http:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            api = NetworkApi(http, router, loaded=False)
            async with httpx.AsyncClient(
                transport=httpx.ASGITransport(app=api.app), base_url="http://hermes"
            ) as client:
                response = await capabilities(client)
    assert response.status_code == 503
    assert checked(response)["error"]["code"] == "unavailable"
