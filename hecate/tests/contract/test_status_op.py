import json
from datetime import timedelta

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, FakeClock, initialize, make_service

STATUS_FIELDS = {
    "state", "initialized_at", "unsealed_via", "recommend_passphrase_change",
    "consecutive_failures", "retry_after_s",
}


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


def test_status_when_uninitialized():
    reply = call(RequestHandler(make_service()), "status")
    assert reply["status"] == "completed" and reply["error"] is None
    assert set(reply["data"]) == STATUS_FIELDS
    assert reply["data"] | {} == {
        "state": "uninitialized", "initialized_at": None, "unsealed_via": None,
        "recommend_passphrase_change": False, "consecutive_failures": 0, "retry_after_s": 0,
    }


def test_status_when_sealed_unsealed_and_after_recovery():
    service = make_service()
    shares = initialize(service)
    handler = RequestHandler(service)

    sealed = call(handler, "status")["data"]
    assert set(sealed) == STATUS_FIELDS
    assert sealed["state"] == "sealed" and sealed["initialized_at"] and sealed["unsealed_via"] is None

    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    unsealed = call(handler, "status")["data"]
    assert (unsealed["state"], unsealed["unsealed_via"], unsealed["recommend_passphrase_change"]) == (
        "unsealed", "passphrase", False)

    call(handler, "seal")
    call(handler, "unseal", {"shares": shares[:2]})
    recovered = call(handler, "status")["data"]
    assert (recovered["unsealed_via"], recovered["recommend_passphrase_change"]) == ("recovery", True)


def test_status_reports_failures_and_the_pending_wait():
    clock = FakeClock()
    service = make_service(clock=clock)
    initialize(service)
    handler = RequestHandler(service)
    for _ in range(3):
        call(handler, "unseal", {"passphrase": "wrong wrong wrong"})
    data = call(handler, "status")["data"]
    assert (data["consecutive_failures"], data["retry_after_s"]) == (3, 60)
    clock.advance(timedelta(seconds=30))
    assert call(handler, "status")["data"]["retry_after_s"] == 30


@pytest.mark.parametrize("op", ["status", "seal"])
def test_internal_error_hides_the_cause_and_is_retryable(op):
    service = make_service()
    initialize(service)

    def boom(*_a, **_k):
        raise RuntimeError("secret detail: /var/lib/hecate/hecate.db is locked")

    service.status = boom
    service.seal = boom
    reply = call(RequestHandler(service), op)

    assert reply["status"] == "failed"
    assert reply["error"] == {"code": "internal", "message": "internal error", "retryable": True}
    assert reply["data"] == {}
    assert "secret detail" not in json.dumps(reply)
    assert reply["meta"]["request_id"] == "r"
