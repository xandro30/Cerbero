from datetime import timedelta
from itertools import permutations

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import (
    AuthenticationFailed,
    InvalidInput,
    RateLimited,
    StateConflict,
)
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
    shares = initialize(service)
    return repo, log, clock, service, shares


@pytest.mark.parametrize("order", list(permutations(range(3), 2)))
def test_any_ordered_pair_of_shares_unseals_and_recommends_a_new_passphrase(env, order):
    _repo, log, _clock, service, shares = env
    out = service.unseal_with_shares([shares[i] for i in order])
    assert out == {"state": "unsealed", "unsealed_via": "recovery", "recommend_passphrase_change": True}
    status = service.status()
    assert status["state"] == "unsealed" and status["recommend_passphrase_change"] is True
    entry = [e for e in log.entries() if e.operation is Operation.UNSEAL][-1]
    assert (entry.result, entry.method) == (Result.SUCCESS, Method.RECOVERY)


def test_all_three_shares_also_work(env):
    _repo, _log, _clock, service, shares = env
    assert service.unseal_with_shares(shares)["state"] == "unsealed"


def test_one_share_or_a_repeated_share_is_invalid_input_and_not_a_failure(env):
    _repo, _log, _clock, service, shares = env
    with pytest.raises(InvalidInput):
        service.unseal_with_shares([shares[0]])
    with pytest.raises(InvalidInput):
        service.unseal_with_shares([shares[0], shares[0]])
    with pytest.raises(InvalidInput):
        service.unseal_with_shares("not a list")  # type: ignore[arg-type]
    assert service.status()["state"] == "sealed" and service.status()["consecutive_failures"] == 0


def test_a_badly_copied_share_is_detected_without_spending_an_attempt(env):
    _repo, log, _clock, service, shares = env
    mangled = shares[1][:-1] + ("A" if shares[1][-1] != "A" else "B")
    with pytest.raises(InvalidInput):
        service.unseal_with_shares([shares[0], mangled])
    assert service.status()["consecutive_failures"] == 0
    assert any(e.result is Result.INVALID_INPUT and e.method is Method.RECOVERY for e in log.entries())


def test_shares_from_another_set_are_an_authentication_failure(env):
    _repo, _log, _clock, service, shares = env
    other = make_service()
    foreign = initialize(other)
    with pytest.raises(AuthenticationFailed):
        service.unseal_with_shares([foreign[0], foreign[1]])
    with pytest.raises(AuthenticationFailed):
        service.unseal_with_shares([shares[0], foreign[1]])
    assert service.status()["state"] == "sealed" and service.status()["consecutive_failures"] == 2


def test_failures_with_passphrase_and_with_shares_share_one_counter(env):
    _repo, _log, clock, service, shares = env
    foreign = initialize(make_service())
    for _ in range(2):
        with pytest.raises(AuthenticationFailed):
            service.unseal_with_passphrase("wrong wrong wrong")
    with pytest.raises(AuthenticationFailed):
        service.unseal_with_shares(foreign[:2])
    assert service.status()["consecutive_failures"] == 3
    with pytest.raises(RateLimited) as err:
        service.unseal_with_shares(shares[:2])  # valid shares, but the wait applies
    assert err.value.retry_after_s == 60
    clock.advance(timedelta(seconds=61))
    assert service.unseal_with_shares(shares[:2])["unsealed_via"] == "recovery"
    assert service.status()["consecutive_failures"] == 0


def test_conflicts_when_unsealed_or_uninitialized(env):
    _repo, _log, _clock, service, shares = env
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    with pytest.raises(StateConflict) as err:
        service.unseal_with_shares(shares[:2])
    assert err.value.state.value == "unsealed"
    with pytest.raises(StateConflict):
        make_service().unseal_with_shares(shares[:2])
