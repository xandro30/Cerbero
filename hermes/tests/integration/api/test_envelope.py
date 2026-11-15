"""The skeleton of the API (T012): the common envelope, X-Request-Id, the 64 KiB body and the error codes."""

from collections.abc import AsyncIterator

import httpx
import pytest
from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.app import create_api
from hermes.infrastructure.api.envelope import completed_response
from hermes.infrastructure.api.middleware import read_json
from support.contract import contract

pytestmark = pytest.mark.anyio

router = APIRouter()


@router.post("/echo")
async def echo(request: Request) -> JSONResponse:
    body = await read_json(request)
    return completed_response(request, {"echo": body})


@router.get("/conflict")
async def conflict() -> JSONResponse:
    raise HermesError(ErrorCode.CONFLICT, "the channel is not verified")


@router.get("/provider")
async def provider() -> JSONResponse:
    raise HermesError(ErrorCode.PROVIDER_UNAVAILABLE, "the provider did not answer", from_provider=True)


@router.get("/limited")
async def limited() -> JSONResponse:
    raise HermesError(ErrorCode.RATE_LIMITED, "the instance reached its limit", retry_after_s=7)


@router.get("/boom")
async def boom() -> JSONResponse:
    raise RuntimeError("cannot call https://api.telegram.org/bot123:abc with eyJx.eyJy.sig")


@pytest.fixture
async def client() -> AsyncIterator[httpx.AsyncClient]:
    transport = httpx.ASGITransport(app=create_api(routers=[router]))
    async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as http:
        yield http


def assert_common(body: dict[str, object], schema: str) -> None:
    known = contract()
    assert not known.errors(known.common_uri(schema), body)


async def test_completed_and_failed_follow_the_common_envelope(client: httpx.AsyncClient) -> None:
    done = await client.post("/echo", json={"a": 1})
    assert done.status_code == 200
    assert done.json()["data"] == {"echo": {"a": 1}}
    assert_common(done.json(), "Completed")
    failed = await client.get("/conflict")
    assert failed.status_code == 409
    assert failed.json()["error"] == {"code": "conflict", "message": "the channel is not verified", "retryable": False}
    assert_common(failed.json(), "Failed")


async def test_the_request_id_is_repeated_or_generated(client: httpx.AsyncClient) -> None:
    given = await client.post("/echo", json={}, headers={"X-Request-Id": "req-42 from hades"})
    assert given.json()["meta"]["request_id"] == "req-42 from hades"
    generated = await client.post("/echo", json={})
    assert generated.json()["meta"]["request_id"]
    assert isinstance(generated.json()["meta"]["duration_ms"], int)


@pytest.mark.parametrize("value", ["", "x" * 129])
async def test_a_request_id_out_of_its_rule_is_validation(client: httpx.AsyncClient, value: str) -> None:
    response = await client.post("/echo", json={}, headers={"X-Request-Id": value})
    assert response.status_code == 400
    assert response.json()["error"]["code"] == "validation"
    assert_common(response.json(), "Failed")


async def test_a_body_over_64_kib_is_validation(client: httpx.AsyncClient) -> None:
    big = '{"a": "' + "x" * (64 * 1024) + '"}'
    response = await client.post("/echo", content=big, headers={"Content-Type": "application/json"})
    assert response.status_code == 400
    assert response.json()["error"]["code"] == "validation"

    async def chunks() -> AsyncIterator[bytes]:
        for _ in range(70):
            yield b"x" * 1024

    streamed = await client.post("/echo", content=chunks(), headers={"Content-Type": "application/json"})
    assert streamed.status_code == 400
    assert streamed.json()["error"]["code"] == "validation"


@pytest.mark.parametrize("raw", ["NaN", '{"a": Infinity}', '{"a": 1, "a": 2}', "{'a': 1}", '{"a": 1', b"\xff\xfe"])
async def test_a_body_that_is_not_standard_json_is_validation(client: httpx.AsyncClient, raw: str | bytes) -> None:
    response = await client.post("/echo", content=raw, headers={"Content-Type": "application/json"})
    assert response.status_code == 400
    assert response.json()["error"]["code"] == "validation"


@pytest.mark.parametrize(
    ("method", "path"), [("GET", "/nowhere"), ("GET", "/echo"), ("GET", "/docs"), ("GET", "/openapi.json")]
)
async def test_an_unknown_route_or_method_is_not_found(client: httpx.AsyncClient, method: str, path: str) -> None:
    response = await client.request(method, path)
    assert response.status_code == 404
    assert response.json()["error"]["code"] == "not_found"
    assert_common(response.json(), "Failed")


async def test_an_unexpected_failure_is_internal_without_details(client: httpx.AsyncClient) -> None:
    response = await client.get("/boom")
    assert response.status_code == 500
    error = response.json()["error"]
    assert error["code"] == "internal"
    assert error["retryable"] is True
    assert "telegram" not in error["message"]
    assert "eyJ" not in response.text


async def test_a_provider_failure_goes_inside_the_envelope_with_200(client: httpx.AsyncClient) -> None:
    response = await client.get("/provider")
    assert response.status_code == 200
    assert response.json()["status"] == "failed"
    assert response.json()["error"]["code"] == "provider_unavailable"


async def test_rate_limited_carries_retry_after(client: httpx.AsyncClient) -> None:
    response = await client.get("/limited")
    assert response.status_code == 429
    assert response.headers["Retry-After"] == "7"
    assert response.json()["error"]["retry_after_s"] == 7
