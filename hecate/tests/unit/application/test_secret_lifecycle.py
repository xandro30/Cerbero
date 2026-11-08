from datetime import timedelta

import pytest

from hecate.domain.sealing.access_log import Operation, Result
from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from tests.fakes import SecretsRig

PATH = "secret://hermes/telegram/bot_token"
V1, V2 = "first-value-111", "second-value-222"


@pytest.fixture
def rig():
    """Version 1 deprecated, version 2 active."""
    rig = SecretsRig()
    rig.service.create(PATH, V1, "string")
    rig.service.add_version(PATH, V2, current(rig))
    rig.access_log.items.clear()
    return rig


def current(rig) -> int:
    """The number of events of the secret, read without leaving a row in the access log."""
    return len(rig.store.events[f"secret/{PATH.removeprefix('secret://')}"])


def states(rig) -> dict[int, str]:
    return {v["version"]: v["state"] for v in rig.service.get(PATH)["versions"]}


def can_resolve(rig, ref) -> bool:
    try:
        rig.service.resolve("hermes", [ref])
    except NotFound:
        return False
    return True


def test_revoking_the_active_version_stops_it_at_once_and_leaves_none_active(rig):
    out = rig.service.revoke(PATH, 2, current(rig))
    assert out == {"stream_version": current(rig)}
    assert states(rig) == {1: "deprecated", 2: "revoked"}
    assert not can_resolve(rig, PATH + "@2")
    assert not can_resolve(rig, PATH), "without an active version the resolution is refused"
    assert can_resolve(rig, PATH + "@1")


def test_revoking_a_deprecated_version_stops_it_before_its_grace_ends(rig):
    assert can_resolve(rig, PATH + "@1")
    rig.service.revoke(PATH, 1, current(rig))
    assert states(rig)[1] == "revoked"
    assert not can_resolve(rig, PATH + "@1")
    assert can_resolve(rig, PATH)


def test_restoring_a_revoked_version_leaves_it_deprecated_with_a_new_grace(rig):
    rig.service.revoke(PATH, 1, current(rig))
    rig.clock.advance(timedelta(hours=30))
    rig.service.restore(PATH, 1, current(rig))
    assert states(rig)[1] == "deprecated"
    assert can_resolve(rig, PATH + "@1")
    rig.clock.advance(timedelta(hours=23))
    assert can_resolve(rig, PATH + "@1")
    rig.clock.advance(timedelta(hours=1))
    assert not can_resolve(rig, PATH + "@1")
    assert states(rig)[1] == "revoked"


def test_an_expired_deprecated_version_can_be_restored(rig):
    rig.clock.advance(timedelta(hours=25))
    assert states(rig)[1] == "revoked"
    rig.service.restore(PATH, 1, current(rig))
    assert states(rig)[1] == "deprecated" and can_resolve(rig, PATH + "@1")


def test_promoting_a_deprecated_version_makes_it_active_and_demotes_the_active_one(rig):
    rig.service.promote(PATH, 1, current(rig))
    assert states(rig) == {1: "active", 2: "deprecated"}
    assert rig.service.resolve("hermes", [PATH])["secrets"][0]["value"] == V1
    assert can_resolve(rig, PATH + "@2")


def test_promoting_when_nothing_is_active_only_promotes(rig):
    rig.service.revoke(PATH, 2, current(rig))
    rig.service.promote(PATH, 1, current(rig))
    assert states(rig) == {1: "active", 2: "revoked"}


def test_a_new_version_after_revoking_the_active_one_becomes_active(rig):
    rig.service.revoke(PATH, 2, current(rig))
    rig.service.add_version(PATH, "third-value-333", current(rig))
    assert states(rig) == {1: "deprecated", 2: "revoked", 3: "active"}
    assert rig.service.resolve("hermes", [PATH])["secrets"][0]["version"] == 3


@pytest.mark.parametrize(
    "action,version",
    [("revoke", None), ("restore", 2), ("restore", 1), ("promote", 2)],
)
def test_transitions_that_are_not_allowed_conflict_and_change_nothing(rig, action, version):
    if action == "revoke":
        rig.service.revoke(PATH, 2, current(rig))
        version = 2
    before = current(rig)
    with pytest.raises(StateConflict) as exc:
        getattr(rig.service, action)(PATH, version, before)
    assert exc.value.state is None and "version" in str(exc.value)
    assert current(rig) == before


def test_a_revoked_version_cannot_be_promoted(rig):
    rig.service.revoke(PATH, 1, current(rig))
    with pytest.raises(StateConflict):
        rig.service.promote(PATH, 1, current(rig))


@pytest.mark.parametrize("action", ["revoke", "restore", "promote"])
def test_an_unknown_version_or_path_is_not_found(rig, action):
    with pytest.raises(NotFound):
        getattr(rig.service, action)(PATH, 9, current(rig))
    with pytest.raises(NotFound):
        getattr(rig.service, action)("secret://hermes/missing", 1, 1)


@pytest.mark.parametrize("action", ["revoke", "restore", "promote"])
def test_a_stale_or_missing_expected_version_is_refused(rig, action):
    now = current(rig)
    with pytest.raises(StateConflict) as exc:
        getattr(rig.service, action)(PATH, 1, now - 1)
    assert exc.value.current_version == now
    with pytest.raises(InvalidInput):
        getattr(rig.service, action)(PATH, 1, None)
    assert current(rig) == now


@pytest.mark.parametrize("version", [None, "1", 0, -1, True, 1.5])
def test_a_malformed_version_is_invalid(rig, version):
    with pytest.raises(InvalidInput):
        rig.service.revoke(PATH, version, current(rig))


def test_none_of_these_operations_asks_for_a_proof_or_touches_the_attempt_counter(rig):
    rig.service.revoke(PATH, 1, current(rig))
    rig.service.restore(PATH, 1, current(rig))
    rig.service.promote(PATH, 1, current(rig))
    assert rig.sealing.status()["consecutive_failures"] == 0


def test_each_operation_is_logged_with_its_version(rig):
    rig.service.revoke(PATH, 1, current(rig))
    rig.service.restore(PATH, 1, current(rig))
    rig.service.promote(PATH, 1, current(rig))
    with pytest.raises(StateConflict):
        rig.service.restore(PATH, 1, current(rig))
    rows = rig.secret_rows()
    assert [(r.operation, r.subject, r.path, r.version, r.result) for r in rows] == [
        (Operation.SECRET_REVOKE, "admin", PATH, 1, Result.SUCCESS),
        (Operation.SECRET_RESTORE, "admin", PATH, 1, Result.SUCCESS),
        (Operation.SECRET_PROMOTE, "admin", PATH, 1, Result.SUCCESS),
        (Operation.SECRET_RESTORE, "admin", PATH, 1, Result.REJECTED_CONFLICT),
    ]


def test_sealed_hecate_refuses_all_three(rig):
    rig.sealing.seal()
    for action in ("revoke", "restore", "promote"):
        with pytest.raises(StateConflict) as exc:
            getattr(rig.service, action)(PATH, 1, 4)
        assert exc.value.state.value == "sealed"
