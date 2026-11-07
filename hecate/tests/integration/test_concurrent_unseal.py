import threading

from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.client import ControlClient
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from tests.fakes import GOOD_PASSPHRASE, ServerThread, initialize, make_service, short_dir


def test_two_simultaneous_unseals_one_wins_and_the_other_gets_conflict():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        service = make_service()
        initialize(service)
        server = ControlServer(RequestHandler(service), config.socket_path)
        replies: list[dict] = []
        barrier = threading.Barrier(2)

        def attempt() -> None:
            client = ControlClient(config.socket_path)
            barrier.wait()
            replies.append(client.request("unseal", {"passphrase": GOOD_PASSPHRASE}))

        with ServerThread(server):
            threads = [threading.Thread(target=attempt) for _ in range(2)]
            for t in threads:
                t.start()
            for t in threads:
                t.join(30)

        assert sorted(r["status"] for r in replies) == ["completed", "failed"]
        loser = next(r for r in replies if r["status"] == "failed")
        assert loser["error"]["code"] == "conflict"
        assert loser["data"] == {"state": "unsealed"}
        assert service.status()["state"] == "unsealed"
