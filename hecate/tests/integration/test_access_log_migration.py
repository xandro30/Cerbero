import sqlite3
from datetime import UTC, datetime

from hecate.domain.sealing.access_log import AccessLogEntry, Method, Operation, Result
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository

AT = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)

# The access_log table exactly as the feature 001 created it.
OLD_SCHEMA = """
CREATE TABLE access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    operation TEXT NOT NULL,
    method TEXT NOT NULL,
    result TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE TRIGGER access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
"""


def make_old_database(path):
    raw = sqlite3.connect(path)
    raw.executescript(OLD_SCHEMA)
    raw.executemany(
        "INSERT INTO access_log (at, operation, method, result, detail) VALUES (?, ?, ?, ?, ?)",
        [(AT.isoformat(), "unseal", "passphrase", "success", "passphrase"),
         (AT.isoformat(), "seal", "local", "success", "seal")],
    )
    raw.commit()
    raw.close()


def columns(path) -> list[str]:
    raw = sqlite3.connect(path)
    try:
        return [row[1] for row in raw.execute("PRAGMA table_info(access_log)")]
    finally:
        raw.close()


def test_a_feature_001_database_gains_the_new_columns_without_losing_rows(tmp_path):
    path = tmp_path / "hecate.db"
    make_old_database(path)
    assert "subject" not in columns(path)

    log = SqliteAccessLog(path)
    try:
        assert {"subject", "path", "version"} <= set(columns(path))
        old = log.entries()
        assert [(e.operation, e.method, e.result) for e in old] == [
            (Operation.UNSEAL, Method.PASSPHRASE, Result.SUCCESS),
            (Operation.SEAL, Method.LOCAL, Result.SUCCESS),
        ]
        assert all((e.subject, e.path, e.version) == (None, None, None) for e in old)
    finally:
        log.close()


def test_new_rows_keep_subject_path_and_version_and_old_ones_stay_empty(tmp_path):
    path = tmp_path / "hecate.db"
    make_old_database(path)
    log = SqliteAccessLog(path)
    try:
        log.append(AccessLogEntry(
            AT, Operation.SECRET_ADD_VERSION, Method.LOCAL, Result.SUCCESS, "add_version",
            subject="admin", path="secret://hermes/token", version=2,
        ))
        log.append(AccessLogEntry(AT, Operation.SEAL, Method.LOCAL, Result.SUCCESS, "seal"))
        entries = log.entries()
    finally:
        log.close()
    assert len(entries) == 4
    assert (entries[0].subject, entries[0].path, entries[0].version) == (None, None, None)
    assert (entries[2].subject, entries[2].path, entries[2].version) == (
        "admin", "secret://hermes/token", 2)
    assert (entries[3].subject, entries[3].path, entries[3].version) == (None, None, None)


def test_opening_twice_and_from_other_connections_is_harmless(tmp_path):
    path = tmp_path / "hecate.db"
    make_old_database(path)
    for _ in range(2):
        SqliteAccessLog(path).close()
        SqliteSealRepository(path).close()
    assert columns(path).count("subject") == 1
    log = SqliteAccessLog(path)
    try:
        assert len(log.entries()) == 2
    finally:
        log.close()


def test_the_log_stays_append_only_after_the_migration(tmp_path):
    import pytest

    path = tmp_path / "hecate.db"
    make_old_database(path)
    SqliteAccessLog(path).close()
    raw = sqlite3.connect(path)
    try:
        with pytest.raises(sqlite3.DatabaseError, match="append-only"):
            raw.execute("DELETE FROM access_log")
    finally:
        raw.close()


def test_a_fresh_database_has_the_columns_from_the_start(tmp_path):
    path = tmp_path / "hecate.db"
    SqliteAccessLog(path).close()
    assert {"subject", "path", "version"} <= set(columns(path))
