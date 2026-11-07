import sqlite3
from datetime import UTC, datetime

import pytest

from hecate.domain.sealing.access_log import AccessLogEntry, Method, Operation, Result
from hecate.domain.sealing.attempts import AttemptPolicy
from hecate.domain.sealing.protection import PassphraseProtection, RecoveryProtection, SealConfig
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository

NOW = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)


def make_config(version: int = 1) -> SealConfig:
    return SealConfig(
        config_version=version,
        initialized_at=NOW,
        updated_at=NOW,
        passphrase=PassphraseProtection(
            version=version, kdf="argon2id", kdf_opslimit=1, kdf_memlimit=8192,
            salt=b"s" * 16, aead="AES-256-GCM", nonce=b"n" * 12, ciphertext=b"c" * 48,
        ),
        recovery=RecoveryProtection(
            version=version, share_set_id=b"\x01\x02\x03\x04", threshold=2, share_count=3,
            aead="AES-256-GCM", nonce=b"m" * 12, ciphertext=b"d" * 48,
        ),
    )


@pytest.fixture
def db_path(tmp_path):
    return tmp_path / "hecate.db"


def test_config_and_attempts_survive_reopen(db_path):
    repo = SqliteSealRepository(db_path)
    assert repo.load_config() is None
    assert repo.load_attempts() == AttemptPolicy()
    attempts = AttemptPolicy().register_failure(NOW).register_failure(NOW).register_failure(NOW)
    with repo.transaction():
        repo.save_config(make_config())
        repo.save_attempts(attempts)
    repo.close()

    reopened = SqliteSealRepository(db_path)
    assert reopened.load_config() == make_config()
    assert reopened.load_attempts() == attempts
    reopened.close()


def test_failure_inside_transaction_leaves_no_changes(db_path):
    repo = SqliteSealRepository(db_path)
    with repo.transaction():
        repo.save_config(make_config(1))
    with pytest.raises(RuntimeError):
        with repo.transaction():
            repo.save_config(make_config(2))
            raise RuntimeError("power cut")
    assert repo.load_config().config_version == 1
    repo.close()


def test_data_dir_files_are_private(db_path):
    repo = SqliteSealRepository(db_path)
    assert (db_path.stat().st_mode & 0o777) == 0o600
    repo.close()


def test_access_log_appends_and_rejects_update_and_delete(db_path):
    repo = SqliteSealRepository(db_path)
    log = SqliteAccessLog(db_path)
    log.append(AccessLogEntry(NOW, Operation.UNSEAL, Method.PASSPHRASE, Result.FAILURE, "bad passphrase"))
    log.append(AccessLogEntry(NOW, Operation.SEAL, Method.LOCAL, Result.SUCCESS, ""))
    entries = log.entries()
    assert [e.operation for e in entries] == [Operation.UNSEAL, Operation.SEAL]
    log.close()
    repo.close()

    raw = sqlite3.connect(db_path)
    with pytest.raises(sqlite3.DatabaseError):
        raw.execute("UPDATE access_log SET result = 'success'")
    with pytest.raises(sqlite3.DatabaseError):
        raw.execute("DELETE FROM access_log")
    raw.close()
