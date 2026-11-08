import json
import os
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path

from hecate.application.ports import (
    ConcurrencyConflict,
    EventRecord,
    KeyMaterial,
    SecretMaterial,
)
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
    detail TEXT NOT NULL,
    subject TEXT,
    path TEXT,
    version INTEGER
);
CREATE TRIGGER IF NOT EXISTS access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TABLE IF NOT EXISTS events (
    stream TEXT NOT NULL,
    version INTEGER NOT NULL,
    type TEXT NOT NULL,
    data TEXT NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY (stream, version)
);
CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TABLE IF NOT EXISTS key_material (
    identity TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    ciphertext BLOB NOT NULL,
    PRIMARY KEY (identity, key_version)
);
CREATE TABLE IF NOT EXISTS secret_material (
    path TEXT NOT NULL,
    version INTEGER NOT NULL,
    key_version INTEGER NOT NULL,
    value_nonce BLOB NOT NULL,
    value_ciphertext BLOB NOT NULL,
    dek_nonce BLOB NOT NULL,
    dek_ciphertext BLOB NOT NULL,
    PRIMARY KEY (path, version)
);
"""

# Columns the access log gained after the feature 001: added to databases created before.
_ACCESS_LOG_NEW_COLUMNS = (("subject", "TEXT"), ("path", "TEXT"), ("version", "INTEGER"))


def _connect(path: Path) -> sqlite3.Connection:
    fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    os.close(fd)
    # The control server runs each operation in a worker thread but serializes them with a lock,
    # so a connection is never used from two threads at once.
    conn = sqlite3.connect(path, isolation_level=None, timeout=10, check_same_thread=False)
    conn.execute("PRAGMA synchronous = FULL")
    # A deleted key must not linger in freed pages (cryptographic erasure, FR-009).
    conn.execute("PRAGMA secure_delete = ON")
    conn.executescript(_SCHEMA)
    _migrate_access_log(conn)
    return conn


def _migrate_access_log(conn: sqlite3.Connection) -> None:
    present = {row[1] for row in conn.execute("PRAGMA table_info(access_log)")}
    for name, sql_type in _ACCESS_LOG_NEW_COLUMNS:
        if name not in present:
            try:
                conn.execute(f"ALTER TABLE access_log ADD COLUMN {name} {sql_type}")
            except sqlite3.OperationalError:
                # Another connection added it between the check and the change.
                if name not in {row[1] for row in conn.execute("PRAGMA table_info(access_log)")}:
                    raise


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
            "INSERT INTO access_log (at, operation, method, result, detail, subject, path, version)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (entry.at.isoformat(), entry.operation.value, entry.method.value,
             entry.result.value, entry.detail, entry.subject, entry.path, entry.version),
        )

    def entries(self) -> list[AccessLogEntry]:
        rows = self._conn.execute(
            "SELECT at, operation, method, result, detail, subject, path, version"
            " FROM access_log ORDER BY id"
        ).fetchall()
        return [
            AccessLogEntry(datetime.fromisoformat(a), Operation(o), Method(m), Result(r), d, sub, p, v)
            for a, o, m, r, d, sub, p, v in rows
        ]


class SqliteSecretStore:
    """Events and encrypted material in SQLite, on one connection with one shared transaction()."""

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

    # -- events --------------------------------------------------------------------------------

    def load(self, stream: str) -> list[EventRecord]:
        rows = self._conn.execute(
            "SELECT type, data, at FROM events WHERE stream = ? ORDER BY version", (stream,)
        ).fetchall()
        return [EventRecord(t, json.loads(d), datetime.fromisoformat(a)) for t, d, a in rows]

    def append(self, stream: str, events: list[EventRecord], expected_version: int) -> int:
        with self._atomic():
            current = self._conn.execute(
                "SELECT COALESCE(MAX(version), 0) FROM events WHERE stream = ?", (stream,)
            ).fetchone()[0]
            if current != expected_version:
                raise ConcurrencyConflict(current)
            for offset, event in enumerate(events, 1):
                self._conn.execute(
                    "INSERT INTO events (stream, version, type, data, at) VALUES (?, ?, ?, ?, ?)",
                    (stream, current + offset, event.type, json.dumps(event.data, sort_keys=True),
                     event.at.isoformat()),
                )
            return current + len(events)

    def streams(self, prefix: str) -> list[str]:
        rows = self._conn.execute(
            "SELECT DISTINCT stream FROM events WHERE substr(stream, 1, ?) = ? ORDER BY stream",
            (len(prefix), prefix),
        ).fetchall()
        return [r[0] for r in rows]

    # -- encrypted material --------------------------------------------------------------------

    def save_secret_material(self, path: str, version: int, material: SecretMaterial) -> None:
        self._conn.execute(
            "INSERT OR REPLACE INTO secret_material"
            " (path, version, key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext)"
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
            (path, version, material.key_version, material.value_nonce, material.value_ciphertext,
             material.dek_nonce, material.dek_ciphertext),
        )

    def load_secret_material(self, path: str, version: int) -> SecretMaterial | None:
        row = self._conn.execute(
            "SELECT key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext"
            " FROM secret_material WHERE path = ? AND version = ?",
            (path, version),
        ).fetchone()
        return SecretMaterial(row[0], row[1], row[2], row[3], row[4]) if row else None

    def delete_secret_material(self, path: str, version: int) -> None:
        self._conn.execute("DELETE FROM secret_material WHERE path = ? AND version = ?", (path, version))

    def save_key_material(self, identity: str, key_version: int, material: KeyMaterial) -> None:
        self._conn.execute(
            "INSERT OR REPLACE INTO key_material (identity, key_version, nonce, ciphertext)"
            " VALUES (?, ?, ?, ?)",
            (identity, key_version, material.nonce, material.ciphertext),
        )

    def load_key_material(self, identity: str, key_version: int) -> KeyMaterial | None:
        row = self._conn.execute(
            "SELECT nonce, ciphertext FROM key_material WHERE identity = ? AND key_version = ?",
            (identity, key_version),
        ).fetchone()
        return KeyMaterial(row[0], row[1]) if row else None

    @contextmanager
    def _atomic(self) -> Iterator[None]:
        """Run inside the caller's transaction if there is one, or in a short one of its own."""
        if self._conn.in_transaction:
            yield
        else:
            with self.transaction():
                yield
