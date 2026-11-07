import pytest

from hecate.domain.sealing.errors import AuthenticationFailed, RateLimited
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository
from tests.fakes import GOOD_PASSPHRASE, FakeClock, initialize, make_service


def boot(path, clock):
    repo, log = SqliteSealRepository(path), SqliteAccessLog(path)
    return repo, log, make_service(repo=repo, access_log=log, clock=clock)


def test_after_unsealing_a_restart_leaves_hecate_sealed(tmp_path):
    db, clock = tmp_path / "hecate.db", FakeClock()
    repo, log, service = boot(db, clock)
    initialize(service)
    service.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert service.status()["state"] == "unsealed"
    repo.close()
    log.close()

    repo2, log2, restarted = boot(db, clock)
    assert restarted.status()["state"] == "sealed"  # FR-006, SC-003
    restarted.unseal_with_passphrase(GOOD_PASSPHRASE)
    assert restarted.status()["state"] == "unsealed"
    repo2.close()
    log2.close()


def test_failed_attempts_and_pending_wait_survive_a_restart(tmp_path):
    db, clock = tmp_path / "hecate.db", FakeClock()
    repo, log, service = boot(db, clock)
    initialize(service)
    for _ in range(3):
        with pytest.raises(AuthenticationFailed):
            service.unseal_with_passphrase("wrong wrong wrong")
    repo.close()
    log.close()

    repo2, log2, restarted = boot(db, clock)  # "restart to skip the wait" does not work (FR-016)
    status = restarted.status()
    assert status["consecutive_failures"] == 3 and status["retry_after_s"] == 60
    with pytest.raises(RateLimited):
        restarted.unseal_with_passphrase(GOOD_PASSPHRASE)
    repo2.close()
    log2.close()


def test_sc007_no_more_than_ten_evaluated_attempts_per_hour_even_restarting_and_alternating():
    """An attacker who restarts Hécate and alternates passphrase and shares gains nothing (SC-007)."""
    from datetime import timedelta

    from hecate.domain.sealing.errors import AuthenticationFailed, RateLimited
    from tests.fakes import FakeClock, InMemoryAccessLog, InMemorySealRepository

    repo, log, clock = InMemorySealRepository(), InMemoryAccessLog(), FakeClock()
    initialize(make_service(repo=repo, access_log=log, clock=clock))
    foreign = initialize(make_service())
    start, evaluated, turn = clock.now(), 0, 0

    while clock.now() - start <= timedelta(hours=1):
        service = make_service(repo=repo, access_log=log, clock=clock)  # restart before every try
        try:
            if turn % 2 == 0:
                service.unseal_with_passphrase("wrong wrong wrong")
            else:
                service.unseal_with_shares(foreign[:2])
            raise AssertionError("a wrong proof must not unseal")
        except AuthenticationFailed:
            evaluated += 1
        except RateLimited as wait:
            clock.advance(timedelta(seconds=wait.retry_after_s))
            continue
        turn += 1

    assert evaluated <= 10
    assert evaluated >= 9  # the wait series really lets the attacker use its allowance
