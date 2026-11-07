from datetime import timedelta

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import AuthenticationFailed, InvalidInput, StateConflict
from hecate.domain.sealing.share import RecoveryShare
from tests.fakes import (
    GOOD_PASSPHRASE,
    FakeClock,
    InMemoryAccessLog,
    InMemorySealRepository,
    initialize,
    make_service,
)

PROOF = {"passphrase": GOOD_PASSPHRASE}


@pytest.fixture
def env():
    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    service = make_service(repo=repo, access_log=log, clock=clock)
    old_shares = initialize(service)
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    return repo, log, clock, service, old_shares


def restart(repo, log, clock):
    return make_service(repo=repo, access_log=log, clock=clock)


def test_rekey_replaces_the_shares_and_leaves_everything_else(env):
    repo, log, clock, service, old = env
    before = repo.config
    begun = service.rekey_begin(PROOF)
    new = begun["shares"]
    assert len(new) == 3 and set(new).isdisjoint(old)
    assert repo.config is before  # nothing is persisted until the admin confirms

    assert service.rekey_confirm(begun["rekey_id"], new[:2]) == {"config_version": 2}
    after = repo.config
    assert after.passphrase == before.passphrase  # FR-013: passphrase protection untouched
    assert after.recovery.share_set_id != before.recovery.share_set_id
    assert after.initialized_at == before.initialized_at

    restarted = restart(repo, log, clock)
    with pytest.raises(AuthenticationFailed):  # old shares now belong to another set
        restarted.unseal_with_shares(old[:2])
    assert restarted.unseal_with_shares([new[2], new[0]])["state"] == "unsealed"
    restarted.seal()
    assert restarted.unseal_with_passphrase(GOOD_PASSPHRASE)["state"] == "unsealed"


def test_new_shares_still_work_after_a_later_passphrase_change(env):
    repo, log, clock, service, _old = env
    begun = service.rekey_begin(PROOF)
    service.rekey_confirm(begun["rekey_id"], begun["shares"][:2])
    service.change_passphrase(PROOF, "a brand new passphrase")
    assert restart(repo, log, clock).unseal_with_shares(begun["shares"][1:])["state"] == "unsealed"


def test_the_proof_can_be_two_current_shares(env):
    _repo, log, _clock, service, old = env
    begun = service.rekey_begin({"shares": old[:2]})
    assert begun["rekey_id"]
    entry = [e for e in log.entries() if e.operation is Operation.SHARES_REKEY][-1]
    assert (entry.method, entry.result) == (Method.RECOVERY, Result.SUCCESS)


def test_invalid_proof_nothing_pending_and_a_failure_is_counted(env):
    repo, _log, _clock, service, _old = env
    before = repo.config
    with pytest.raises(AuthenticationFailed):
        service.rekey_begin({"passphrase": "not the current one"})
    assert service.status()["consecutive_failures"] == 1
    with pytest.raises(InvalidInput):
        service.rekey_confirm("anything", ["x", "y"])
    assert repo.config is before


def test_malformed_proofs_are_validation_errors_and_not_counted(env):
    _repo, _log, _clock, service, old = env
    for bad in ({}, "x", {"passphrase": 5}, {"shares": old[:1]}, {"passphrase": GOOD_PASSPHRASE, "shares": old[:2]}):
        with pytest.raises(InvalidInput):
            service.rekey_begin(bad)  # type: ignore[arg-type]
    assert service.status()["consecutive_failures"] == 0


def test_confirming_with_the_old_or_wrong_shares_changes_nothing(env):
    repo, _log, _clock, service, old = env
    before = repo.config
    begun = service.rekey_begin(PROOF)
    with pytest.raises(InvalidInput):
        service.rekey_confirm(begun["rekey_id"], old[:2])
    with pytest.raises(InvalidInput):
        service.rekey_confirm(begun["rekey_id"], begun["shares"][:1])
    with pytest.raises(InvalidInput):
        service.rekey_confirm("another id", begun["shares"][:2])
    assert repo.config is before


def test_abort_and_expiry_discard_the_pending_set(env):
    repo, _log, clock, service, _old = env
    before = repo.config
    begun = service.rekey_begin(PROOF)
    assert service.rekey_abort(begun["rekey_id"]) == {}
    with pytest.raises(InvalidInput):
        service.rekey_confirm(begun["rekey_id"], begun["shares"][:2])

    begun = service.rekey_begin(PROOF)
    clock.advance(timedelta(minutes=16))
    with pytest.raises(InvalidInput):
        service.rekey_confirm(begun["rekey_id"], begun["shares"][:2])
    assert repo.config is before


def test_rekey_needs_to_be_unsealed_and_sealing_drops_the_pending_set(env):
    repo, _log, _clock, service, _old = env
    begun = service.rekey_begin(PROOF)
    service.seal()
    with pytest.raises(StateConflict) as err:
        service.rekey_begin(PROOF)
    assert err.value.state.value == "sealed"
    with pytest.raises(StateConflict):
        service.rekey_confirm(begun["rekey_id"], begun["shares"][:2])
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    with pytest.raises(InvalidInput):  # the pending set did not survive sealing
        service.rekey_confirm(begun["rekey_id"], begun["shares"][:2])
    assert repo.config.config_version == 1


def test_new_shares_are_valid_shares_of_a_new_set(env):
    _repo, _log, _clock, service, old = env
    new = [RecoveryShare.parse(t) for t in service.rekey_begin(PROOF)["shares"]]
    first = RecoveryShare.parse(old[0])
    assert {s.share_set_id for s in new} != {first.share_set_id}
    assert sorted(s.index for s in new) == [1, 2, 3]
