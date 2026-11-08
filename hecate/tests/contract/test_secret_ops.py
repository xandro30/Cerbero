import json

import pytest

from hecate.domain.sealing.access_log import Operation, Result
from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import SecretsRig

PATH = "secret://hermes/telegram/bot_token"
VALUE = "contract-secret-value"
ENVELOPE_KEYS = {"status", "data", "error", "meta"}


def make(unsealed: bool = True):
    rig = SecretsRig(unsealed=unsealed)
    return rig, RequestHandler(rig.sealing, rig.service)


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


def create(handler, path=PATH, value=VALUE, value_type="string", **extra):
    return call(handler, "secrets.create", {"path": path, "value": value, "value_type": value_type, **extra})


def test_create_replies_with_the_common_envelope():
    _, handler = make()
    reply = create(handler)
    assert set(reply) == ENVELOPE_KEYS
    assert reply["status"] == "completed" and reply["error"] is None
    assert reply["meta"]["request_id"] == "r"
    data = reply["data"]
    assert set(data) == {"path", "version", "fingerprint", "stream_version"}
    assert (data["path"], data["version"], data["stream_version"]) == (PATH, 1, 2)


def test_create_with_a_maximum_of_one_version_is_validation():
    _, handler = make()
    reply = create(handler, max_versions=1)
    assert reply["status"] == "failed" and reply["error"]["code"] == "validation"
    assert "from 2 to 100" in reply["error"]["message"]


def test_create_accepts_optional_metadata():
    _, handler = make()
    reply = create(handler, max_versions=4, rotate_every="30d", expires_at="2027-01-01T00:00:00+00:00")
    assert reply["status"] == "completed"
    got = call(handler, "secrets.get", {"path": PATH})["data"]
    assert (got["max_versions"], got["rotate_every"], got["expires_at"]) == (
        4, "30d", "2027-01-01T00:00:00+00:00")


def test_list_returns_the_documented_shape_and_filters():
    _, handler = make()
    create(handler)
    create(handler, path="secret://echidna/key")
    everything = call(handler, "secrets.list")["data"]["secrets"]
    assert [s["path"] for s in everything] == ["secret://echidna/key", PATH]
    assert {"path", "value_type", "active_version", "version_count", "expires_at"} <= set(everything[0])
    only = call(handler, "secrets.list", {"identity": "hermes"})["data"]["secrets"]
    assert [s["path"] for s in only] == [PATH]


def test_get_returns_metadata_stream_version_and_versions_without_the_value():
    _, handler = make()
    create(handler)
    reply = call(handler, "secrets.get", {"path": PATH})
    data = reply["data"]
    assert data["stream_version"] == 2 and data["value_type"] == "string"
    assert data["versions"][0]["state"] == "active"
    assert VALUE not in json.dumps(reply)


@pytest.mark.parametrize(
    "op,args",
    [
        ("secrets.create", {"path": "nonsense", "value": VALUE, "value_type": "string"}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "binary"}),
        ("secrets.create", {"path": PATH, "value": "{bad", "value_type": "json"}),
        ("secrets.create", {"path": PATH, "value": "x" * (64 * 1024 + 1), "value_type": "string"}),
        ("secrets.create", {"path": PATH, "value": VALUE}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "rotate_every": ""}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "rotate_every": "x" * 65}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "rotate_every": "90\nd"}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "rotate_every": 90}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "expires_at": "tomorrow"}),
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string", "expires_at": 20270101}),
        ("secrets.create", {"path": PATH, "value": 5, "value_type": "string"}),
        ("secrets.create", {}),
        ("secrets.get", {"path": 5}),
        ("secrets.get", {}),
        ("secrets.get", {"path": "nonsense"}),
        ("secrets.list", {"identity": "Bad Identity"}),
        ("secrets.list", {"identity": 7}),
    ],
)
def test_validation_errors(op, args):
    rig, handler = make()
    reply = call(handler, op, args)
    assert reply["status"] == "failed" and reply["error"]["code"] == "validation"
    assert reply["error"]["retryable"] is False and reply["data"] == {}
    assert not rig.store.events
    assert rig.secret_rows()[-1].result is Result.INVALID_INPUT


def test_malformed_arguments_are_logged_without_their_content():
    rig, handler = make()
    call(handler, "secrets.create", {"path": "oops-this-is-the-password", "value": 5})
    (row,) = rig.secret_rows()
    assert row.operation is Operation.SECRET_CREATE and row.result is Result.INVALID_INPUT
    assert "password" not in repr(row)


