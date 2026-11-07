import json
import os
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path

from hecate.domain.sealing.access_log import AccessLogEntry, Method, Operation, Result
from hecate.domain.sealing.attempts import AttemptPolicy
from hecate.domain.sealing.protection import PassphraseProtection, RecoveryProtection, SealConfig

_SCHEMA = """
CREATE TABLE IF NOT EXISTS seal_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    config_version INTEGER NOT NULL,
    initialized_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    passphrase TEXT NOT NULL,
    recovery TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS attempts (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    consecutive_failures INTEGER NOT NULL,
    next_attempt_at TEXT
);
CREATE TABLE IF NOT EXISTS access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    operation TEXT NOT NULL,
    method TEXT NOT NULL,
    result TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
"""


def _connect(path: Path) -> sqlite3.Connection:
    fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    os.close(fd)
    # The control server runs each operation in a worker thread but serializes them with a lock,
    # so a connection is never used from two threads at once.
    conn = sqlite3.connect(path, isolation_level=None, timeout=10, check_same_thread=False)
    conn.execute("PRAGMA synchronous = FULL")
    conn.executescript(_SCHEMA)
    return conn


def _dump(obj: PassphraseProtection | RecoveryProtection) -> str:
    return json.dumps(
        {k: (v.hex() if isinstance(v, bytes) else v) for k, v in vars(obj).items()},
        sort_keys=True,
    )


def _load(cls, text: str):
    raw = json.loads(text)
    return cls(**{k: (bytes.fromhex(v) if k in _BYTES_FIELDS else v) for k, v in raw.items()})


_BYTES_FIELDS = {"salt", "nonce", "ciphertext", "share_set_id"}


class SqliteSealRepository:
    """Seal configuration and attempt counter in SQLite (ADR 0003)."""

    def __init__(self, path: Path) -> None:
        self._conn = _connect(Path(path))

    def close(self) -> None:
        self._conn.close()

    @contextmanager
    def transaction(self) -> Iterator[None]:
        self._conn.execute("BEGIN IMMEDIATE")
        try:
            yield
        except BaseException:
            self._conn.execute("ROLLBACK")
            raise
        else:
            self._conn.execute("COMMIT")

    def load_config(self) -> SealConfig | None:
        row = self._conn.execute(
            "SELECT config_version, initialized_at, updated_at, passphrase, recovery"
            " FROM seal_config WHERE id = 1"
        ).fetchone()
        if row is None:
            return None
        return SealConfig(
            config_version=row[0],
            initialized_at=datetime.fromisoformat(row[1]),
            updated_at=datetime.fromisoformat(row[2]),
            passphrase=_load(PassphraseProtection, row[3]),
            recovery=_load(RecoveryProtection, row[4]),
        )

    def save_config(self, config: SealConfig) -> None:
        self._conn.execute(
            "INSERT INTO seal_config (id, config_version, initialized_at, updated_at, passphrase, recovery)"
            " VALUES (1, ?, ?, ?, ?, ?)"
            " ON CONFLICT (id) DO UPDATE SET config_version = excluded.config_version,"
            " initialized_at = excluded.initialized_at, updated_at = excluded.updated_at,"
            " passphrase = excluded.passphrase, recovery = excluded.recovery",
            (
                config.config_version,
                config.initialized_at.isoformat(),
                config.updated_at.isoformat(),
                _dump(config.passphrase),
                _dump(config.recovery),
            ),
        )

    def load_attempts(self) -> AttemptPolicy:
        row = self._conn.execute(
            "SELECT consecutive_failures, next_attempt_at FROM attempts WHERE id = 1"
        ).fetchone()
        if row is None:
            return AttemptPolicy()
        nxt = datetime.fromisoformat(row[1]) if row[1] else None
        return AttemptPolicy(row[0], nxt)

    def save_attempts(self, attempts: AttemptPolicy) -> None:
        nxt = attempts.next_attempt_at.isoformat() if attempts.next_attempt_at else None
        self._conn.execute(
            "INSERT INTO attempts (id, consecutive_failures, next_attempt_at) VALUES (1, ?, ?)"
            " ON CONFLICT (id) DO UPDATE SET consecutive_failures = excluded.consecutive_failures,"
            " next_attempt_at = excluded.next_attempt_at",
            (attempts.consecutive_failures, nxt),
        )


class SqliteAccessLog:
    """Append-only access log; its own connection so a rolled-back change still leaves its trace."""

    def __init__(self, path: Path) -> None:
        self._conn = _connect(Path(path))

    def close(self) -> None:
        self._conn.close()

    def append(self, entry: AccessLogEntry) -> None:
        self._conn.execute(
            "INSERT INTO access_log (at, operation, method, result, detail) VALUES (?, ?, ?, ?, ?)",
            (entry.at.isoformat(), entry.operation.value, entry.method.value,
             entry.result.value, entry.detail),
        )

    def entries(self) -> list[AccessLogEntry]:
        rows = self._conn.execute(
            "SELECT at, operation, method, result, detail FROM access_log ORDER BY id"
        ).fetchall()
        return [
            AccessLogEntry(datetime.fromisoformat(a), Operation(o), Method(m), Result(r), d)
            for a, o, m, r, d in rows
        ]
