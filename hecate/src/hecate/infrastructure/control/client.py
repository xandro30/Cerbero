import json
import socket
import uuid
from pathlib import Path
from typing import Any


class ControlUnavailable(Exception):
    """Hécate is not running or its socket is not reachable."""


class ControlClient:
    def __init__(self, socket_path: Path, timeout_s: float = 600) -> None:
        self._path = str(socket_path)
        self._timeout = timeout_s

    def request(self, op: str, args: dict[str, Any]) -> dict[str, Any]:
        message = {"request_id": str(uuid.uuid4()), "op": op, "args": args}
        try:
            with socket.socket(socket.AF_UNIX) as sock:
                sock.settimeout(self._timeout)
                sock.connect(self._path)
                sock.sendall(json.dumps(message).encode() + b"\n")
                line = sock.makefile("rb").readline()
        except OSError as exc:
            raise ControlUnavailable(f"cannot reach Hécate at {self._path}") from exc
        if not line:
            raise ControlUnavailable("Hécate closed the connection without answering")
        return json.loads(line)
