import json
from datetime import timedelta

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from tests.fakes import SecretsRig

PATH = "secret://hermes/telegram/bot_token"
V1, V2, V3 = "first-value-111", "second-value-222", "third-value-333"


@pytest.fixture
def rig():
    rig = SecretsRig()
    rig.service.create(PATH, V1, "string")
    rig.access_log.items.clear()
    return rig


def current(rig, path=PATH) -> int:
    """The number of events of the secret, read without leaving a row in the access log."""
    return len(rig.store.events[f"secret/{path.removeprefix('secret://')}"])


def resolve(rig, ref):
    return rig.service.resolve("hermes", [ref])["secrets"][0]


def test_add_version_makes_the_new_one_active_and_the_old_one_deprecated(rig):
    out = rig.service.add_version(PATH, V2, current(rig))
    assert out["version"] == 2 and len(out["fingerprint"]) == 19
    assert out["stream_version"] == current(rig) == 4  # created + added (2), then deprecated + added (2)
    assert set(out) == {"version", "fingerprint", "stream_version"}
    states = {v["version"]: v["state"] for v in rig.service.get(PATH)["versions"]}
    assert states == {1: "deprecated", 2: "active"}
    assert resolve(rig, PATH)["value"] == V2 and resolve(rig, PATH)["version"] == 2


def test_the_new_version_has_its_own_fingerprint_and_the_same_scope_key(rig):
    first = rig.service.get(PATH)["versions"][0]["fingerprint"]
    out = rig.service.add_version(PATH, V2, current(rig))
    assert out["fingerprint"] != first
    assert len(rig.store.key_material) == 1
    assert [r.type for r in rig.store.events["key/hermes"]] == ["ScopeKeyCreated"]


def test_the_previous_version_resolves_explicitly_during_the_grace_and_not_after(rig):
    rig.service.add_version(PATH, V2, current(rig))
    assert resolve(rig, PATH + "@1")["value"] == V1
    rig.clock.advance(timedelta(hours=24) - timedelta(seconds=1))
    assert resolve(rig, PATH + "@1")["value"] == V1
    rig.clock.advance(timedelta(seconds=1))
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", [PATH + "@1"])
    assert resolve(rig, PATH)["value"] == V2
    assert {v["version"]: v["state"] for v in rig.service.get(PATH)["versions"]} == {1: "revoked", 2: "active"}


def test_a_stale_expected_version_conflicts_with_the_current_one_and_changes_nothing(rig):
    stale = current(rig)
    rig.service.add_version(PATH, V2, stale)
    now = current(rig)
    with pytest.raises(StateConflict) as exc:
        rig.service.add_version(PATH, V3, stale)
    assert exc.value.current_version == now and exc.value.state is None
    assert current(rig) == now and len(rig.store.secret_material) == 2
    assert resolve(rig, PATH)["value"] == V2


@pytest.mark.parametrize("expected", [None, "2", 2.0, True, [2]])
def test_a_missing_or_malformed_expected_version_is_invalid(rig, expected):
    before = current(rig)
    with pytest.raises(InvalidInput):
        rig.service.add_version(PATH, V2, expected)
    assert current(rig) == before and len(rig.store.secret_material) == 1


def test_an_unknown_path_is_not_found(rig):
    with pytest.raises(NotFound):
        rig.service.add_version("secret://hermes/missing", V2, 2)


def test_a_malformed_path_is_invalid(rig):
    with pytest.raises(InvalidInput):
        rig.service.add_version("nonsense", V2, 2)


def test_the_value_must_be_valid_for_the_type_of_the_secret(rig):
    rig.service.create("secret://hermes/doc", '{"a": 1}', "json")
    before = current(rig, "secret://hermes/doc")
    with pytest.raises(InvalidInput):
        rig.service.add_version("secret://hermes/doc", "{broken", before)
    with pytest.raises(InvalidInput):
        rig.service.add_version(PATH, "", current(rig))
    assert current(rig, "secret://hermes/doc") == before
    rig.service.add_version("secret://hermes/doc", '{"a": 2}', before)


