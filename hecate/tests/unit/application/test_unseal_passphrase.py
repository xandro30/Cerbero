from datetime import timedelta

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import AuthenticationFailed, InvalidInput, RateLimited, StateConflict
from tests.fakes import (
    GOOD_PASSPHRASE,
    FakeClock,
    InMemoryAccessLog,
    InMemorySealRepository,
    initialize,
    make_service,
)


@pytest.fixture
def env():
    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    service = make_service(repo=repo, access_log=log, clock=clock)
    initialize(service)
    return repo, log, clock, service


def results(log, result):
    return [e for e in log.entries() if e.operation is Operation.UNSEAL and e.result is result]


def test_correct_passphrase_unseals(env):
    _repo, log, _clock, service = env
    out = service.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert out == {"state": "unsealed", "unsealed_via": "passphrase", "recommend_passphrase_change": False}
    status = service.status()
    assert status["state"] == "unsealed" and status["unsealed_via"] == "passphrase"
    assert results(log, Result.SUCCESS)[0].method is Method.PASSPHRASE


def test_wrong_passphrase_stays_sealed_and_counts_a_failure(env):
    _repo, log, _clock, service = env
    with pytest.raises(AuthenticationFailed):
        service.unseal_with_passphrase("not the passphrase")
    status = service.status()
    assert status["state"] == "sealed" and status["consecutive_failures"] == 1
    assert len(results(log, Result.FAILURE)) == 1
    assert "not the passphrase" not in repr(log.entries())


def test_wait_after_third_failure_blocks_even_the_right_passphrase(env):
    _repo, log, clock, service = env
    for _ in range(3):
        with pytest.raises(AuthenticationFailed):
            service.unseal_with_passphrase("wrong wrong wrong")
    with pytest.raises(RateLimited) as err:
        service.unseal_with_passphrase(GOOD_PASSPHRASE)  # not evaluated during the wait
    assert err.value.retry_after_s == 60
    assert service.status()["state"] == "sealed"
    assert len(results(log, Result.REJECTED_RATE_LIMITED)) == 1
    assert service.status()["consecutive_failures"] == 3  # a rejected attempt is not a failure

    clock.advance(timedelta(seconds=61))
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert service.status()["consecutive_failures"] == 0


def test_wait_grows_with_each_failure(env):
    _repo, _log, clock, service = env
    waits = []
    for _ in range(5):
        with pytest.raises(AuthenticationFailed):
            service.unseal_with_passphrase("wrong wrong wrong")
        waits.append(service.status()["retry_after_s"])
        clock.advance(timedelta(minutes=20))
    assert waits == [0, 0, 60, 120, 240]


def test_unsealing_when_unsealed_or_uninitialized_is_a_conflict(env):
    _repo, log, _clock, service = env
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    with pytest.raises(StateConflict) as err:
        service.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert err.value.state.value == "unsealed"
    assert results(log, Result.REJECTED_CONFLICT)
    with pytest.raises(StateConflict) as err:
        make_service().unseal_with_passphrase(GOOD_PASSPHRASE)
    assert err.value.state.value == "uninitialized"


@pytest.mark.parametrize("bad", [None, 12, ""])
def test_non_text_passphrase_is_invalid_input_and_does_not_count(env, bad):
    _repo, _log, _clock, service = env
    with pytest.raises(InvalidInput):
        service.unseal_with_passphrase(bad)
    assert service.status()["consecutive_failures"] == 0


def test_failures_and_wait_survive_a_new_process(env):
    repo, log, clock, service = env
    for _ in range(3):
        with pytest.raises(AuthenticationFailed):
            service.unseal_with_passphrase("wrong wrong wrong")
    restarted = make_service(repo=repo, access_log=log, clock=clock)
    assert restarted.status()["state"] == "sealed"  # FR-006: a new process never starts unsealed
    assert restarted.status()["consecutive_failures"] == 3
    with pytest.raises(RateLimited):
        restarted.unseal_with_passphrase(GOOD_PASSPHRASE)
