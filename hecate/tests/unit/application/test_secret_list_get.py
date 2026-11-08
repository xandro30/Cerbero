import json

import pytest

from hecate.domain.sealing.access_log import Operation, Result
from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from tests.fakes import SecretsRig

TOKEN = "secret://hermes/telegram/bot_token"
PROXY = "secret://hermes/certs/proxy"
ECHIDNA = "secret://echidna/api_key"
VALUE = "the-value-that-must-never-be-shown"


@pytest.fixture
def rig():
    rig = SecretsRig()
    rig.service.create(TOKEN, VALUE, "string", rotate_every="90d", expires_at="2027-06-01T00:00:00+00:00")
    rig.service.create(PROXY, '{"k": "v"}', "json")
    rig.service.create(ECHIDNA, VALUE, "string")
    return rig


def test_list_returns_paths_and_metadata_sorted(rig):
    secrets = rig.service.list_secrets()["secrets"]
    assert [s["path"] for s in secrets] == [ECHIDNA, PROXY, TOKEN]
    token = secrets[2]
    assert token == {
        "path": TOKEN, "value_type": "string", "active_version": 1, "version_count": 1,
        "rotate_every": "90d", "expires_at": "2027-06-01T00:00:00+00:00",
    }
    assert secrets[1]["rotate_every"] is None and secrets[1]["expires_at"] is None


def test_list_filters_by_identity(rig):
    assert [s["path"] for s in rig.service.list_secrets("hermes")["secrets"]] == [PROXY, TOKEN]
    assert [s["path"] for s in rig.service.list_secrets("echidna")["secrets"]] == [ECHIDNA]
    assert rig.service.list_secrets("nobody")["secrets"] == []


def test_list_does_not_treat_a_prefix_of_an_identity_as_the_identity(rig):
    rig.service.create("secret://hermes-eu/x", VALUE, "string")
    assert [s["path"] for s in rig.service.list_secrets("hermes")["secrets"]] == [PROXY, TOKEN]


@pytest.mark.parametrize("identity", ["Hermes", "1abc", "he/rmes", "", 5])
def test_list_rejects_a_malformed_identity(rig, identity):
    with pytest.raises(InvalidInput):
        rig.service.list_secrets(identity)


def test_get_returns_metadata_stream_version_and_versions(rig):
    got = rig.service.get(TOKEN)
    assert got["path"] == TOKEN and got["owner"] == "hermes" and got["value_type"] == "string"
    assert (got["max_versions"], got["rotate_every"]) == (10, "90d")
    assert got["expires_at"] == "2027-06-01T00:00:00+00:00"
    assert got["stream_version"] == 2
    (version,) = got["versions"]
    assert set(version) == {"version", "state", "created_at", "deprecated_at", "fingerprint"}
    assert (version["version"], version["state"], version["deprecated_at"]) == (1, "active", None)
    assert version["created_at"] == rig.clock.now().isoformat()
    assert len(version["fingerprint"]) == 19


def test_no_response_contains_the_value(rig):
    dumped = json.dumps([rig.service.list_secrets(), rig.service.get(TOKEN), rig.service.get(PROXY)])
    assert VALUE not in dumped
    assert '{"k": "v"}' not in dumped and '"k"' not in dumped.replace('"key', "")


def test_unknown_path_is_not_found(rig):
    with pytest.raises(NotFound):
        rig.service.get("secret://hermes/missing")


def test_a_malformed_path_is_invalid(rig):
    with pytest.raises(InvalidInput):
        rig.service.get("nonsense")


def test_sealed_hecate_refuses_both(rig):
    rig.sealing.seal()
    with pytest.raises(StateConflict):
        rig.service.list_secrets()
    with pytest.raises(StateConflict):
        rig.service.get(TOKEN)


def test_every_consultation_is_logged_without_the_value(rig):
    rig.access_log.items.clear()
    rig.service.list_secrets("hermes")
    rig.service.get(TOKEN)
    with pytest.raises(NotFound):
        rig.service.get("secret://hermes/missing")
    listed, got, missing = rig.secret_rows()
    assert (listed.operation, listed.subject, listed.result) == (Operation.SECRET_LIST, "admin", Result.SUCCESS)
    assert (got.operation, got.path, got.result) == (Operation.SECRET_GET, TOKEN, Result.SUCCESS)
    assert (missing.operation, missing.path, missing.result) == (
        Operation.SECRET_GET, "secret://hermes/missing", Result.NOT_FOUND)
    assert VALUE not in repr(rig.access_log.items)
