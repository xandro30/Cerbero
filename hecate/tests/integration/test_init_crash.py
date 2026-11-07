from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository
from tests.fakes import GOOD_PASSPHRASE, make_service


def build(path):
    repo, log = SqliteSealRepository(path), SqliteAccessLog(path)
    return repo, log, make_service(repo=repo, access_log=log)


def test_process_stopped_between_begin_and_confirm_stays_uninitialized(tmp_path):
    db = tmp_path / "hecate.db"
    repo, log, service = build(db)
    out = service.init_begin(GOOD_PASSPHRASE)
    assert out["init_id"]
    # The process dies here: nothing was confirmed and the pending init lived only in memory.
    repo.close()
    log.close()

    repo2, log2, restarted = build(db)
    assert restarted.status()["state"] == "uninitialized"
    assert repo2.load_config() is None
    # ...and starting again works.
    fresh = restarted.init_begin(GOOD_PASSPHRASE)
    restarted.init_confirm(fresh["init_id"], fresh["shares"][:2])
    assert restarted.status()["state"] == "sealed"
    repo2.close()
    log2.close()


def test_confirmed_init_survives_a_restart_as_sealed(tmp_path):
    db = tmp_path / "hecate.db"
    repo, log, service = build(db)
    out = service.init_begin(GOOD_PASSPHRASE)
    service.init_confirm(out["init_id"], out["shares"][:2])
    repo.close()
    log.close()

    repo2, log2, restarted = build(db)
    assert restarted.status()["state"] == "sealed"
    repo2.close()
    log2.close()
