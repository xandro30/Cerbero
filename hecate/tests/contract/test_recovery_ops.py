import json

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, initialize, make_service

NEW = "a brand new passphrase"


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


@pytest.fixture
def parts():
    service = make_service()
    shares = initialize(service)
    return RequestHandler(service), shares


def test_unseal_with_two_shares(parts):
    handler, shares = parts
    reply = call(handler, "unseal", {"shares": shares[:2]})
    assert reply["status"] == "completed"
    assert reply["data"] == {"state": "unsealed", "unsealed_via": "recovery", "recommend_passphrase_change": True}


def test_unseal_with_bad_shares_codes(parts):
    handler, shares = parts
    assert call(handler, "unseal", {"shares": shares[:1]})["error"]["code"] == "validation"
    assert call(handler, "unseal", {"shares": "x"})["error"]["code"] == "validation"
    foreign = initialize(make_service())
    reply = call(handler, "unseal", {"shares": foreign[:2]})
    assert reply["error"]["code"] == "auth" and reply["error"]["retryable"] is False


def test_passphrase_change_completes_with_either_proof(parts):
    handler, shares = parts
    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    reply = call(handler, "passphrase.change", {"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW})
    assert reply["status"] == "completed" and reply["data"] == {"config_version": 2}
    reply = call(handler, "passphrase.change", {"proof": {"shares": shares[:2]}, "new_passphrase": GOOD_PASSPHRASE})
    assert reply["data"] == {"config_version": 3}


def test_passphrase_change_error_codes(parts):
    handler, shares = parts
    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    ok = {"passphrase": GOOD_PASSPHRASE}
    assert call(handler, "passphrase.change", {"proof": ok, "new_passphrase": "short"})["error"]["code"] == "validation"
    assert call(handler, "passphrase.change", {"new_passphrase": NEW})["error"]["code"] == "validation"
    assert call(handler, "passphrase.change", {"proof": "x", "new_passphrase": NEW})["error"]["code"] == "validation"
    bad = call(handler, "passphrase.change", {"proof": {"passphrase": "nope nope nope"}, "new_passphrase": NEW})
    assert bad["error"]["code"] == "auth"
    assert "nope nope nope" not in json.dumps(bad) and NEW not in json.dumps(bad)


def test_passphrase_change_while_sealed_is_conflict(parts):
    handler, _shares = parts
    reply = call(handler, "passphrase.change", {"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW})
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"state": "sealed"}
