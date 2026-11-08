import sqlite3
from datetime import UTC, datetime

import pytest

from hecate.application.ports import ConcurrencyConflict, EventRecord, KeyMaterial, SecretMaterial
from hecate.infrastructure.store.sqlite import SqliteSecretStore

AT = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)
STREAM = "secret/hermes/token"


def record(n: int = 1) -> EventRecord:
    return EventRecord("VersionAdded", {"version": n, "fingerprint": "aaaa-bbbb-cccc-dddd"}, AT)


def material(tag: int = 1) -> SecretMaterial:
    return SecretMaterial(
        key_version=1, value_nonce=bytes([tag]) * 12, value_ciphertext=b"value-ct" + bytes([tag]),
        dek_nonce=bytes([tag + 1]) * 12, dek_ciphertext=b"dek-ct" + bytes([tag]),
    )


@pytest.fixture
def store(tmp_path):
    s = SqliteSecretStore(tmp_path / "hecate.db")
    yield s
    s.close()


def test_events_are_appended_with_the_expected_version(store):
    assert store.append(STREAM, [record(1), record(2)], 0) == 2
    assert store.append(STREAM, [record(3)], 2) == 3
    loaded = store.load(STREAM)
    assert [r.data["version"] for r in loaded] == [1, 2, 3]
    assert loaded[0] == record(1)


def test_a_stale_expected_version_conflicts_and_writes_nothing(store):
    store.append(STREAM, [record(1)], 0)
    with pytest.raises(ConcurrencyConflict) as exc:
        store.append(STREAM, [record(2), record(3)], 0)
    assert exc.value.current_version == 1
    with pytest.raises(ConcurrencyConflict):
        store.append(STREAM, [record(2)], 5)
    assert len(store.load(STREAM)) == 1


def test_streams_are_independent_and_listed_by_prefix(store):
    store.append("secret/hermes/token", [record()], 0)
    store.append("secret/hermes/certs/proxy", [record()], 0)
    store.append("secret/echidna/key", [record()], 0)
    store.append("key/hermes", [record()], 0)
    assert store.load("secret/none") == []
    assert store.streams("secret/hermes/") == ["secret/hermes/certs/proxy", "secret/hermes/token"]
    assert len(store.streams("secret/")) == 3
    assert store.streams("key/") == ["key/hermes"]
    assert store.streams("secret/he_mes/") == []  # `_` is not a wildcard


def test_events_and_material_survive_closing_and_reopening(tmp_path):
    path = tmp_path / "hecate.db"
    first = SqliteSecretStore(path)
    first.append(STREAM, [record(1)], 0)
    first.save_secret_material("secret://hermes/token", 1, material())
    first.save_key_material("hermes", 1, KeyMaterial(nonce=b"n" * 12, ciphertext=b"wrapped"))
    first.close()

    second = SqliteSecretStore(path)
    try:
        assert second.load(STREAM) == [record(1)]
        assert second.load_secret_material("secret://hermes/token", 1) == material()
        assert second.load_key_material("hermes", 1) == KeyMaterial(nonce=b"n" * 12, ciphertext=b"wrapped")
    finally:
        second.close()


def test_missing_material_is_none(store):
    assert store.load_secret_material("secret://hermes/token", 1) is None
    assert store.load_key_material("hermes", 1) is None


def test_deleting_the_material_of_a_version_keeps_its_events(store):
    store.append(STREAM, [record(1), record(2)], 0)
    store.save_secret_material("secret://hermes/token", 1, material(1))
    store.save_secret_material("secret://hermes/token", 2, material(2))
    store.delete_secret_material("secret://hermes/token", 1)
    assert store.load_secret_material("secret://hermes/token", 1) is None
    assert store.load_secret_material("secret://hermes/token", 2) == material(2)
    assert len(store.load(STREAM)) == 2
    store.delete_secret_material("secret://hermes/token", 1)  # deleting again is harmless


def test_a_failure_inside_the_transaction_leaves_neither_events_nor_material(store):
    with pytest.raises(RuntimeError):
        with store.transaction():
            store.append(STREAM, [record(1)], 0)
            store.save_secret_material("secret://hermes/token", 1, material())
            store.save_key_material("hermes", 1, KeyMaterial(nonce=b"n" * 12, ciphertext=b"w"))
            raise RuntimeError("boom")
    assert store.load(STREAM) == []
    assert store.load_secret_material("secret://hermes/token", 1) is None
    assert store.load_key_material("hermes", 1) is None


def test_a_committed_transaction_keeps_everything(store):
    with store.transaction():
        store.append(STREAM, [record(1)], 0)
        store.save_secret_material("secret://hermes/token", 1, material())
    assert len(store.load(STREAM)) == 1
    assert store.load_secret_material("secret://hermes/token", 1) == material()


def test_a_conflict_inside_a_transaction_rolls_back_the_rest(store):
    store.append(STREAM, [record(1)], 0)
    with pytest.raises(ConcurrencyConflict):
        with store.transaction():
            store.save_secret_material("secret://hermes/token", 9, material())
            store.append(STREAM, [record(2)], 0)
    assert store.load_secret_material("secret://hermes/token", 9) is None


def test_the_events_table_is_append_only(tmp_path):
    path = tmp_path / "hecate.db"
    store = SqliteSecretStore(path)
    store.append(STREAM, [record(1)], 0)
    store.close()
    raw = sqlite3.connect(path)
    try:
        with pytest.raises(sqlite3.DatabaseError, match="append-only"):
            raw.execute("UPDATE events SET type = 'x'")
        with pytest.raises(sqlite3.DatabaseError, match="append-only"):
            raw.execute("DELETE FROM events")
    finally:
        raw.close()
