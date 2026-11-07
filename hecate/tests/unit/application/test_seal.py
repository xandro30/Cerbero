import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import StateConflict
from tests.fakes import GOOD_PASSPHRASE, InMemoryAccessLog, InMemorySealRepository, initialize, make_service


@pytest.fixture
def env():
    log = InMemoryAccessLog()
    service = make_service(repo=InMemorySealRepository(), access_log=log)
    initialize(service)
    return log, service


def seal_entries(log):
    return [e for e in log.entries() if e.operation is Operation.SEAL]


def test_sealing_unsealed_hecate_wipes_the_root_and_logs_it(env):
    log, service = env
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    holder = service._root  # the object that holds the root in memory
    assert holder is not None and not holder.released

    assert service.seal() == {"state": "sealed"}

    assert holder.released
    with pytest.raises(ValueError):
        holder.get()
    status = service.status()
    assert status["state"] == "sealed"
    assert status["unsealed_via"] is None and status["recommend_passphrase_change"] is False
    entry = seal_entries(log)[-1]
    assert (entry.result, entry.method) == (Result.SUCCESS, Method.LOCAL)


def test_sealing_when_sealed_is_a_conflict_that_changes_nothing(env):
    log, service = env
    with pytest.raises(StateConflict) as err:
        service.seal()
    assert err.value.state.value == "sealed"
    assert service.status()["state"] == "sealed"
    assert seal_entries(log)[-1].result is Result.REJECTED_CONFLICT


def test_sealing_when_uninitialized_is_a_conflict():
    with pytest.raises(StateConflict) as err:
        make_service().seal()
    assert err.value.state.value == "uninitialized"


def test_can_unseal_again_after_sealing(env):
    _log, service = env
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    service.seal()
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert service.status()["state"] == "unsealed"
