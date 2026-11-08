from datetime import timedelta

import pytest

from hecate.domain.sealing.errors import AuthenticationFailed, RateLimited, StateConflict
from hecate.domain.sealing.state import SealState
from tests.fakes import GOOD_PASSPHRASE, FakeClock, initialize, make_service


def unsealed_service(clock=None):
    service = make_service(clock=clock)
    shares = initialize(service)
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    return service, shares


def test_root_is_available_only_while_unsealed():
    service = make_service()
    with pytest.raises(StateConflict) as exc:
        service.root()
    assert exc.value.state is SealState.UNINITIALIZED

    initialize(service)
    with pytest.raises(StateConflict) as exc:
        service.root()
    assert exc.value.state is SealState.SEALED

    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    root = service.root()
    assert isinstance(root, bytes) and len(root) == 32

    service.seal()
    with pytest.raises(StateConflict):
        service.root()


def test_root_is_the_same_across_calls_and_unseal_methods():
    service, shares = unsealed_service()
    first = service.root()
    assert service.root() == first
    service.seal()
    service.unseal_with_shares(shares[:2])
    assert service.root() == first


def test_verify_proof_accepts_the_current_passphrase():
    service, _ = unsealed_service()
    service.verify_proof({"passphrase": GOOD_PASSPHRASE})


def test_verify_proof_accepts_two_shares():
    service, shares = unsealed_service()
    service.verify_proof({"shares": shares[1:]})


def test_verify_proof_rejects_a_wrong_passphrase_and_counts_the_failure():
    service, _ = unsealed_service()
    with pytest.raises(AuthenticationFailed):
        service.verify_proof({"passphrase": "wrong wrong wrong"})
    assert service.status()["consecutive_failures"] == 1


def test_verify_proof_rejects_wrong_shares_and_counts_the_failure():
    service, shares = unsealed_service()
    other = initialize(make_service())
    with pytest.raises(AuthenticationFailed):
        service.verify_proof({"shares": other[:2]})
    assert service.status()["consecutive_failures"] == 1
    service.verify_proof({"shares": shares[:2]})


def test_the_counter_is_shared_with_unsealing_and_resets_on_success():
    clock = FakeClock()
    service, _ = unsealed_service(clock)
    for _ in range(2):
        with pytest.raises(AuthenticationFailed):
            service.verify_proof({"passphrase": "wrong wrong wrong"})
    service.seal()
    with pytest.raises(AuthenticationFailed):
        service.unseal_with_passphrase("wrong wrong wrong")
    assert service.status()["consecutive_failures"] == 3
    clock.advance(timedelta(seconds=61))
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    service.verify_proof({"passphrase": GOOD_PASSPHRASE})
    assert service.status()["consecutive_failures"] == 0


def test_verify_proof_respects_the_wait_even_with_a_correct_proof():
    clock = FakeClock()
    service, _ = unsealed_service(clock)
    for _ in range(3):
        with pytest.raises(AuthenticationFailed):
            service.verify_proof({"passphrase": "wrong wrong wrong"})
    with pytest.raises(RateLimited) as exc:
        service.verify_proof({"passphrase": GOOD_PASSPHRASE})
    assert exc.value.retry_after_s == 60
    clock.advance(timedelta(seconds=61))
    service.verify_proof({"passphrase": GOOD_PASSPHRASE})


@pytest.mark.parametrize("proof", [None, {}, {"passphrase": "x", "shares": []}, {"passphrase": ""}, "text"])
def test_verify_proof_rejects_malformed_proofs_without_counting(proof):
    from hecate.domain.sealing.errors import InvalidInput

    service, _ = unsealed_service()
    with pytest.raises(InvalidInput):
        service.verify_proof(proof)
    assert service.status()["consecutive_failures"] == 0


def test_verify_proof_needs_the_service_unsealed():
    service, _ = unsealed_service()
    service.seal()
    with pytest.raises(StateConflict):
        service.verify_proof({"passphrase": GOOD_PASSPHRASE})
