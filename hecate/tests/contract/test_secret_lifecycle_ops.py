import json
from datetime import timedelta

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, SecretsRig

PATH = "secret://hermes/telegram/bot_token"
RIGHT = {"passphrase": GOOD_PASSPHRASE}
WRONG = {"passphrase": "wrong wrong wrong"}


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


@pytest.fixture
def env():
    """Version 1 deprecated, version 2 active; four events per version pair."""
    rig = SecretsRig()
    handler = RequestHandler(rig.sealing, rig.service)
    call(handler, "secrets.create", {"path": PATH, "value": "value-one", "value_type": "string"})
    call(handler, "secrets.add_version", {"path": PATH, "value": "value-two", "expected_version": 2})
    return rig, handler


def stream(handler) -> int:
    return call(handler, "secrets.get", {"path": PATH})["data"]["stream_version"]


def states(handler) -> dict:
    return {v["version"]: v["state"] for v in call(handler, "secrets.get", {"path": PATH})["data"]["versions"]}


# -- revoke, restore, promote ------------------------------------------------------------------------


def test_revoke_restore_and_promote_reply_with_the_stream_version(env):
    _, handler = env
    for op, version, expected_state in [
        ("secrets.revoke", 1, "revoked"),
        ("secrets.restore", 1, "deprecated"),
        ("secrets.promote", 1, "active"),
    ]:
        before = stream(handler)
        reply = call(handler, op, {"path": PATH, "version": 1, "expected_version": before})
        assert reply["status"] == "completed" and reply["error"] is None, (op, reply)
        assert set(reply["data"]) == {"stream_version"} and reply["data"]["stream_version"] > before
        assert states(handler)[1] == expected_state


@pytest.mark.parametrize("op", ["secrets.revoke", "secrets.restore", "secrets.promote"])
@pytest.mark.parametrize(
    "args",
    [
        {"version": 1, "expected_version": 4},                  # path missing
        {"path": PATH, "expected_version": 4},                  # version missing
        {"path": PATH, "version": "1", "expected_version": 4},
        {"path": PATH, "version": 0, "expected_version": 4},
        {"path": PATH, "version": 1},                           # expected_version missing
        {"path": PATH, "version": 1, "expected_version": "4"},
        {"path": 5, "version": 1, "expected_version": 4},
        {"path": "nonsense", "version": 1, "expected_version": 4},
    ],
)
def test_lifecycle_validation(env, op, args):
    _, handler = env
    reply = call(handler, op, args)
    assert reply["status"] == "failed" and reply["error"]["code"] == "validation"
    assert stream(handler) == 4


@pytest.mark.parametrize("op", ["secrets.revoke", "secrets.restore", "secrets.promote"])
def test_lifecycle_not_found(env, op):
    _, handler = env
    assert call(handler, op, {"path": PATH, "version": 9, "expected_version": 4})["error"]["code"] == "not_found"
    missing = {"path": "secret://hermes/missing", "version": 1, "expected_version": 1}
    assert call(handler, op, missing)["error"]["code"] == "not_found"


@pytest.mark.parametrize(
    "op,version",
    [("secrets.restore", 1), ("secrets.restore", 2), ("secrets.promote", 2)],
)
def test_lifecycle_transition_not_allowed_is_a_conflict(env, op, version):
    _, handler = env
    reply = call(handler, op, {"path": PATH, "version": version, "expected_version": 4})
    assert reply["status"] == "failed" and reply["error"]["code"] == "conflict"
    assert reply["error"]["retryable"] is False and reply["data"] == {}
    assert stream(handler) == 4


@pytest.mark.parametrize("op", ["secrets.revoke", "secrets.restore", "secrets.promote"])
def test_lifecycle_stale_version_is_a_conflict_with_the_current_one(env, op):
    _, handler = env
    reply = call(handler, op, {"path": PATH, "version": 1, "expected_version": 3})
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"current_version": 4}


