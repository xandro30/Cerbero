from datetime import timedelta

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import AuthenticationFailed, InvalidInput, NotFound, RateLimited, StateConflict
from tests.fakes import GOOD_PASSPHRASE, SecretsRig

PATH = "secret://hermes/telegram/bot_token"
V1, V2 = "first-value-111", "second-value-222"
WRONG = {"passphrase": "wrong wrong wrong"}
RIGHT = {"passphrase": GOOD_PASSPHRASE}


@pytest.fixture
def rig():
    """Version 1 revoked, version 2 active."""
    rig = SecretsRig()
    rig.service.create(PATH, V1, "string")
    rig.service.add_version(PATH, V2, current(rig))
    rig.service.revoke(PATH, 1, current(rig))
    rig.access_log.items.clear()
    return rig


def current(rig) -> int:
    """The number of events of the secret, read without leaving a row in the access log."""
    return len(rig.store.events[f"secret/{PATH.removeprefix('secret://')}"])


def states(rig) -> dict[int, str]:
    return {v["version"]: v["state"] for v in rig.service.get(PATH)["versions"]}


def test_destroying_with_the_passphrase_removes_the_material_and_keeps_the_events(rig):
    assert rig.store.load_secret_material(PATH, 1) is not None
    before = current(rig)
    out = rig.service.destroy(PATH, 1, before, RIGHT)
    assert out == {"stream_version": before + 1}
    assert states(rig) == {1: "destroyed", 2: "active"}
    assert rig.store.load_secret_material(PATH, 1) is None
    assert rig.store.load_secret_material(PATH, 2) is not None
    events = rig.store.events["secret/hermes/telegram/bot_token"]
    assert [r.type for r in events][-1] == "VersionDestroyed"
    assert events[-1].data == {"version": 1, "reason": "admin"}
    assert any(r.type == "VersionAdded" and r.data["version"] == 1 for r in events), "history stays"


def test_destroying_with_two_recovery_shares(rig):
    rig.service.destroy(PATH, 1, current(rig), {"shares": rig.shares[1:]})
    assert states(rig)[1] == "destroyed"


def test_a_destroyed_version_is_not_resolved_nor_restored(rig):
    rig.service.destroy(PATH, 1, current(rig), RIGHT)
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", [PATH + "@1"])
    with pytest.raises(StateConflict):
        rig.service.restore(PATH, 1, current(rig))
    with pytest.raises(StateConflict):
        rig.service.destroy(PATH, 1, current(rig), RIGHT)
    assert rig.service.resolve("hermes", [PATH])["secrets"][0]["value"] == V2


def test_a_wrong_proof_is_refused_and_counts_in_the_shared_counter(rig):
    before = current(rig)
    with pytest.raises(AuthenticationFailed):
        rig.service.destroy(PATH, 1, before, WRONG)
    assert states(rig)[1] == "revoked" and current(rig) == before
    assert rig.store.load_secret_material(PATH, 1) is not None
    assert rig.sealing.status()["consecutive_failures"] == 1


def test_wrong_shares_are_refused_too(rig):
    other = SecretsRig().shares
    with pytest.raises(AuthenticationFailed):
        rig.service.destroy(PATH, 1, current(rig), {"shares": other[:2]})
    assert states(rig)[1] == "revoked"


def test_during_a_wait_even_the_right_proof_is_rate_limited(rig):
    for _ in range(3):
        with pytest.raises(AuthenticationFailed):
            rig.service.destroy(PATH, 1, current(rig), WRONG)
    with pytest.raises(RateLimited) as exc:
        rig.service.destroy(PATH, 1, current(rig), RIGHT)
    assert exc.value.retry_after_s == 60
    assert states(rig)[1] == "revoked"
    rig.clock.advance(timedelta(seconds=61))
    rig.service.destroy(PATH, 1, current(rig), RIGHT)
    assert states(rig)[1] == "destroyed"


