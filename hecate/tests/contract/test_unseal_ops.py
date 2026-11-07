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


def test_unseal_with_passphrase_completes(handler):
    reply = call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    assert reply["status"] == "completed"
    assert reply["data"]["state"] == "unsealed"
    assert reply["data"]["unsealed_via"] == "passphrase"
    assert call(handler, "status")["data"]["state"] == "unsealed"


def test_wrong_passphrase_is_auth_not_retryable(handler):
    reply = call(handler, "unseal", {"passphrase": "wrong wrong wrong"})
    assert reply["status"] == "failed"
    assert reply["error"]["code"] == "auth" and reply["error"]["retryable"] is False
    assert "wrong wrong wrong" not in json.dumps(reply)


def test_rate_limited_carries_retry_after_s(handler):
    for _ in range(3):
        call(handler, "unseal", {"passphrase": "wrong wrong wrong"})
    reply = call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    assert reply["error"]["code"] == "rate_limited"
    assert reply["error"]["retryable"] is True
    assert reply["error"]["retry_after_s"] == 60


def test_unseal_when_unsealed_is_conflict_with_state(handler):
    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    reply = call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    assert reply["error"]["code"] == "conflict"
    assert reply["data"] == {"state": "unsealed"}


def test_unseal_needs_exactly_one_proof(handler):
    assert call(handler, "unseal", {})["error"]["code"] == "validation"
    both = {"passphrase": GOOD_PASSPHRASE, "shares": ["x", "y"]}
    assert call(handler, "unseal", both)["error"]["code"] == "validation"
    assert call(handler, "unseal", {"passphrase": 5})["error"]["code"] == "validation"


# FR-007: while sealed, only `unseal` and `status` are accepted. Every other operation, with
# well-formed arguments, answers `conflict` and reports the state.
OTHER_OPS = [
    ("init.begin", {"passphrase": GOOD_PASSPHRASE}),
    ("init.confirm", {"init_id": "x", "shares": ["a", "b"]}),
    ("init.abort", {"init_id": "x"}),
    ("seal", {}),
    ("passphrase.change", {"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": "a brand new passphrase"}),
    ("shares.rekey.begin", {"proof": {"passphrase": GOOD_PASSPHRASE}}),
    ("shares.rekey.confirm", {"rekey_id": "x", "shares": ["a", "b"]}),
    ("shares.rekey.abort", {"rekey_id": "x"}),
]


@pytest.mark.parametrize(("op", "args"), OTHER_OPS, ids=[o for o, _ in OTHER_OPS])
def test_sealed_rejects_every_other_operation(handler, op, args):
    reply = call(handler, op, args)
    assert reply["status"] == "failed"
    assert reply["error"]["code"] == "conflict"
    assert reply["data"] == {"state": "sealed"}


# Every recognized operation except `status` leaves a row in the access log, also when its
# arguments are malformed (contract: control-protocol.md, common rules).
MALFORMED = [
    ("init.begin", {}, "init"),
    ("init.confirm", {"init_id": 1, "shares": []}, "init"),
    ("unseal", {}, "unseal"),
    ("unseal", {"passphrase": 5}, "unseal"),
    ("unseal", {"shares": "x"}, "unseal"),
    ("passphrase.change", {"proof": "x", "new_passphrase": "y"}, "passphrase_change"),
    ("shares.rekey.begin", {}, "shares_rekey"),
    ("shares.rekey.confirm", {"rekey_id": 1}, "shares_rekey"),
    ("shares.rekey.abort", {}, "shares_rekey"),
]


@pytest.mark.parametrize(("op", "args", "operation"), MALFORMED, ids=[f"{o}-{i}" for i, (o, _, _) in enumerate(MALFORMED)])
def test_malformed_arguments_leave_an_invalid_input_row(op, args, operation):
    from tests.fakes import InMemoryAccessLog, InMemorySealRepository

    log = InMemoryAccessLog()
    service = make_service(repo=InMemorySealRepository(), access_log=log)
    initialize(service)
    handler = RequestHandler(service)
    before = len(log.entries())

    reply = call(handler, op, args)

    assert reply["error"]["code"] == "validation"
    added = log.entries()[before:]
    assert [(e.operation.value, e.result.value) for e in added] == [(operation, "invalid_input")]
    assert not any("x" == e.detail for e in added)


def test_unknown_operations_and_status_leave_no_row():
    from tests.fakes import InMemoryAccessLog, InMemorySealRepository

    log = InMemoryAccessLog()
    handler = RequestHandler(make_service(repo=InMemorySealRepository(), access_log=log))
    call(handler, "status")
    call(handler, "no.such.op")
    assert log.entries() == []
