import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import AuthenticationFailed, InvalidInput, StateConflict
from tests.fakes import (
    GOOD_PASSPHRASE,
    FakeClock,
    InMemoryAccessLog,
    InMemorySealRepository,
    initialize,
    make_service,
)

NEW = "a brand new passphrase"


@pytest.fixture
def env():
    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    service = make_service(repo=repo, access_log=log, clock=clock)
    shares = initialize(service)
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    return repo, log, clock, service, shares


def restart(repo, log, clock):
    return make_service(repo=repo, access_log=log, clock=clock)


def test_change_with_the_current_passphrase(env):
    repo, log, clock, service, shares = env
    before = repo.config
    out = service.change_passphrase({"passphrase": GOOD_PASSPHRASE}, NEW)
    assert out == {"config_version": 2}

    after = repo.config
    assert after.config_version == 2 and after.updated_at >= before.updated_at
    assert after.recovery == before.recovery  # FR-013: recovery protection untouched
    assert after.passphrase.salt != before.passphrase.salt
    assert after.initialized_at == before.initialized_at

    restarted = restart(repo, log, clock)  # only the new passphrase opens it after a restart
    with pytest.raises(AuthenticationFailed):
        restarted.unseal_with_passphrase(GOOD_PASSPHRASE)
    restarted.unseal_with_passphrase(NEW)
    entry = [e for e in log.entries() if e.operation is Operation.PASSPHRASE_CHANGE][-1]
    assert (entry.result, entry.method) == (Result.SUCCESS, Method.PASSPHRASE)


def test_shares_still_unseal_after_a_passphrase_change(env):
    repo, log, clock, service, shares = env
    service.change_passphrase({"passphrase": GOOD_PASSPHRASE}, NEW)
    restarted = restart(repo, log, clock)
    assert restarted.unseal_with_shares(shares[1:])["state"] == "unsealed"


def test_change_proving_with_two_shares_clears_the_recommendation():
    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    service = make_service(repo=repo, access_log=log, clock=clock)
    shares = initialize(service)
    service.unseal_with_shares(shares[:2])
    assert service.status()["recommend_passphrase_change"] is True

    service.change_passphrase({"shares": [shares[0], shares[2]]}, NEW)
    assert service.status()["recommend_passphrase_change"] is False
    entry = [e for e in log.entries() if e.operation is Operation.PASSPHRASE_CHANGE][-1]
    assert entry.method is Method.RECOVERY
    restart(repo, log, clock).unseal_with_passphrase(NEW)


def test_without_a_valid_proof_nothing_changes_and_a_failure_is_counted(env):
    repo, log, _clock, service, _shares = env
    before = repo.config
    with pytest.raises(AuthenticationFailed):
        service.change_passphrase({"passphrase": "not the current one"}, NEW)
    foreign = initialize(make_service())
    with pytest.raises(AuthenticationFailed):
        service.change_passphrase({"shares": foreign[:2]}, NEW)
    assert repo.config is before
    assert service.status()["consecutive_failures"] == 2
    assert NEW not in repr(log.entries())


def test_invalid_requests_are_validation_errors_and_not_failures(env):
    repo, _log, _clock, service, shares = env
    before = repo.config
    with pytest.raises(InvalidInput):
        service.change_passphrase({"passphrase": GOOD_PASSPHRASE}, "short")
    with pytest.raises(InvalidInput):
        service.change_passphrase({}, NEW)
    with pytest.raises(InvalidInput):
        service.change_passphrase({"passphrase": GOOD_PASSPHRASE, "shares": shares[:2]}, NEW)
    with pytest.raises(InvalidInput):
        service.change_passphrase({"shares": shares[:1]}, NEW)
    assert repo.config is before and service.status()["consecutive_failures"] == 0


def test_change_needs_to_be_unsealed(env):
    _repo, _log, _clock, service, _shares = env
    service.seal()
    with pytest.raises(StateConflict) as err:
        service.change_passphrase({"passphrase": GOOD_PASSPHRASE}, NEW)
    assert err.value.state.value == "sealed"