@pytest.mark.parametrize("op", ["secrets.revoke", "secrets.restore", "secrets.promote", "secrets.destroy"])
def test_lifecycle_while_sealed_conflicts_with_the_state(env, op):
    rig, handler = env
    rig.sealing.seal()
    args = {"path": PATH, "version": 1, "expected_version": 4, "proof": RIGHT}
    reply = call(handler, op, args)
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"state": "sealed"}


# -- destroy -------------------------------------------------------------------------------------------


@pytest.fixture
def revoked(env):
    rig, handler = env
    assert call(handler, "secrets.revoke", {"path": PATH, "version": 1, "expected_version": 4})["status"] == "completed"
    return rig, handler


def destroy(handler, proof=RIGHT, version=1, expected=5):
    return call(handler, "secrets.destroy",
                {"path": PATH, "version": version, "expected_version": expected, "proof": proof})


def test_destroy_with_the_passphrase(revoked):
    _, handler = revoked
    reply = destroy(handler)
    assert reply["status"] == "completed" and reply["data"] == {"stream_version": 6}
    assert states(handler)[1] == "destroyed"


def test_destroy_with_two_shares(revoked):
    rig, handler = revoked
    assert destroy(handler, {"shares": rig.shares[:2]})["status"] == "completed"


def test_destroy_with_a_wrong_proof_is_auth(revoked):
    _, handler = revoked
    reply = destroy(handler, WRONG)
    assert reply["status"] == "failed" and reply["error"]["code"] == "auth"
    assert reply["error"]["retryable"] is False and reply["data"] == {}
    assert states(handler)[1] == "revoked"
    assert "wrong wrong" not in json.dumps(reply)


def test_destroy_during_a_wait_is_rate_limited(revoked):
    rig, handler = revoked
    for _ in range(3):
        destroy(handler, WRONG)
    reply = destroy(handler, RIGHT)
    assert reply["error"]["code"] == "rate_limited" and reply["error"]["retryable"] is True
    assert reply["error"]["retry_after_s"] == 60
    rig.clock.advance(timedelta(seconds=61))
    assert destroy(handler, RIGHT)["status"] == "completed"


def test_destroy_of_a_version_that_is_not_revoked_is_a_conflict(revoked):
    _, handler = revoked
    reply = destroy(handler, version=2)
    assert reply["error"]["code"] == "conflict" and states(handler)[2] == "active"


def test_destroy_of_a_destroyed_version_is_a_conflict(revoked):
    _, handler = revoked
    destroy(handler)
    assert destroy(handler, expected=6)["error"]["code"] == "conflict"


def test_destroy_stale_version_is_a_conflict_with_the_current_one(revoked):
    _, handler = revoked
    reply = destroy(handler, expected=4)
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"current_version": 5}


def test_destroy_not_found(revoked):
    _, handler = revoked
    assert destroy(handler, version=9)["error"]["code"] == "not_found"
    reply = call(handler, "secrets.destroy", {"path": "secret://hermes/missing", "version": 1,
                                              "expected_version": 1, "proof": RIGHT})
    assert reply["error"]["code"] == "not_found"


@pytest.mark.parametrize(
    "args",
    [
        {"version": 1, "expected_version": 5, "proof": RIGHT},                  # path missing
        {"path": PATH, "expected_version": 5, "proof": RIGHT},                  # version missing
        {"path": PATH, "version": 1, "proof": RIGHT},                           # expected_version missing
        {"path": PATH, "version": 1, "expected_version": 5},                    # proof missing
        {"path": PATH, "version": 1, "expected_version": 5, "proof": "secret"},
        {"path": PATH, "version": 1, "expected_version": 5, "proof": {}},
        {"path": PATH, "version": 1, "expected_version": 5, "proof": {"passphrase": ""}},
        {"path": PATH, "version": 1, "expected_version": 5, "proof": {"shares": ["only-one"]}},
    ],
)
def test_destroy_validation_does_not_spend_an_attempt(revoked, args):
    rig, handler = revoked
    reply = call(handler, "secrets.destroy", args)
    assert reply["status"] == "failed" and reply["error"]["code"] == "validation"
    assert rig.sealing.status()["consecutive_failures"] == 0
    assert states(handler)[1] == "revoked"
