import pytest

from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository
from tests.fakes import GOOD_PASSPHRASE, FakeClock, initialize, make_service

PROOF = {"passphrase": GOOD_PASSPHRASE}


class PowerCutAfterSave:
    """Repository wrapper that writes the configuration and then fails inside the transaction."""

    def __init__(self, inner) -> None:
        self._inner = inner

    def __getattr__(self, name):
        return getattr(self._inner, name)

    def save_config(self, config) -> None:
        self._inner.save_config(config)
        raise RuntimeError("power cut")


def boot(path, *, crashing=False):
    repo, log = SqliteSealRepository(path), SqliteAccessLog(path)
    used = PowerCutAfterSave(repo) if crashing else repo
    return repo, log, make_service(repo=used, access_log=log, clock=FakeClock())


def assert_old_protection_intact(path, shares):
    repo, log, service = boot(path)
    assert repo.load_config().config_version == 1  # SC-008: nothing changed
    assert service.unseal_with_passphrase(GOOD_PASSPHRASE)["state"] == "unsealed"
    service.seal()
    assert service.unseal_with_shares(shares[:2])["state"] == "unsealed"
    repo.close()
    log.close()


def test_power_cut_inside_passphrase_change_keeps_the_previous_protection(tmp_path):
    db = tmp_path / "hecate.db"
    repo, log, service = boot(db)
    shares = initialize(service)
    repo.close()
    log.close()

    repo, log, crashing = boot(db, crashing=True)
    crashing.unseal_with_passphrase(GOOD_PASSPHRASE)
    with pytest.raises(RuntimeError):
        crashing.change_passphrase(PROOF, "a brand new passphrase")
    repo.close()
    log.close()

    assert_old_protection_intact(db, shares)
    repo, log, service = boot(db)
    from hecate.domain.sealing.errors import AuthenticationFailed

    with pytest.raises(AuthenticationFailed):  # the half-written new passphrase did not survive
        service.unseal_with_passphrase("a brand new passphrase")
    repo.close()
    log.close()


def test_power_cut_inside_rekey_confirm_keeps_the_old_shares(tmp_path):
    db = tmp_path / "hecate.db"
    repo, log, service = boot(db)
    shares = initialize(service)
    repo.close()
    log.close()

    repo, log, crashing = boot(db, crashing=True)
    crashing.unseal_with_passphrase(GOOD_PASSPHRASE)
    begun = crashing.rekey_begin(PROOF)
    with pytest.raises(RuntimeError):
        crashing.rekey_confirm(begun["rekey_id"], begun["shares"][:2])
    repo.close()
    log.close()

    assert_old_protection_intact(db, shares)
