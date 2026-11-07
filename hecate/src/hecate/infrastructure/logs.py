import json
import logging
import sys
from datetime import UTC, datetime

_STANDARD = set(logging.makeLogRecord({}).__dict__) | {"message", "asctime"}


class JsonFormatter(logging.Formatter):
    """One JSON object per line. Only explicit `extra` fields are included, never arguments."""

    def format(self, record: logging.LogRecord) -> str:
        doc = {
            "ts": datetime.now(UTC).isoformat(),
            "level": record.levelname.lower(),
            "logger": record.name,
            "event": record.getMessage(),
        }
        doc.update({k: v for k, v in record.__dict__.items() if k not in _STANDARD})
        if record.exc_info:
            doc["exception"] = record.exc_info[0].__name__  # type only: messages could echo input
        return json.dumps(doc)


def configure_logging() -> None:
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(JsonFormatter())
    root = logging.getLogger("hecate")
    root.handlers[:] = [handler]
    root.setLevel(logging.INFO)
    root.propagate = False
