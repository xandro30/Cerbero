"""The row of a key of idempotency (data-model.md#idempotency_keys): in progress until the provider answers, then
completed with the response that its repetitions give back."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from datetime import timedelta
from enum import StrEnum
from typing import Any

RETENTION = timedelta(hours=24)


class KeyState(StrEnum):
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"


@dataclass(frozen=True)
class StoredResponse:
    """The response of a request, as its repetitions give it back: its HTTP status and its envelope."""

    http_status: int
    body: Mapping[str, Any]


@dataclass(frozen=True)
class KeyRecord:
    fingerprint: bytes
    state: KeyState
    response: StoredResponse | None
