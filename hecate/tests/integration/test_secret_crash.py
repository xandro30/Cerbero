import pytest

from hecate.application.ports import KeyMaterial
from tests.fakes import GOOD_PASSPHRASE, SqliteStack, initialize

PATH = "secret://hermes/telegram/bot_token"
PROOF = {"passphrase": GOOD_PASSPHRASE}


class Crash(RuntimeError):
    """Stands for the process dying in the middle of a change."""


def boom(*_a, **_k):
    raise Crash("power cut")


def ready(db):
    stack = SqliteStack(db)
    initialize(stack.sealing)
    stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
    return stack


def reopen(db):
    stack = SqliteStack(db)
    stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
    return stack


def snapshot(stack, path=PATH):
    return (
        stack.store.load(f"secret/{path.removeprefix('secret://')}"),
        stack.store.load_secret_material(path, 1),
        stack.store.load_secret_material(path, 2),
        stack.store.load("key/hermes"),
    )


def test_a_crash_while_creating_leaves_no_secret_and_no_key(tmp_path):
    db = tmp_path / "hecate.db"
    stack = ready(db)
    stack.store.save_secret_material = boom  # after the events and the key were appended
    with pytest.raises(Crash):
        stack.service.create(PATH, "value-one", "string")
    stack.close()

    again = reopen(db)
    try:
        assert snapshot(again) == ([], None, None, [])
        assert again.store.load_key_material("hermes", 1) is None
        assert again.service.list_secrets()["secrets"] == []
        assert again.service.create(PATH, "value-one", "string")["version"] == 1
    finally:
        again.close()


def test_a_crash_while_adding_a_version_keeps_the_previous_state(tmp_path):
    db = tmp_path / "hecate.db"
    stack = ready(db)
    stack.service.create(PATH, "value-one", "string")
    before = snapshot(stack)
    stack.store.save_secret_material = boom
    with pytest.raises(Crash):
        stack.service.add_version(PATH, "value-two", 2)
    stack.close()

    again = reopen(db)
    try:
        assert snapshot(again) == before
        assert again.service.resolve("hermes", [PATH])["secrets"][0]["value"] == "value-one"
        assert again.service.add_version(PATH, "value-two", 2)["version"] == 2
    finally:
        again.close()


def test_a_crash_while_destroying_keeps_the_version_revoked_with_its_material(tmp_path):
    db = tmp_path / "hecate.db"
    stack = ready(db)
    stack.service.create(PATH, "value-one", "string")
    stack.service.add_version(PATH, "value-two", 2)
    stack.service.revoke(PATH, 1, 4)
    before = snapshot(stack)
    stack.store.delete_secret_material = boom  # after VersionDestroyed was appended
    with pytest.raises(Crash):
        stack.service.destroy(PATH, 1, 5, PROOF)
    stack.close()

    again = reopen(db)
    try:
        assert snapshot(again) == before
        assert {v["version"]: v["state"] for v in again.service.get(PATH)["versions"]}[1] == "revoked"
        again.service.destroy(PATH, 1, 5, PROOF)
        assert again.store.load_secret_material(PATH, 1) is None
    finally:
        again.close()


def test_a_connection_that_dies_without_committing_changes_nothing(tmp_path):
    db = tmp_path / "hecate.db"
    stack = ready(db)
    stack.service.create(PATH, "value-one", "string")
    before = snapshot(stack)
    # Begin a change by hand and drop the connection before COMMIT, like a killed process.
    stack.store._conn.execute("BEGIN IMMEDIATE")
    stack.store.save_key_material("echidna", 1, KeyMaterial(b"n" * 12, b"wrapped"))
    stack.store.save_secret_material(PATH, 9, stack.store.load_secret_material(PATH, 1))
    stack.store._conn.close()
    stack.sealing.shutdown()
    stack.repo.close()
    stack.access_log.close()

    again = reopen(db)
    try:
        assert snapshot(again) == before
        assert again.store.load_key_material("echidna", 1) is None
        assert again.store.load_secret_material(PATH, 9) is None
    finally:
        again.close()
