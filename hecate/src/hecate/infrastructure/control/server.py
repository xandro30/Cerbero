import asyncio
import logging
import os
import socket
import struct
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

from hecate.application.sealing.service import SealingService
from hecate.domain.sealing.access_log import Method, Operation
from hecate.domain.sealing.errors import InvalidInput
from hecate.infrastructure.control import protocol

log = logging.getLogger("hecate.control")


class _ArgError(InvalidInput):
    """Malformed arguments of a recognized operation: refused before reaching the service."""


# Recognized operation -> (access-log operation, detail) for the rows of refused requests.
_AUDITED: dict[str, tuple[Operation, str]] = {
    "init.begin": (Operation.INIT, "begin"),
    "init.confirm": (Operation.INIT, "confirm"),
    "init.abort": (Operation.INIT, "abort"),
    "unseal": (Operation.UNSEAL, "unseal"),
    "seal": (Operation.SEAL, "seal"),
    "passphrase.change": (Operation.PASSPHRASE_CHANGE, "change"),
    "shares.rekey.begin": (Operation.SHARES_REKEY, "begin"),
    "shares.rekey.confirm": (Operation.SHARES_REKEY, "confirm"),
    "shares.rekey.abort": (Operation.SHARES_REKEY, "abort"),
}


def _text(args: dict[str, Any], key: str) -> str:
    value = args.get(key)
    if not isinstance(value, str):
        raise _ArgError(f"'{key}' must be a string")
    return value


def _texts(args: dict[str, Any], key: str) -> list[str]:
    value = args.get(key)
    if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
        raise _ArgError(f"'{key}' must be a list of strings")
    return value


class RequestHandler:
    """Maps protocol operations to the sealing service; synchronous and socket-free."""

    def __init__(self, service: SealingService) -> None:
        self._service = service
        s = self._service
        self._ops: dict[str, Callable[[dict[str, Any]], dict[str, Any]]] = {
            "status": lambda a: s.status(),
            "init.begin": lambda a: s.init_begin(_text(a, "passphrase")),
            "init.confirm": lambda a: s.init_confirm(_text(a, "init_id"), _texts(a, "shares")),
            "init.abort": lambda a: s.init_abort(_text(a, "init_id")),
            "unseal": self._unseal,
            "seal": lambda a: s.seal(),
            "passphrase.change": self._passphrase_change,
            "shares.rekey.begin": self._rekey_begin,
            "shares.rekey.confirm": lambda a: s.rekey_confirm(_text(a, "rekey_id"), _texts(a, "shares")),
            "shares.rekey.abort": lambda a: s.rekey_abort(_text(a, "rekey_id")),
        }

    def _unseal(self, args: dict[str, Any]) -> dict[str, Any]:
        if "passphrase" in args and "shares" not in args:
            return self._service.unseal_with_passphrase(_text(args, "passphrase"))
        if "shares" in args and "passphrase" not in args:
            return self._service.unseal_with_shares(_texts(args, "shares"))
        raise _ArgError("'unseal' needs either 'passphrase' or 'shares'")

    def _passphrase_change(self, args: dict[str, Any]) -> dict[str, Any]:
        proof = args.get("proof")
        if not isinstance(proof, dict):
            raise _ArgError("'proof' must be an object")
        return self._service.change_passphrase(proof, _text(args, "new_passphrase"))

    def _rekey_begin(self, args: dict[str, Any]) -> dict[str, Any]:
        proof = args.get("proof")
        if not isinstance(proof, dict):
            raise _ArgError("'proof' must be an object")
        return self._service.rekey_begin(proof)

    def _record_refused(self, op: str, args: dict[str, Any]) -> None:
        operation, detail = _AUDITED[op]
        proof = args.get("proof") if op in ("passphrase.change", "shares.rekey.begin") else args
        method = SealingService.proof_method(proof) if isinstance(proof, dict) else Method.LOCAL
        self._service.record_invalid_request(operation, method, detail)

    def handle(self, raw: bytes) -> dict[str, Any]:
        started = time.monotonic()
        request_id: str | None = None
        op = "?"
        args: dict[str, Any] = {}
        try:
            request = protocol.parse_request(raw)
            request_id, op, args = request.request_id, request.op, request.args
            handler = self._ops.get(request.op)
            if handler is None:
                raise InvalidInput(f"unknown operation '{request.op}'")
            data = handler(request.args)
            envelope = protocol.completed(request_id, data, started)
        except _ArgError as exc:
            self._record_refused(op, args)
            envelope = protocol.failed(request_id, exc, started)
        except Exception as exc:  # noqa: BLE001 - every failure becomes an envelope
            envelope = protocol.failed(request_id, exc, started)
            if envelope["error"]["code"] == "internal":
                log.exception("unexpected error", extra={"op": op})
        log.info("request", extra={"op": op, "result": envelope["error"]["code"] if envelope["error"] else "ok"})
        return envelope


class ControlServer:
    """Unix socket server: one JSON request per line, operations served one at a time."""

    def __init__(self, handler: RequestHandler, socket_path: Path) -> None:
        self._handler = handler
        self._path = Path(socket_path)
        self._lock = asyncio.Lock()
        self._server: asyncio.AbstractServer | None = None

    async def start(self) -> None:
        self._remove_stale_socket()
        previous = os.umask(0o177)  # the socket is born 0600, with no window for others
        try:
            self._server = await asyncio.start_unix_server(
                self._client, path=str(self._path), limit=protocol.MAX_LINE
            )
        finally:
            os.umask(previous)
        os.chmod(self._path, 0o600)

    async def stop(self) -> None:
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()
            self._server = None
        self._path.unlink(missing_ok=True)

    def _remove_stale_socket(self) -> None:
        if not self._path.exists():
            return
        with socket.socket(socket.AF_UNIX) as probe:
            try:
                probe.connect(str(self._path))
            except OSError:
                self._path.unlink()
                return
        raise RuntimeError(f"another Hécate is already listening on {self._path}")

    @staticmethod
    def _peer_is_same_user(writer: asyncio.StreamWriter) -> bool:
        if not sys.platform.startswith("linux"):
            return True  # the 0600 socket in a 0700 directory is the barrier elsewhere
        sock = writer.get_extra_info("socket")
        creds = sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i"))
        _pid, uid, _gid = struct.unpack("3i", creds)
        return uid == os.geteuid()

    async def _client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        try:
            if not self._peer_is_same_user(writer):
                log.warning("connection from another user rejected")
                return
            while True:
                try:
                    line = await reader.readline()
                except (asyncio.LimitOverrunError, ValueError):
                    writer.write(protocol.encode(protocol.failed(None, InvalidInput("request too large"), time.monotonic())))
                    await writer.drain()
                    return
                if not line:
                    return
                async with self._lock:
                    envelope = await asyncio.to_thread(self._handler.handle, line)
                writer.write(protocol.encode(envelope))
                await writer.drain()
        except ConnectionError:
            pass
        finally:
            writer.close()
