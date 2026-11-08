import logging
from pathlib import Path

from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.client import ControlClient
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from hecate.infrastructure.logs import JsonFormatter
from tests.fakes import GOOD_PASSPHRASE, ServerThread, SqliteStack, short_dir

PATH = "secret://hermes/telegram/bot_token"
VALUES = [f"LEAK-SENTINEL-{n}-0123456789abcdef" for n in (1, 2, 3)]
PEM_BODY = "QkVHSU4tU0VOVElORUwtUEVNLUJPRFk="
PEM = f"-----BEGIN CERTIFICATE-----\n{PEM_BODY}\n-----END CERTIFICATE-----\n"


def variants(value: str) -> list[bytes]:
    raw = value.encode()
    return [raw, raw.hex().encode(), raw.hex().upper().encode()]


def leaked(blob: bytes, values: list[str]) -> list[bytes]:
    return [v for value in values for v in variants(value) if v in blob]


class Capture(logging.Handler):
    def __init__(self) -> None:
        super().__init__()
        self.setFormatter(JsonFormatter())
        self.lines: list[str] = []

    def emit(self, record: logging.LogRecord) -> None:
        self.lines.append(self.format(record))


def test_the_check_itself_detects_a_planted_value():
    assert leaked(b"..." + VALUES[0].encode() + b"...", VALUES)
    assert leaked(VALUES[1].encode().hex().encode(), VALUES)
    assert not leaked(b"nothing to see", VALUES)


def test_no_value_reaches_the_database_the_events_the_log_or_the_replies():
    capture = Capture()
    logger = logging.getLogger("hecate")
    logger.addHandler(capture)
    logger.setLevel(logging.INFO)
    replies: list[str] = []
    try:
        with short_dir() as base:
            config = HecateConfig.for_dir(base / "data")
            config.prepare()
            stack = SqliteStack(config.db_path)
            server = ControlServer(RequestHandler(stack.sealing, stack.service), config.socket_path)
            with ServerThread(server):
                client = ControlClient(config.socket_path)

                def send(op: str, args: dict) -> dict:
                    reply = client.request(op, args)
                    replies.append(repr(reply))
                    return reply

                begun = send("init.begin", {"passphrase": GOOD_PASSPHRASE})["data"]
                send("init.confirm", {"init_id": begun["init_id"], "shares": begun["shares"][:2]})
                send("unseal", {"passphrase": GOOD_PASSPHRASE})

                send("secrets.create", {"path": PATH, "value": VALUES[0], "value_type": "string"})
                send("secrets.create", {"path": "secret://hermes/certs/proxy", "value": PEM,
                                        "value_type": "pem"})
                send("secrets.add_version", {"path": PATH, "value": VALUES[1], "expected_version": 2})
                send("secrets.add_version", {"path": PATH, "value": VALUES[2], "expected_version": 4})
                send("secrets.list", {})
                send("secrets.get", {"path": PATH})
                send("secrets.revoke", {"path": PATH, "version": 1, "expected_version": 6})
                send("secrets.destroy", {"path": PATH, "version": 1, "expected_version": 7,
                                         "proof": {"passphrase": GOOD_PASSPHRASE}})
                send("secrets.destroy", {"path": PATH, "version": 2, "expected_version": 8,
                                         "proof": {"passphrase": VALUES[0]}})
                send("secrets.create", {"path": PATH, "value": VALUES[0], "value_type": "string"})
                send("secrets.add_version", {"path": PATH, "value": "", "expected_version": VALUES[0]})
                send("secrets.create", {"path": VALUES[0], "value": 5})
                send("status", {})

            # The resolution has no channel yet: it is a use case, exercised directly.
            resolved = stack.service.resolve("hermes", [PATH, "secret://hermes/certs/proxy"])["secrets"]
            assert resolved[0]["value"] == VALUES[2] and resolved[1]["value"] == PEM
            stack.service.revoke(PATH, 3, 8)

            events = [r for stream in stack.store.streams("") for r in stack.store.load(stream)]
            log_rows = stack.access_log.entries()
            stack.close()

            database = b"".join(
                f.read_bytes() for f in sorted(Path(config.data_dir).rglob("*")) if f.is_file()
                and f.suffix != ".sock"
            )
            all_values = [*VALUES, PEM, PEM_BODY]
            assert leaked(database, all_values) == [], "a value is in the database files"
            assert leaked(repr(events).encode(), all_values) == [], "a value is in an event"
            assert leaked(repr(log_rows).encode(), all_values) == [], "a value is in the access log"
            assert leaked("\n".join(replies).encode(), all_values) == [], "a value is in a reply"
            assert leaked("\n".join(capture.lines).encode(), all_values) == [], "a value is in the logs"
            assert len(log_rows) > 10 and capture.lines
    finally:
        logger.removeHandler(capture)


def test_fingerprints_stay_out_of_the_log_and_events_carry_no_value_or_key_field():
    with short_dir() as base:
        stack = SqliteStack(base / "hecate.db")
        try:
            from tests.fakes import initialize

            initialize(stack.sealing)
            stack.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
            out = stack.service.create(PATH, VALUES[0], "string")
            fingerprint = out["fingerprint"]
            stack.service.get(PATH)
            rows = repr(stack.access_log.entries())
            assert fingerprint not in rows and fingerprint.replace("-", "") not in rows
            events = [r for stream in stack.store.streams("") for r in stack.store.load(stream)]
            fields = {k for r in events for k in r.data}
            assert not fields & {"value", "key", "dek", "secret", "material", "ciphertext", "nonce"}
            assert all(isinstance(v, (str, int, type(None))) for r in events for v in r.data.values())
        finally:
            stack.close()
