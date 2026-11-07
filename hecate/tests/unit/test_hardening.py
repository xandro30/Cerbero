import asyncio
import logging
import os
import socket
import struct

import pytest

from hecate.infrastructure import process
from hecate.infrastructure.control import server as control_server
from hecate.infrastructure.control.server import ControlServer


class FakeSock:
    def __init__(self, uid: int) -> None:
        self._uid = uid

    def getsockopt(self, level, option, size):
        return struct.pack("3i", 1234, self._uid, 20)


class FakeWriter:
    def __init__(self, uid: int) -> None:
        self._sock = FakeSock(uid)

    def get_extra_info(self, name):
        return self._sock


@pytest.fixture
def as_linux(monkeypatch):
    monkeypatch.setattr(control_server.sys, "platform", "linux")
    monkeypatch.setattr(socket, "SO_PEERCRED", 17, raising=False)


def test_connections_from_the_same_user_are_accepted_on_linux(as_linux):
    assert ControlServer._peer_is_same_user(FakeWriter(os.geteuid())) is True


def test_connections_from_another_user_are_rejected_on_linux(as_linux):
    assert ControlServer._peer_is_same_user(FakeWriter(os.geteuid() + 1)) is False


def test_other_platforms_rely_on_the_private_socket_permissions(monkeypatch):
    monkeypatch.setattr(control_server.sys, "platform", "darwin")
    assert ControlServer._peer_is_same_user(FakeWriter(os.geteuid() + 1)) is True


def test_a_rejected_peer_is_closed_without_reading_or_handling(as_linux):
    class Reader:
        async def readline(self):
            raise AssertionError("must not read from a rejected peer")

    class Writer(FakeWriter):
        closed = False

        def close(self):
            self.closed = True

    class Handler:
        def handle(self, raw):
            raise AssertionError("must not handle")

    writer = Writer(os.geteuid() + 1)
    asyncio.run(ControlServer(Handler(), "/nonexistent")._client(Reader(), writer))
    assert writer.closed


def test_core_dumps_are_disabled(monkeypatch):
    calls = []
    monkeypatch.setattr(process.resource, "setrlimit", lambda *a: calls.append(a))
    monkeypatch.setattr(process.sys, "platform", "darwin")
    process.harden_process()
    assert calls == [(process.resource.RLIMIT_CORE, (0, 0))]


def test_linux_locks_memory_and_warns_when_it_cannot(monkeypatch, caplog):
    monkeypatch.setattr(process.resource, "setrlimit", lambda *a: None)
    monkeypatch.setattr(process.sys, "platform", "linux")
    seen = []

    class Libc:
        def __init__(self, result):
            self.result = result

        def mlockall(self, flags):
            seen.append(flags)
            return self.result

    monkeypatch.setattr(process.ctypes, "CDLL", lambda *a, **k: Libc(0))
    with caplog.at_level(logging.WARNING, logger="hecate.process"):
        process.harden_process()
    assert seen == [3] and not caplog.records

    monkeypatch.setattr(process.ctypes, "CDLL", lambda *a, **k: Libc(-1))
    with caplog.at_level(logging.WARNING, logger="hecate.process"):
        process.harden_process()
    assert any("mlockall failed" in r.getMessage() for r in caplog.records)


def test_macos_warns_that_memory_is_not_locked(monkeypatch, caplog):
    monkeypatch.setattr(process.resource, "setrlimit", lambda *a: None)
    monkeypatch.setattr(process.sys, "platform", "darwin")
    with caplog.at_level(logging.WARNING, logger="hecate.process"):
        process.harden_process()
    assert any("only available on Linux" in r.getMessage() for r in caplog.records)
