"""The fixtures of the tests of the local channel.

Every answer that these tests receive from it through its socket uses the common envelope of common.v1.json, also the
errors (T130, SC-009, contracts/admin-channel.md): a common check around the transports of httpx, the sync one of the
CLI and the async one of the tests that call the channel themselves, so no test can forget it.
"""

from __future__ import annotations

import json
from collections.abc import Iterator

import httpx
import pytest

from support.contract import contract

ADMIN_HOST = "hermes"


def envelope_problems(request: httpx.Request, status: int, content: bytes) -> list[str]:
    """What an answer of the local channel breaks of the common envelope: `completed` or `failed`, as common.v1.json
    defines them."""
    where = f"{request.method} {request.url.path} {status}"
    try:
        body = json.loads(content)
    except ValueError:
        return [f"{where}: the body is not JSON"]
    known = contract()
    shape = "Completed" if isinstance(body, dict) and body.get("status") == "completed" else "Failed"
    return [f"{where}: {problem}" for problem in known.errors(known.common_uri(shape), body)]


@pytest.fixture(autouse=True)
def every_admin_answer_uses_the_common_envelope(monkeypatch: pytest.MonkeyPatch) -> Iterator[list[str]]:
    problems: list[str] = []
    sync_original = httpx.HTTPTransport.handle_request
    async_original = httpx.AsyncHTTPTransport.handle_async_request

    def checked(transport: httpx.HTTPTransport, request: httpx.Request) -> httpx.Response:
        response = sync_original(transport, request)
        if request.url.host != ADMIN_HOST:
            return response
        content = response.read()
        problems.extend(envelope_problems(request, response.status_code, content))
        return httpx.Response(response.status_code, headers=response.headers, content=content, request=request)

    async def checked_async(transport: httpx.AsyncHTTPTransport, request: httpx.Request) -> httpx.Response:
        response = await async_original(transport, request)
        if request.url.host != ADMIN_HOST:
            return response
        content = await response.aread()
        problems.extend(envelope_problems(request, response.status_code, content))
        return httpx.Response(response.status_code, headers=response.headers, content=content, request=request)

    monkeypatch.setattr(httpx.HTTPTransport, "handle_request", checked)
    monkeypatch.setattr(httpx.AsyncHTTPTransport, "handle_async_request", checked_async)
    yield problems
    assert not problems, "\n".join(problems)
