from datetime import timedelta
from itertools import combinations

import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import InvalidInput, StateConflict
from hecate.domain.sealing.share import RecoveryShare
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider
from tests.fakes import (
    GOOD_PASSPHRASE,
    FakeClock,
    InMemoryAccessLog,
    InMemorySealRepository,
    make_service,
)

crypto = SoftwareCryptoProvider()


@pytest.fixture
def env():
    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    return repo, log, clock, make_service(repo=repo, access_log=log, clock=clock)


def test_begin_returns_three_distinct_shares_and_persists_nothing(env):
    repo, _log, clock, service = env
    out = service.init_begin(GOOD_PASSPHRASE)
    shares = [RecoveryShare.parse(t) for t in out["shares"]]
    assert sorted(s.index for s in shares) == [1, 2, 3]
    assert len({s.fragments for s in shares}) == 3
    assert len({s.share_set_id for s in shares}) == 1
    assert out["init_id"]
    assert out["expires_at"] == (clock.now() + timedelta(minutes=15)).isoformat()
    assert "irrecoverable" in out["warning"].lower() or "cannot be recovered" in out["warning"].lower()
    assert repo.config is None
    assert service.status()["state"] == "uninitialized"


def test_confirm_with_any_two_shown_shares_leaves_it_sealed(env):
    repo, log, _clock, service = env
    out = service.init_begin(GOOD_PASSPHRASE)
    result = service.init_confirm(out["init_id"], out["shares"][:2])
    assert result == {"state": "sealed"}
    assert service.status()["state"] == "sealed"
    assert repo.config.config_version == 1
    assert any(e.operation is Operation.INIT and e.result is Result.SUCCESS
               and e.detail == "confirm" and e.method is Method.LOCAL for e in log.entries())


@pytest.mark.parametrize("pair", list(combinations(range(3), 2)))
def test_stored_protections_recover_the_same_root(pair):
    """Both the passphrase and any pair of shares must open the stored root."""
    repo = InMemorySealRepository()
    service = make_service(repo=repo)
    out = service.init_begin(GOOD_PASSPHRASE)
    service.init_confirm(out["init_id"], out["shares"][:2])
    cfg = repo.config

    p = cfg.passphrase
    key = crypto.derive_key(GOOD_PASSPHRASE, p.salt, p.kdf_opslimit, p.kdf_memlimit)
    root_from_passphrase = crypto.aead_decrypt(key, p.nonce, p.ciphertext, p.aad())

    shares = [RecoveryShare.parse(out["shares"][i]) for i in pair]
    recovery_key = crypto.shamir_combine([(s.index, s.fragments) for s in shares])
    r = cfg.recovery
    root_from_shares = crypto.aead_decrypt(recovery_key, r.nonce, r.ciphertext, r.aad())

    assert len(root_from_passphrase) == 32
    assert root_from_passphrase == root_from_shares
    assert r.share_set_id == shares[0].share_set_id and (r.threshold, r.share_count) == (2, 3)


def test_confirm_rejects_shares_that_were_not_shown(env):
    repo, _log, _clock, service = env
    other = make_service().init_begin(GOOD_PASSPHRASE)
    out = service.init_begin(GOOD_PASSPHRASE)
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], other["shares"][:2])
    assert repo.config is None


def test_confirm_needs_two_distinct_shares(env):
    _repo, _log, _clock, service = env
    out = service.init_begin(GOOD_PASSPHRASE)
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], out["shares"][:1])
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], [out["shares"][0], out["shares"][0]])
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], [out["shares"][0], out["shares"][1][:-4] + "AAAA"])
    assert service.status()["state"] == "uninitialized"


def test_abort_expiry_and_unknown_id_persist_nothing(env):
    repo, _log, clock, service = env
    out = service.init_begin(GOOD_PASSPHRASE)
    service.init_abort(out["init_id"])
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], out["shares"][:2])

    out = service.init_begin(GOOD_PASSPHRASE)
    clock.advance(timedelta(minutes=16))
    with pytest.raises(InvalidInput):
        service.init_confirm(out["init_id"], out["shares"][:2])

    with pytest.raises(InvalidInput):
        service.init_confirm("unknown", out["shares"][:2])
    assert repo.config is None
    assert service.status()["state"] == "uninitialized"


def test_short_passphrase_is_rejected_without_leaving_a_pending_init(env):
    _repo, log, _clock, service = env
    with pytest.raises(InvalidInput):
        service.init_begin("short")
    assert any(e.result is Result.INVALID_INPUT for e in log.entries())


def test_cannot_initialize_twice(env):
    repo, log, _clock, service = env
    out = service.init_begin(GOOD_PASSPHRASE)
    service.init_confirm(out["init_id"], out["shares"][:2])
    before = repo.config
    with pytest.raises(StateConflict) as err:
        service.init_begin(GOOD_PASSPHRASE)
    assert err.value.state.value == "sealed"
    with pytest.raises(StateConflict):
        service.init_confirm(out["init_id"], out["shares"][:2])
    assert repo.config is before
    assert any(e.result is Result.REJECTED_CONFLICT for e in log.entries())
