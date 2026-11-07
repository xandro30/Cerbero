import json
import time
from dataclasses import dataclass
from typing import Any

from hecate.domain.sealing.errors import HecateError, InvalidInput, RateLimited, StateConflict

MAX_LINE = 1 << 20


@dataclass(frozen=True)
class Request:
    request_id: str | None
    op: str
    args: dict[str, Any]


def parse_request(raw: bytes) -> Request:
    """Parse one JSON line; anything malformed is a `validation` error."""
    try:
        doc = json.loads(raw)
    except (ValueError, UnicodeDecodeError) as exc:
        raise InvalidInput("request is not valid JSON") from exc
    if not isinstance(doc, dict) or not isinstance(doc.get("op"), str):
        raise InvalidInput("request must be an object with a string 'op'")
    args = doc.get("args", {})
    if not isinstance(args, dict):
        raise InvalidInput("'args' must be an object")
    request_id = doc.get("request_id")
    if request_id is not None and not isinstance(request_id, str):
        raise InvalidInput("'request_id' must be a string")
    return Request(request_id=request_id, op=doc["op"], args=args)


def _meta(request_id: str | None, started: float) -> dict[str, Any]:
    return {"request_id": request_id, "duration_ms": int((time.monotonic() - started) * 1000)}


def completed(request_id: str | None, data: dict[str, Any], started: float) -> dict[str, Any]:
    return {"status": "completed", "data": data, "error": None, "meta": _meta(request_id, started)}


def failed(request_id: str | None, exc: Exception, started: float) -> dict[str, Any]:
    """Translate an exception into the common envelope; unexpected errors become `internal`."""
    data: dict[str, Any] = {}
    if isinstance(exc, HecateError):
        error: dict[str, Any] = {"code": exc.code, "message": str(exc), "retryable": exc.retryable}
        if isinstance(exc, RateLimited):
            error["retry_after_s"] = exc.retry_after_s
        if isinstance(exc, StateConflict):
            data = {"state": exc.state.value}
    else:
        error = {"code": "internal", "message": "internal error", "retryable": True}
    return {"status": "failed", "data": data, "error": error, "meta": _meta(request_id, started)}


def encode(envelope: dict[str, Any]) -> bytes:
    return json.dumps(envelope, separators=(",", ":")).encode() + b"\n"
