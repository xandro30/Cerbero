"""The client of the local channel that the CLI uses: HTTP over the Unix socket of `HERMES_ADMIN_DIR`."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import httpx


class AdminUnreachable(Exception):
    def __init__(self, socket: Path, reason: str) -> None:
        super().__init__(reason)
        self.socket = socket
        self.reason = reason


class AdminFailure(Exception):
    def __init__(self, code: str, message: str, details: list[dict[str, str]]) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details


class AdminClient:
    def __init__(self, socket: Path, timeout: float = 60.0) -> None:
        self.socket = socket
        self.timeout = timeout

    def call(self, method: str, path: str, body: Any = None) -> dict[str, Any]:
        """The `data` of a completed answer; a failed one raises `AdminFailure`."""
        transport = httpx.HTTPTransport(uds=str(self.socket))
        try:
            with httpx.Client(transport=transport, base_url="http://hermes", timeout=self.timeout) as client:
                response = client.request(method, path, json=body)
        except httpx.TransportError as error:
            raise AdminUnreachable(self.socket, str(error) or type(error).__name__) from None
        try:
            envelope = response.json()
        except ValueError:
            raise AdminFailure("internal", "the answer of hermes is not its envelope", []) from None
        if envelope.get("status") == "completed":
            data: dict[str, Any] = envelope.get("data", {})
            return data
        failure = envelope.get("error") or {}
        details = (envelope.get("data") or {}).get("errors", [])
        raise AdminFailure(str(failure.get("code", "internal")), str(failure.get("message", "")), details)
