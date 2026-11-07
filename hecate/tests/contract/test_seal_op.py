import json

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, initialize, make_service


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


@pytest.fixture
def handler():
    service = make_service()
    initialize(service)
    return RequestHandler(service)


def test_seal_unsealed_completes(handler):
    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    reply = call(handler, "seal")
    assert reply["status"] == "completed" and reply["data"] == {"state": "sealed"}
    assert call(handler, "status")["data"]["state"] == "sealed"


def test_seal_when_sealed_is_conflict_with_state(handler):
    reply = call(handler, "seal")
    assert reply["error"]["code"] == "conflict"
    assert reply["error"]["retryable"] is False
    assert reply["data"] == {"state": "sealed"}


def test_seal_when_uninitialized_is_conflict():
    reply = call(RequestHandler(make_service()), "seal")
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"state": "uninitialized"}
