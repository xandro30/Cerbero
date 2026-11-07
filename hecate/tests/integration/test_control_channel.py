import json
import os
import socket
import stat

from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.client import ControlClient
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from tests.fakes import ServerThread, make_service, short_dir


def test_socket_is_private_and_status_works_when_uninitialized():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        server = ControlServer(RequestHandler(make_service()), config.socket_path)
        with ServerThread(server):
            assert stat.S_IMODE(os.stat(config.data_dir).st_mode) == 0o700
            assert stat.S_IMODE(os.stat(config.socket_path).st_mode) == 0o600

            reply = ControlClient(config.socket_path).request("status", {})
            assert reply["status"] == "completed"
            assert reply["data"]["state"] == "uninitialized"
            assert set(reply) == {"status", "data", "error", "meta"}
            assert reply["meta"]["duration_ms"] >= 0


def test_malformed_request_gets_validation_error_in_common_envelope():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        server = ControlServer(RequestHandler(make_service()), config.socket_path)
        with ServerThread(server):
            for raw in (b"not json\n", b'{"op": 7}\n', b'{"op": "nope", "args": {}}\n'):
                with socket.socket(socket.AF_UNIX) as s:
                    s.connect(str(config.socket_path))
                    s.sendall(raw)
                    reply = json.loads(s.makefile("rb").readline())
                assert reply["status"] == "failed"
                assert reply["error"]["code"] == "validation"
                assert reply["error"]["retryable"] is False
                assert "retry_after_s" not in reply["error"]


def test_status_works_with_real_sqlite_from_worker_threads():
    """The server runs each operation in a worker thread; the SQLite adapters must allow it."""
    from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository

    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        repo = SqliteSealRepository(config.db_path)
        log = SqliteAccessLog(config.db_path)
        service = make_service(repo=repo, access_log=log)
        server = ControlServer(RequestHandler(service), config.socket_path)
        with ServerThread(server):
            client = ControlClient(config.socket_path)
            assert client.request("status", {})["data"]["state"] == "uninitialized"
            assert client.request("status", {})["status"] == "completed"
        repo.close()
        log.close()
