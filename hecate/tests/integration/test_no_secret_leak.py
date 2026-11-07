import logging
import threading
from pathlib import Path

from hecate.domain.sealing.share import RecoveryShare
from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.client import ControlClient
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from hecate.infrastructure.logs import JsonFormatter
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository
from tests.fakes import GOOD_PASSPHRASE, ServerThread, make_service, short_dir

NEW = "the second passphrase"


def secret_variants(value: str | bytes) -> list[bytes]:
    raw = value.encode() if isinstance(value, str) else value
    variants = [raw, raw.hex().encode(), raw.hex().upper().encode()]
    if isinstance(value, str):
        variants.append(value.replace("-", "").encode())
    return variants


def leaks(blob: bytes, secrets: list[str | bytes]) -> list[bytes]:
    return [v for s in secrets for v in secret_variants(s) if v in blob]


def test_the_check_itself_detects_a_planted_secret():
    assert leaks(b"xx" + GOOD_PASSPHRASE.encode() + b"yy", [GOOD_PASSPHRASE])
    assert leaks(b"fragment " + (b"\xab" * 32).hex().encode(), [b"\xab" * 32])
    assert not leaks(b"nothing here", [GOOD_PASSPHRASE])


class Capture(logging.Handler):
    def __init__(self) -> None:
        super().__init__()
        self.setFormatter(JsonFormatter())
        self.lines: list[str] = []

    def emit(self, record: logging.LogRecord) -> None:
        self.lines.append(self.format(record))


def test_no_passphrase_share_or_root_reaches_the_database_the_log_or_the_replies():
    capture = Capture()
    logger = logging.getLogger("hecate")
    logger.addHandler(capture)
    logger.setLevel(logging.INFO)
    replies: list[str] = []
    try:
        with short_dir() as base:
            config = HecateConfig.for_dir(base / "data")
            config.prepare()
            repo, access_log = SqliteSealRepository(config.db_path), SqliteAccessLog(config.db_path)
            service = make_service(repo=repo, access_log=access_log)
            server = ControlServer(RequestHandler(service), config.socket_path)
            shown: list[str] = []
            with ServerThread(server):
                client = ControlClient(config.socket_path)

                def ask(op, args):
                    reply = client.request(op, args)
                    replies.append(str(reply))
                    return reply

                begun = ask("init.begin", {"passphrase": GOOD_PASSPHRASE})["data"]
                shown += begun["shares"]
                ask("init.confirm", {"init_id": begun["init_id"], "shares": begun["shares"][:2]})
                ask("unseal", {"passphrase": "a wrong passphrase!!"})
                ask("unseal", {"passphrase": GOOD_PASSPHRASE})
                root = service._root.get()  # white-box: the one place the root legitimately lives
                ask("passphrase.change", {"proof": {"passphrase": GOOD_PASSPHRASE}, "new_passphrase": NEW})
                rekey = ask("shares.rekey.begin", {"proof": {"passphrase": NEW}})["data"]
                shown += rekey["shares"]
                ask("shares.rekey.confirm", {"rekey_id": rekey["rekey_id"], "shares": rekey["shares"][:2]})
                ask("seal", {})
                ask("unseal", {"shares": rekey["shares"][1:]})
            repo.close()
            access_log.close()

            parsed = [RecoveryShare.parse(t) for t in shown]
            secrets: list[str | bytes] = [GOOD_PASSPHRASE, NEW, "a wrong passphrase!!", root]
            secrets += shown + [s.fragments for s in parsed]

            for path in Path(config.data_dir).iterdir():
                if path.is_socket():
                    continue
                assert not leaks(path.read_bytes(), secrets), f"secret found in {path.name}"

        log_blob = "\n".join(capture.lines).encode()
        assert capture.lines, "the server should have logged its requests"
        assert not leaks(log_blob, secrets), "secret found in the logs"
        # The replies carry the shares to the admin on purpose; they must never carry the passphrase or the root.
        assert not leaks("".join(replies).encode(), [GOOD_PASSPHRASE, NEW, "a wrong passphrase!!", root])
    finally:
        logger.removeHandler(capture)
