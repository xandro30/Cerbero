"""The `Idempotency-Key` of a request with effect and the fingerprint of the request (research R11, FR-016)."""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any

from hermes.domain.shared.errors import ErrorCode, HermesError

KEY = re.compile(r"[\x20-\x7e]{1,128}")
KEY_RULE = "Idempotency-Key must have from 1 to 128 printable ASCII characters"


def idempotency_key(value: str | None) -> str:
    """The key as it came, if it follows its rule; without it, or with another, `validation`."""
    if value is None or KEY.fullmatch(value) is None:
        raise HermesError(ErrorCode.VALIDATION, KEY_RULE, details=[{"path": "Idempotency-Key", "message": KEY_RULE}])
    return value


def canonical_json(value: Any) -> bytes:
    """The same JSON value always as the same bytes: keys in order, no spaces, UTF-8."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode()


def request_fingerprint(method: str, path: str, body: Any) -> bytes:
    """SHA-256 of the method, the path and the canonical JSON of the body, together in one JSON array so that no part
    can pass for another."""
    return hashlib.sha256(canonical_json([method, path, body])).digest()
