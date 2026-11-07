import json

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, initialize, make_service

PROOF = {"passphrase": GOOD_PASSPHRASE}


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


@pytest.fixture
def handler():
    service = make_service()
    initialize(service)
    h = RequestHandler(service)
    call(h, "unseal", {"passphrase": GOOD_PASSPHRASE})
    return h


def test_begin_confirm_flow(handler):
    begin = call(handler, "shares.rekey.begin", {"proof": PROOF})
    assert begin["status"] == "completed"
    assert set(begin["data"]) >= {"rekey_id", "shares", "expires_at"} and len(begin["data"]["shares"]) == 3
    confirm = call(handler, "shares.rekey.confirm",
                   {"rekey_id": begin["data"]["rekey_id"], "shares": begin["data"]["shares"][:2]})
    assert confirm["status"] == "completed" and confirm["data"] == {"config_version": 2}


def test_abort_completes_and_discards(handler):
    begin = call(handler, "shares.rekey.begin", {"proof": PROOF})
    assert call(handler, "shares.rekey.abort", {"rekey_id": begin["data"]["rekey_id"]})["data"] == {}
    again = call(handler, "shares.rekey.confirm",
                 {"rekey_id": begin["data"]["rekey_id"], "shares": begin["data"]["shares"][:2]})
    assert again["error"]["code"] == "validation"


def test_error_codes(handler):
    assert call(handler, "shares.rekey.begin", {})["error"]["code"] == "validation"
    assert call(handler, "shares.rekey.begin", {"proof": "x"})["error"]["code"] == "validation"
    bad = call(handler, "shares.rekey.begin", {"proof": {"passphrase": "nope nope nope"}})
    assert bad["error"]["code"] == "auth" and "nope nope nope" not in json.dumps(bad)
    assert call(handler, "shares.rekey.confirm", {"rekey_id": 1, "shares": []})["error"]["code"] == "validation"


def test_rekey_ops_while_sealed_are_conflicts(handler):
    call(handler, "seal")
    for op, args in (
        ("shares.rekey.begin", {"proof": PROOF}),
        ("shares.rekey.confirm", {"rekey_id": "x", "shares": ["a", "b"]}),
        ("shares.rekey.abort", {"rekey_id": "x"}),
    ):
        reply = call(handler, op, args)
        assert reply["error"]["code"] == "conflict" and reply["data"] == {"state": "sealed"}, op