def test_the_counter_is_shared_with_unsealing(rig):
    for _ in range(2):
        with pytest.raises(AuthenticationFailed):
            rig.service.destroy(PATH, 1, current(rig), WRONG)
    rig.sealing.seal()
    with pytest.raises(AuthenticationFailed):
        rig.sealing.unseal_with_passphrase("wrong wrong wrong")
    assert rig.sealing.status()["consecutive_failures"] == 3


@pytest.mark.parametrize("version,state", [(2, "active")])
def test_only_a_revoked_version_can_be_destroyed(rig, version, state):
    with pytest.raises(StateConflict):
        rig.service.destroy(PATH, version, current(rig), WRONG)
    assert states(rig)[version] == state
    assert rig.sealing.status()["consecutive_failures"] == 0, "the state is checked before the proof"


def test_a_deprecated_version_is_not_destroyable_until_its_grace_ends(rig):
    rig.service.restore(PATH, 1, current(rig))
    with pytest.raises(StateConflict):
        rig.service.destroy(PATH, 1, current(rig), RIGHT)
    rig.clock.advance(timedelta(hours=24))
    rig.service.destroy(PATH, 1, current(rig), RIGHT)
    assert states(rig)[1] == "destroyed"


@pytest.mark.parametrize("proof", [None, {}, "text", {"passphrase": ""}, {"passphrase": "x", "shares": []},
                                   {"shares": "AB"}, {"shares": []}])
def test_a_malformed_proof_is_invalid_without_counting(rig, proof):
    with pytest.raises(InvalidInput):
        rig.service.destroy(PATH, 1, current(rig), proof)
    assert rig.sealing.status()["consecutive_failures"] == 0


def test_a_stale_expected_version_is_refused_before_the_proof_is_checked(rig):
    now = current(rig)
    with pytest.raises(StateConflict) as exc:
        rig.service.destroy(PATH, 1, now - 1, WRONG)
    assert exc.value.current_version == now
    with pytest.raises(InvalidInput):
        rig.service.destroy(PATH, 1, None, RIGHT)
    assert rig.sealing.status()["consecutive_failures"] == 0


def test_unknown_version_or_path_is_not_found(rig):
    with pytest.raises(NotFound):
        rig.service.destroy(PATH, 9, current(rig), RIGHT)
    with pytest.raises(NotFound):
        rig.service.destroy("secret://hermes/missing", 1, 1, RIGHT)


def test_sealed_hecate_refuses(rig):
    rig.sealing.seal()
    with pytest.raises(StateConflict) as exc:
        rig.service.destroy(PATH, 1, 6, RIGHT)
    assert exc.value.state.value == "sealed"


def test_the_log_records_the_method_of_the_proof_and_the_result(rig):
    with pytest.raises(AuthenticationFailed):
        rig.service.destroy(PATH, 1, current(rig), WRONG)
    rig.service.destroy(PATH, 1, current(rig), {"shares": rig.shares[:2]})
    failed, done = rig.secret_rows()
    assert (failed.operation, failed.method, failed.result, failed.version) == (
        Operation.SECRET_DESTROY, Method.PASSPHRASE, Result.FAILURE, 1)
    assert (done.operation, done.method, done.result, done.subject, done.path) == (
        Operation.SECRET_DESTROY, Method.RECOVERY, Result.SUCCESS, "admin", PATH)
    assert GOOD_PASSPHRASE not in repr(rig.access_log.items) and "wrong wrong" not in repr(rig.access_log.items)


def test_a_failure_while_destroying_leaves_the_version_revoked_with_its_material(rig):
    before = current(rig)

    def boom(*_a, **_k):
        raise RuntimeError("disk full")

    rig.store.delete_secret_material = boom
    with pytest.raises(RuntimeError):
        rig.service.destroy(PATH, 1, before, RIGHT)
    del rig.store.delete_secret_material
    assert states(rig)[1] == "revoked" and current(rig) == before
    assert rig.store.load_secret_material(PATH, 1) is not None
