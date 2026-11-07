import json

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, make_service


def call(handler, op, args=None, request_id="r1"):
    raw = json.dumps({"request_id": request_id, "op": op, "args": args or {}}).encode()
    return handler.handle(raw)


def test_init_begin_confirm_flow_in_common_envelope():
    handler = RequestHandler(make_service())
    begin = call(handler, "init.begin", {"passphrase": GOOD_PASSPHRASE})
    assert begin["status"] == "completed" and begin["error"] is None
    assert begin["meta"]["request_id"] == "r1"
    assert set(begin["data"]) >= {"init_id", "shares", "expires_at", "warning"}
    assert len(begin["data"]["shares"]) == 3

    confirm = call(handler, "init.confirm",
                   {"init_id": begin["data"]["init_id"], "shares": begin["data"]["shares"][1:]})
    assert confirm["status"] == "completed"
    assert confirm["data"] == {"state": "sealed"}
    assert call(handler, "status")["data"]["state"] == "sealed"


def test_init_abort_discards_the_pending_init():
    handler = RequestHandler(make_service())
    begin = call(handler, "init.begin", {"passphrase": GOOD_PASSPHRASE})
    abort = call(handler, "init.abort", {"init_id": begin["data"]["init_id"]})
    assert abort["status"] == "completed" and abort["data"] == {}
    again = call(handler, "init.confirm",
                 {"init_id": begin["data"]["init_id"], "shares": begin["data"]["shares"][:2]})
    assert again["error"]["code"] == "validation"


def test_short_passphrase_is_validation_not_retryable():
    reply = call(RequestHandler(make_service()), "init.begin", {"passphrase": "short"})
    assert reply["status"] == "failed"
    assert reply["error"]["code"] == "validation"
    assert reply["error"]["retryable"] is False
    assert "short" not in reply["error"]["message"]


def test_missing_or_mistyped_args_are_validation():
    handler = RequestHandler(make_service())
    assert call(handler, "init.begin", {})["error"]["code"] == "validation"
    assert call(handler, "init.begin", {"passphrase": 12})["error"]["code"] == "validation"
    assert call(handler, "init.confirm", {"init_id": "x", "shares": "not a list"})["error"]["code"] == "validation"


def test_second_initialization_is_conflict_with_current_state():
    handler = RequestHandler(make_service())
    begin = call(handler, "init.begin", {"passphrase": GOOD_PASSPHRASE})
    call(handler, "init.confirm",
         {"init_id": begin["data"]["init_id"], "shares": begin["data"]["shares"][:2]})
    reply = call(handler, "init.begin", {"passphrase": GOOD_PASSPHRASE})
    assert reply["status"] == "failed"
    assert reply["error"]["code"] == "conflict"
    assert reply["data"] == {"state": "sealed"}
