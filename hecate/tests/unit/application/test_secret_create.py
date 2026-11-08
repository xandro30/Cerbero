import re

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import InvalidInput, StateConflict
from hecate.domain.sealing.state import SealState
from tests.fakes import SecretsRig

PATH = "secret://hermes/telegram/family_bot/bot_token"
VALUE = "7312:AAF-super-secret-bot-token"
FINGERPRINT = re.compile(r"^[0-9a-f]{4}(-[0-9a-f]{4}){3}$")


def store_is_empty(rig: SecretsRig) -> bool:
    return not rig.store.events and not rig.store.secret_material and not rig.store.key_material


def test_create_gives_version_one_active_with_a_fingerprint():
    rig = SecretsRig()
    out = rig.service.create(PATH, VALUE, "string")
    assert out["path"] == PATH and out["version"] == 1
    assert FINGERPRINT.match(out["fingerprint"])
    assert out["stream_version"] == 2
    assert set(out) == {"path", "version", "fingerprint", "stream_version"}
    assert rig.service.get(PATH)["versions"][0]["state"] == "active"


def test_the_first_secret_of_an_identity_creates_its_key_only_once():
    rig = SecretsRig()
    rig.service.create(PATH, VALUE, "string")
    rig.service.create("secret://hermes/other", VALUE, "string")
    assert [r.type for r in rig.store.events["key/hermes"]] == ["ScopeKeyCreated"]
    assert ("hermes", 1) in rig.store.key_material and len(rig.store.key_material) == 1

    rig.service.create("secret://echidna/key", VALUE, "string")
    assert [r.type for r in rig.store.events["key/echidna"]] == ["ScopeKeyCreated"]
    assert len(rig.store.key_material) == 2


def test_the_same_value_has_different_fingerprints_in_different_identities():
    rig = SecretsRig()
    first = rig.service.create("secret://hermes/a", VALUE, "string")["fingerprint"]
    second = rig.service.create("secret://echidna/a", VALUE, "string")["fingerprint"]
    same_identity = rig.service.create("secret://hermes/b", VALUE, "string")["fingerprint"]
    assert first != second and first == same_identity


def test_metadata_given_at_creation_is_kept():
    rig = SecretsRig()
    rig.service.create(
        PATH, VALUE, "string", max_versions=3, rotate_every="90d", expires_at="2027-01-01T00:00:00+00:00"
    )
    got = rig.service.get(PATH)
    assert (got["max_versions"], got["rotate_every"]) == (3, "90d")
    assert got["expires_at"] == "2027-01-01T00:00:00+00:00"


@pytest.mark.parametrize(
    "kwargs",
    [
        {"path": "hermes/token"},
        {"path": "secret://Hermes/token"},
        {"value_type": "binary"},
        {"value_type": "json", "value": "{nope"},
        {"value_type": "pem", "value": "not a pem"},
        {"value": ""},
        {"value": "x" * (64 * 1024 + 1)},
        {"value": 123},
        {"max_versions": 0},
        {"max_versions": 1},
        {"max_versions": 1000},
        {"max_versions": "ten"},
        {"rotate_every": ""},
        {"rotate_every": "x" * 100},
        {"rotate_every": 5},
        {"expires_at": "tomorrow"},
        {"expires_at": 12345},
    ],
)
def test_invalid_input_changes_nothing(kwargs):
    rig = SecretsRig()
    args = {"path": PATH, "value": VALUE, "value_type": "string"} | kwargs
    with pytest.raises(InvalidInput):
        rig.service.create(**args)
    assert store_is_empty(rig)


def test_a_value_of_exactly_64_kib_is_accepted():
    rig = SecretsRig()
    assert rig.service.create(PATH, "x" * (64 * 1024), "string")["version"] == 1


def test_creating_over_an_existing_path_conflicts_and_keeps_the_original():
    rig = SecretsRig()
    rig.service.create(PATH, VALUE, "string")
    before = len(rig.store.events[f"secret/{PATH.removeprefix('secret://')}"])
    with pytest.raises(StateConflict) as exc:
        rig.service.create(PATH, "another value", "string")
    assert exc.value.state is None
    assert len(rig.store.events[f"secret/{PATH.removeprefix('secret://')}"]) == before
    assert len(rig.store.secret_material) == 1


@pytest.mark.parametrize("unsealed", [False])
def test_creating_while_sealed_conflicts(unsealed):
    rig = SecretsRig(unsealed=unsealed)
    with pytest.raises(StateConflict) as exc:
        rig.service.create(PATH, VALUE, "string")
    assert exc.value.state is SealState.SEALED
    assert store_is_empty(rig)


def test_a_failure_while_saving_leaves_no_trace():
    rig = SecretsRig()

    def boom(*_a, **_k):
        raise RuntimeError("disk full")

    rig.store.save_secret_material = boom
    with pytest.raises(RuntimeError):
        rig.service.create(PATH, VALUE, "string")
    assert store_is_empty(rig)
    del rig.store.save_secret_material
    assert rig.service.create(PATH, VALUE, "string")["version"] == 1


def test_the_access_log_records_who_where_and_which_version_but_no_value():
    rig = SecretsRig()
    out = rig.service.create(PATH, VALUE, "string")
    (row,) = rig.secret_rows()
    assert (row.operation, row.subject, row.method, row.result) == (
        Operation.SECRET_CREATE, "admin", Method.LOCAL, Result.SUCCESS)
    assert (row.path, row.version) == (PATH, 1)
    assert VALUE not in repr(row) and out["fingerprint"] not in repr(row)


def test_refusals_are_logged_too():
    rig = SecretsRig()
    rig.service.create(PATH, VALUE, "string")
    for kwargs, result in [
        ({"value_type": "binary"}, Result.INVALID_INPUT),
        ({}, Result.REJECTED_CONFLICT),
    ]:
        with pytest.raises((InvalidInput, StateConflict)):
            rig.service.create(**({"path": PATH, "value": VALUE, "value_type": "string"} | kwargs))
        assert rig.secret_rows()[-1].result is result
    sealed = SecretsRig(unsealed=False)
    with pytest.raises(StateConflict):
        sealed.service.create(PATH, VALUE, "string")
    assert sealed.secret_rows()[-1].result is Result.REJECTED_CONFLICT


def test_a_malformed_path_is_never_written_to_the_log():
    rig = SecretsRig()
    with pytest.raises(InvalidInput):
        rig.service.create("my-actual-password-by-mistake", VALUE, "string")
    (row,) = rig.secret_rows()
    assert row.path is None and "password" not in repr(row)


def test_the_value_appears_nowhere_in_the_store():
    rig = SecretsRig()
    rig.service.create(PATH, VALUE, "string")
    dump = repr((rig.store.events, rig.store.secret_material, rig.store.key_material)).encode()
    assert VALUE.encode() not in dump
    assert VALUE not in repr(rig.access_log.items)