def test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change(rig):
    path = "secret://hermes/limited"
    rig.service.create(path, "v1-limited", "string", max_versions=2)
    rig.service.add_version(path, "v2-limited", current(rig, path))
    assert rig.store.load_secret_material(path, 1) is not None
    rig.service.add_version(path, "v3-limited", current(rig, path))
    assert rig.store.load_secret_material(path, 1) is None, "the destroyed version keeps no material"
    assert rig.store.load_secret_material(path, 3) is not None
    states = {v["version"]: v["state"] for v in rig.service.get(path)["versions"]}
    assert states == {1: "destroyed", 2: "deprecated", 3: "active"}
    destroyed = [r for r in rig.store.events[f"secret/hermes/limited"] if r.type == "VersionDestroyed"]
    assert [(r.data["version"], r.data["reason"]) for r in destroyed] == [(1, "max_versions")]
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", [path + "@1"])
    assert next(s for s in rig.service.list_secrets()["secrets"] if s["path"] == path)["version_count"] == 2


def test_list_reports_the_active_version_and_how_many_are_kept(rig):
    for value in (V2, V3):
        rig.service.add_version(PATH, value, current(rig))
    item = rig.service.list_secrets()["secrets"][0]
    assert (item["active_version"], item["version_count"]) == (3, 3)


def test_the_access_log_records_the_admin_the_path_and_the_new_version(rig):
    rig.service.add_version(PATH, V2, current(rig))
    (row,) = rig.secret_rows()
    assert (row.operation, row.subject, row.method, row.result) == (
        Operation.SECRET_ADD_VERSION, "admin", Method.LOCAL, Result.SUCCESS)
    assert (row.path, row.version) == (PATH, 2)
    assert V2 not in repr(row)


def test_refusals_are_logged(rig):
    with pytest.raises(StateConflict):
        rig.service.add_version(PATH, V2, 1)
    with pytest.raises(NotFound):
        rig.service.add_version("secret://hermes/missing", V2, 1)
    with pytest.raises(InvalidInput):
        rig.service.add_version(PATH, V2, None)
    assert [r.result for r in rig.secret_rows()] == [
        Result.REJECTED_CONFLICT, Result.NOT_FOUND, Result.INVALID_INPUT]


def test_sealed_hecate_refuses(rig):
    rig.sealing.seal()
    with pytest.raises(StateConflict) as exc:
        rig.service.add_version(PATH, V2, 2)
    assert exc.value.state.value == "sealed"


def test_a_failure_while_saving_leaves_the_secret_as_it_was(rig):
    before = current(rig)

    def boom(*_a, **_k):
        raise RuntimeError("disk full")

    rig.store.save_secret_material = boom
    with pytest.raises(RuntimeError):
        rig.service.add_version(PATH, V2, before)
    del rig.store.save_secret_material
    assert current(rig) == before and len(rig.store.secret_material) == 1
    assert resolve(rig, PATH)["value"] == V1


def test_a_failure_while_destroying_the_oldest_leaves_everything_as_it_was(rig):
    path = "secret://hermes/limited"
    rig.service.create(path, "v1-limited", "string", max_versions=2)
    rig.service.add_version(path, "v2-limited", current(rig, path))
    before = current(rig, path)

    def boom(*_a, **_k):
        raise RuntimeError("disk full")

    rig.store.delete_secret_material = boom
    with pytest.raises(RuntimeError):
        rig.service.add_version(path, "v3-limited", before)
    del rig.store.delete_secret_material
    assert current(rig, path) == before
    assert resolve_path(rig, path)["value"] == "v2-limited"
    assert rig.store.load_secret_material(path, 1) is not None


def resolve_path(rig, path):
    return rig.service.resolve("hermes", [path])["secrets"][0]


def test_no_value_is_stored_in_clear(rig):
    rig.service.add_version(PATH, V2, current(rig))
    dump = repr((rig.store.events, rig.store.secret_material, rig.store.key_material)).encode()
    assert V1.encode() not in dump and V2.encode() not in dump
    assert V2 not in json.dumps(rig.service.get(PATH))
