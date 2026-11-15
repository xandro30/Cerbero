"""The common check of tests/integration/conftest.py sees an answer out of the contract (T107): without this, a check
that saw nothing would pass every test."""

from __future__ import annotations

import httpx
import pytest
from fastapi import FastAPI
from fastapi.responses import JSONResponse

pytestmark = pytest.mark.anyio


async def test_an_answer_out_of_the_contract_is_seen(every_answer_follows_the_contract: list[str]) -> None:
    app = FastAPI()

    @app.get("/v1/capabilities")
    async def wrong() -> JSONResponse:
        return JSONResponse({"status": "maybe"}, status_code=200)

    @app.post("/v1/instances/{name}/test")
    async def unknown_status(name: str) -> JSONResponse:
        return JSONResponse({"status": "completed"}, status_code=418)

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://hermes") as client:
        await client.get("/v1/capabilities")
        await client.post("/v1/instances/family_bot/test")
        await client.get("/not/in/the/contract")
    seen = list(every_answer_follows_the_contract)
    every_answer_follows_the_contract.clear()
    assert [problem.split(":")[0] for problem in seen][:1] == ["GET /v1/capabilities 200"]
    assert "POST /v1/instances/family_bot/test 418: the contract has no answer with this status" in seen
    assert not any("/not/in/the/contract" in problem for problem in seen)