def test_conflict_when_the_path_already_exists():
    _, handler = make()
    create(handler)
    reply = create(handler, value="another")
    assert reply["status"] == "failed" and reply["error"]["code"] == "conflict"
    assert reply["error"]["retryable"] is False


@pytest.mark.parametrize(
    "op,args",
    [
        ("secrets.create", {"path": PATH, "value": VALUE, "value_type": "string"}),
        ("secrets.list", {}),
        ("secrets.get", {"path": PATH}),
    ],
)
def test_conflict_with_the_state_while_sealed(op, args):
    _, handler = make(unsealed=False)
    reply = call(handler, op, args)
    assert reply["status"] == "failed" and reply["error"]["code"] == "conflict"
    assert reply["data"] == {"state": "sealed"}


def test_status_still_works_while_sealed():
    _, handler = make(unsealed=False)
    assert call(handler, "status")["status"] == "completed"


def test_get_of_a_missing_path_is_not_found():
    _, handler = make()
    reply = call(handler, "secrets.get", {"path": "secret://hermes/missing"})
    assert reply["status"] == "failed" and reply["error"]["code"] == "not_found"
    assert reply["error"]["retryable"] is False and reply["data"] == {}


def test_secret_operations_do_not_exist_without_a_secrets_service():
    rig = SecretsRig()
    handler = RequestHandler(rig.sealing)
    reply = call(handler, "secrets.list")
    assert reply["error"]["code"] == "validation"


# -- secrets.add_version ---------------------------------------------------------------------------


def get(handler, path=PATH):
    return call(handler, "secrets.get", {"path": path})["data"]


def test_add_version_replies_with_the_new_version_and_its_fingerprint():
    _, handler = make()
    create(handler)
    reply = call(handler, "secrets.add_version", {"path": PATH, "value": "next-value", "expected_version": 2})
    assert reply["status"] == "completed"
    assert set(reply["data"]) == {"version", "fingerprint", "stream_version"}
    assert (reply["data"]["version"], reply["data"]["stream_version"]) == (2, 4)
    assert "next-value" not in json.dumps(reply)
    assert [v["state"] for v in get(handler)["versions"]] == ["deprecated", "active"]


@pytest.mark.parametrize(
    "args",
    [
        {"path": PATH, "value": "x"},                                     # expected_version missing
        {"path": PATH, "value": "x", "expected_version": "2"},
        {"path": PATH, "value": "x", "expected_version": None},
        {"path": PATH, "expected_version": 2},                            # value missing
        {"path": PATH, "value": 5, "expected_version": 2},
        {"value": "x", "expected_version": 2},                            # path missing
        {"path": "nonsense", "value": "x", "expected_version": 2},
    ],
)
def test_add_version_validation(args):
    _, handler = make()
    create(handler)
    reply = call(handler, "secrets.add_version", args)
    assert reply["status"] == "failed" and reply["error"]["code"] == "validation"
    assert get(handler)["stream_version"] == 2


def test_add_version_with_a_value_invalid_for_the_type_is_validation():
    _, handler = make()
    create(handler, value="{}", value_type="json")
    reply = call(handler, "secrets.add_version", {"path": PATH, "value": "{bad", "expected_version": 2})
    assert reply["error"]["code"] == "validation"


def test_add_version_conflict_carries_the_current_version():
    _, handler = make()
    create(handler)
    call(handler, "secrets.add_version", {"path": PATH, "value": "v2", "expected_version": 2})
    reply = call(handler, "secrets.add_version", {"path": PATH, "value": "v3", "expected_version": 2})
    assert reply["status"] == "failed" and reply["error"]["code"] == "conflict"
    assert reply["error"]["retryable"] is False
    assert reply["data"] == {"current_version": 4}


def test_add_version_of_a_missing_path_is_not_found():
    _, handler = make()
    reply = call(handler, "secrets.add_version",
                 {"path": "secret://hermes/missing", "value": "x", "expected_version": 1})
    assert reply["error"]["code"] == "not_found"


def test_add_version_while_sealed_conflicts_with_the_state():
    _, handler = make(unsealed=False)
    reply = call(handler, "secrets.add_version", {"path": PATH, "value": "x", "expected_version": 2})
    assert reply["error"]["code"] == "conflict" and reply["data"] == {"state": "sealed"}
