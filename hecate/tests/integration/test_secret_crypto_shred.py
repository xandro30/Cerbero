import shutil

import pytest

from hecate.domain.sealing.errors import NotFound
from tests.fakes import GOOD_PASSPHRASE, SqliteStack, initialize

PATH = "secret://hermes/telegram/bot_token"
V1, V2 = "SHRED-SENTINEL-first-0123456789", "SHRED-SENTINEL-second-0123456789"
PROOF = {"passphrase": GOOD_PASSPHRASE}


def test_a_destroyed_version_cannot_be_recovered_with_the_root_nor_from_a_later_copy(tmp_path):
    db = tmp_path / "hecate.db"
    stack = SqliteStack(db)
    initialize(stack.sealing)
    stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
    stack.service.create(PATH, V1, "string")
    stack.service.add_version(PATH, V2, 2)
    stack.service.revoke(PATH, 1, 4)

    before = stack.store.load_secret_material(PATH, 1)
    assert before is not None
    stack.service.destroy(PATH, 1, 5, PROOF)
    stack.close()

    # A copy of the database made after the destruction (the backup the admin keeps).
    copy = tmp_path / "backup.db"
    shutil.copyfile(db, copy)
    for name in (db, copy):
        raw = name.read_bytes()
        for piece in (before.value_ciphertext, before.dek_ciphertext, before.dek_nonce, before.value_nonce):
            assert piece not in raw, f"material of the destroyed version is still in {name.name}"

    restored = SqliteStack(copy)
    try:
        restored.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)  # the root is available
        assert restored.store.load_secret_material(PATH, 1) is None
        with pytest.raises(NotFound):
            restored.service.resolve("hermes", [PATH + "@1"])
        assert restored.service.resolve("hermes", [PATH])["secrets"][0]["value"] == V2
        events = [r.type for r in restored.store.load("secret/hermes/telegram/bot_token")]
        assert "VersionDestroyed" in events and events.count("VersionAdded") == 2, "the history stays"
    finally:
        restored.close()


def test_the_version_destroyed_by_the_maximum_is_erased_as_well(tmp_path):
    db = tmp_path / "hecate.db"
    stack = SqliteStack(db)
    initialize(stack.sealing)
    stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
    path = "secret://hermes/limited"
    stack.service.create(path, V1, "string", max_versions=2)
    material = stack.store.load_secret_material(path, 1)
    stack.service.add_version(path, V2, 2)
    stack.service.add_version(path, V2 + "-3", 4)
    stack.close()
    raw = db.read_bytes()
    assert material.dek_ciphertext not in raw and material.value_ciphertext not in raw


def test_the_other_versions_keep_their_material(tmp_path):
    db = tmp_path / "hecate.db"
    stack = SqliteStack(db)
    try:
        initialize(stack.sealing)
        stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
        stack.service.create(PATH, V1, "string")
        stack.service.add_version(PATH, V2, 2)
        keep = stack.store.load_secret_material(PATH, 2)
    finally:
        stack.close()
    assert keep.value_ciphertext in db.read_bytes()
