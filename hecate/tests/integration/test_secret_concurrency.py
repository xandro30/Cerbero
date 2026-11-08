import threading

from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.client import ControlClient
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from tests.fakes import GOOD_PASSPHRASE, ServerThread, SqliteStack, initialize, short_dir

PATH = "secret://hermes/telegram/bot_token"


def test_two_new_versions_with_the_same_expected_version_one_wins_and_the_other_conflicts():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        stack = SqliteStack(config.db_path)
        try:
            initialize(stack.sealing)
            stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
            stack.service.create(PATH, "value-one", "string")
            server = ControlServer(RequestHandler(stack.sealing, stack.service), config.socket_path)
            replies: list[dict] = []
            barrier = threading.Barrier(2)

            def attempt(value: str) -> None:
                client = ControlClient(config.socket_path)
                barrier.wait()
                replies.append(client.request(
                    "secrets.add_version", {"path": PATH, "value": value, "expected_version": 2}))

            with ServerThread(server):
                threads = [threading.Thread(target=attempt, args=(f"value-{n}",)) for n in ("a", "b")]
                for t in threads:
                    t.start()
                for t in threads:
                    t.join(30)

            assert sorted(r["status"] for r in replies) == ["completed", "failed"]
            loser = next(r for r in replies if r["status"] == "failed")
            assert loser["error"]["code"] == "conflict"
            assert loser["data"] == {"current_version": 4}
            got = stack.service.get(PATH)
            assert got["stream_version"] == 4 and [v["version"] for v in got["versions"]] == [1, 2]
            assert len(stack.store._conn.execute("SELECT * FROM secret_material").fetchall()) == 2
        finally:
            stack.close()


def test_two_destroys_of_the_same_version_one_wins():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        stack = SqliteStack(config.db_path)
        try:
            initialize(stack.sealing)
            stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
            stack.service.create(PATH, "value-one", "string")
            stack.service.revoke(PATH, 1, 2)
            server = ControlServer(RequestHandler(stack.sealing, stack.service), config.socket_path)
            replies: list[dict] = []
            barrier = threading.Barrier(2)

            def attempt() -> None:
                client = ControlClient(config.socket_path)
                barrier.wait()
                replies.append(client.request("secrets.destroy", {
                    "path": PATH, "version": 1, "expected_version": 3,
                    "proof": {"passphrase": GOOD_PASSPHRASE}}))

            with ServerThread(server):
                threads = [threading.Thread(target=attempt) for _ in range(2)]
                for t in threads:
                    t.start()
                for t in threads:
                    t.join(30)

            assert sorted(r["status"] for r in replies) == ["completed", "failed"]
            assert next(r for r in replies if r["status"] == "failed")["error"]["code"] == "conflict"
        finally:
            stack.close()
