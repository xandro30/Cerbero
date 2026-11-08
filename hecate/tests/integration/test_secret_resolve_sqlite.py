import time

import pytest

from hecate.domain.sealing.errors import NotFound, StateConflict
from tests.fakes import GOOD_PASSPHRASE, SqliteStack, initialize

VALUES = {f"secret://hermes/service/key_{n}": f"value-number-{n}-xyz" for n in range(10)}


@pytest.fixture
def db(tmp_path):
    return tmp_path / "hecate.db"


def test_store_restart_unseal_and_resolve(db):
    first = SqliteStack(db)
    initialize(first.sealing)
    first.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
    for path, value in VALUES.items():
        first.service.create(path, value, "string")
    first.service.create("secret://echidna/key", "echidna-value", "string")
    first.close()

    second = SqliteStack(db)  # a new process: it starts sealed
    try:
        with pytest.raises(StateConflict):
            second.service.resolve("hermes", list(VALUES))
        second.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)

        out = second.service.resolve("hermes", list(VALUES))["secrets"]
        assert {i["path"]: i["value"] for i in out} == VALUES
        with pytest.raises(NotFound):
            second.service.resolve("hermes", ["secret://echidna/key"])
        (other,) = second.service.resolve("echidna", ["secret://echidna/key"])["secrets"]
        assert other["value"] == "echidna-value"
    finally:
        second.close()


def test_resolving_ten_references_takes_less_than_a_second(db):
    stack = SqliteStack(db)
    try:
        initialize(stack.sealing)
        stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
        for path, value in VALUES.items():
            stack.service.create(path, value, "string")
        started = time.monotonic()
        out = stack.service.resolve("hermes", list(VALUES))["secrets"]
        elapsed = time.monotonic() - started
        assert len(out) == 10
        assert elapsed < 1.0, f"10 references took {elapsed:.3f}s"
    finally:
        stack.close()


def test_the_access_log_keeps_the_resolutions_across_restarts(db):
    stack = SqliteStack(db)
    try:
        initialize(stack.sealing)
        stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
        stack.service.create("secret://hermes/a", "value-a-xyz", "string")
        stack.service.resolve("hermes", ["secret://hermes/a"])
        rows = [e for e in stack.access_log.entries() if e.operation.value == "secret_resolve"]
        assert [(r.subject, r.path, r.version) for r in rows] == [("hermes", "secret://hermes/a", 1)]
    finally:
        stack.close()
