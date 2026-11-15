"""Structured logs: one JSON line per event to standard output, with secrets and personal data masked (FR-030).

Each line that a request leaves carries its `request_id` (research R19): the middleware binds it for the request, and
every record created meanwhile takes it, also in the application layer. A task that outlives its request starts
outside it, so its lines do not carry it.
"""

from __future__ import annotations

import contextlib
import contextvars
import json
import logging
import re
import sys
from collections.abc import Callable, Iterator, Mapping
from datetime import UTC, datetime
from typing import IO, Any

from hermes.domain.shared.secret import SecretValue

REQUEST_ID: contextvars.ContextVar[str | None] = contextvars.ContextVar("hermes_request_id", default=None)
STAMP = "hermes_request_id"
MASK = "***"
SENSITIVE_KEYS = frozenset(
    {
        "address",
        "authorization",
        "bot_token",
        "cert",
        "certificate",
        "chain",
        "chat",
        "chat_id",
        "code",
        "csr",
        "key",
        "password",
        "private_key",
        "secret",
        "token",
        "verification_code",
        "xfcc",
    }
)
RESERVED = frozenset({"ts", "level", "logger", "event", "exception"})
_PATTERNS = (
    (re.compile(r"-----BEGIN [A-Z0-9 ]+-----.*?-----END [A-Z0-9 ]+-----", re.DOTALL), MASK),
    (re.compile(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*"), MASK),
    (re.compile(r"(api\.telegram\.org/bot)[^/\s\"']+"), r"\1***"),
)


def scrub(value: Any) -> Any:
    """The value with anything secret masked: secret values, sensitive keys, tokens, PEM blocks and bot URLs."""
    if isinstance(value, SecretValue):
        return MASK
    if isinstance(value, str):
        for pattern, replacement in _PATTERNS:
            value = pattern.sub(replacement, value)
        return value
    if isinstance(value, Mapping):
        return {str(k): MASK if str(k).lower() in SENSITIVE_KEYS else scrub(v) for k, v in value.items()}
    if isinstance(value, list | tuple | set | frozenset):
        return [scrub(item) for item in value]
    if value is None or isinstance(value, bool | int | float):
        return value
    return scrub(str(value))


class _RequestIdFactory:
    """Creates each record with the `request_id` of the request it is created in, if any."""

    def __init__(self, base: Callable[..., logging.LogRecord]) -> None:
        self._base = base

    def __call__(self, *args: Any, **kwargs: Any) -> logging.LogRecord:
        record = self._base(*args, **kwargs)
        setattr(record, STAMP, REQUEST_ID.get())
        return record


def stamp_request_ids() -> None:
    """Makes every record take the `request_id` of its request; installing it again changes nothing."""
    factory = logging.getLogRecordFactory()
    if not isinstance(factory, _RequestIdFactory):
        logging.setLogRecordFactory(_RequestIdFactory(factory))


@contextlib.contextmanager
def request_logged_as(request_id: str) -> Iterator[None]:
    """The lines created inside carry `request_id`."""
    token = REQUEST_ID.set(request_id)
    try:
        yield
    finally:
        REQUEST_ID.reset(token)


def outside_requests() -> contextvars.Context:
    """The context for a task that outlives the request that starts it: the current one without its `request_id`."""
    context = contextvars.copy_context()
    context.run(REQUEST_ID.set, None)
    return context


class JsonFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        line: dict[str, Any] = {
            "ts": datetime.fromtimestamp(record.created, UTC).isoformat(timespec="microseconds"),
            "level": record.levelname.lower(),
            "logger": record.name,
            "event": scrub(record.getMessage()),
        }
        request_id = getattr(record, STAMP, None)
        if request_id is not None:
            line["request_id"] = scrub(request_id)
        fields = getattr(record, "fields", None)
        if isinstance(fields, Mapping):
            for key, value in fields.items():
                name = f"field_{key}" if key in RESERVED else str(key)
                line[name] = MASK if name.lower() in SENSITIVE_KEYS else scrub(value)
        if record.exc_info and record.exc_info[0] is not None:
            line["exception"] = record.exc_info[0].__name__
        return json.dumps(line, ensure_ascii=False, default=str)


def configure_logging(level: str, stream: IO[str] | None = None) -> None:
    """Every log of the process to `stream` (standard output by default) as JSON lines."""
    stamp_request_ids()
    handler = logging.StreamHandler(stream or sys.stdout)
    handler.setFormatter(JsonFormatter())
    root = logging.getLogger()
    for existing in list(root.handlers):
        root.removeHandler(existing)
    root.addHandler(handler)
    root.setLevel(logging.WARNING)
    logging.getLogger("hermes").setLevel(level.upper())
    for name in ("httpx", "httpcore"):
        logging.getLogger(name).setLevel(logging.WARNING)
    for name in ("uvicorn", "uvicorn.error", "uvicorn.access"):
        logger = logging.getLogger(name)
        logger.handlers.clear()
        logger.propagate = True
        logger.setLevel(level.upper())
