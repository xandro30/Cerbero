"""The reload of the certificate in Envoy (T062, story 3 scenario 2): after a real renewal in Hécate, written as a new
generation, Envoy presents the new certificate without a restart and without cutting a connection already open."""

from __future__ import annotations

import asyncio
import socket
import ssl
import time
from pathlib import Path

import pytest
from cryptography import x509

from hermes.domain.shared.secret import SecretValue
from support.envoy import EnvoyRig, envoy_rig
from support.hecate import RunningHecate
from support.identity import hermes_for

pytestmark = [pytest.mark.anyio, pytest.mark.docker, pytest.mark.hecate]


def generation(base: Path) -> dict[str, bytes]:
    current = base / "proxy" / "current"
    return {name: (current / name).read_bytes() for name in ("tls.crt", "tls.key", "ca.crt")}


def presented_serial(sock: ssl.SSLSocket) -> int:
    der = sock.getpeercert(binary_form=True)
    assert der is not None
    return x509.load_der_x509_certificate(der).serial_number


def connect(port: int, root_pem: str) -> ssl.SSLSocket:
    context = ssl.create_default_context(cadata=root_pem)
    context.check_hostname = False
    raw = socket.create_connection(("127.0.0.1", port), timeout=10)
    return context.wrap_socket(raw)


def exchange(sock: ssl.SSLSocket) -> int:
    """One request over a kept-alive connection; the status of its answer."""
    sock.sendall(b"GET /reload HTTP/1.1\r\nHost: hermes\r\nConnection: keep-alive\r\n\r\n")
    data = b""
    while b"\r\n\r\n" not in data:
        data += sock.recv(65536)
    head, _, body = data.partition(b"\r\n\r\n")
    length = next(
        int(line.split(b":", 1)[1]) for line in head.split(b"\r\n") if line.lower().startswith(b"content-length:")
    )
    while len(body) < length:
        body += sock.recv(65536)
    return int(head.split(b" ", 2)[1])


def started_at(rig: EnvoyRig) -> tuple[str, int]:
    rig.envoy.reload()
    return rig.envoy.attrs["State"]["StartedAt"], rig.envoy.attrs["RestartCount"]


async def test_envoy_presents_the_renewed_certificate_without_a_restart_or_a_cut(
    hecate: RunningHecate, tmp_path: Path
) -> None:
    hermes = hermes_for(hecate.url, tmp_path)
    try:
        await hermes.identity.enroll(SecretValue(hecate.token()))
        first = hermes.identity.identity
        assert first is not None
        root_pem = first.certificate.chain_pem[-1]
        with envoy_rig(generation(tmp_path), root_pem) as rig:
            before = started_at(rig)
            open_connection = connect(rig.port, root_pem)
            assert presented_serial(open_connection) == int(first.certificate.serial, 16)
            assert exchange(open_connection) == 200

            await hermes.identity.renew()
            renewed = hermes.identity.identity
            assert renewed is not None
            rig.publish(2, generation(tmp_path))

            deadline = time.monotonic() + 30
            while True:
                with connect(rig.port, root_pem) as fresh:
                    if presented_serial(fresh) == int(renewed.certificate.serial, 16):
                        break
                assert time.monotonic() < deadline, rig.logs()
                await asyncio.sleep(0.5)

            assert exchange(open_connection) == 200
            assert presented_serial(open_connection) == int(first.certificate.serial, 16)
            open_connection.close()
            assert started_at(rig) == before
    finally:
        await hermes.client.aclose()
