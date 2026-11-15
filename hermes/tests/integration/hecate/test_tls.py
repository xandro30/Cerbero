"""The TLS checks of Hermes towards Hécate (T055, research R4), against a TLS server of the test and the test CA.

After the handshake and before sending anything, Hermes requires the certificate of Hécate to have the SPIFFE ID
`spiffe://<td>/hecate` and to be out of the CRL; in the enrollment, the root CA must have the fingerprint of the token.
"""

from __future__ import annotations

import asyncio
import ssl
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from pathlib import Path

import httpx
import pytest

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId
from hermes.infrastructure.hecate.tls import (
    HecateTransport,
    hecate_check,
    hecate_ssl_context,
    presented_chain,
    verify_hecate,
)
from support.pki import Leaf, TestCa, spiffe
from support.xfcc import pem

pytestmark = pytest.mark.anyio

HECATE = SpiffeId.parse(spiffe("hecate"))


@dataclass
class Server:
    port: int
    received: list[bytes] = field(default_factory=list)


@dataclass
class Rig:
    ca: TestCa
    workdir: Path
    servers: list[asyncio.Server] = field(default_factory=list)

    async def serve(self, leaf: Leaf) -> Server:
        chain = self.workdir / f"{leaf.serial}.crt"
        key = self.workdir / f"{leaf.serial}.key"
        chain.write_text(pem(leaf.certificate) + self.ca.intermediate_pem + self.ca.root_pem)
        key.write_text(leaf.key_pem)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_3
        context.load_cert_chain(chain, key)
        server = Server(port=0)

        async def handle(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
            data = b""
            try:
                while b"\r\n\r\n" not in data:
                    chunk = await asyncio.wait_for(reader.read(65536), timeout=5)
                    if not chunk:
                        break
                    data += chunk
                if data:
                    writer.write(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                    await writer.drain()
            except ConnectionError, ssl.SSLError, TimeoutError:
                pass
            finally:
                server.received.append(data)
                writer.close()

        listening = await asyncio.start_server(handle, "127.0.0.1", 0, ssl=context)
        self.servers.append(listening)
        server.port = listening.sockets[0].getsockname()[1]
        return server


@pytest.fixture
async def rig(tmp_path: Path) -> AsyncIterator[Rig]:
    created = Rig(ca=TestCa(), workdir=tmp_path)
    yield created
    for server in created.servers:
        server.close()
        await server.wait_closed()


def client(rig: Rig, revoked: Callable[[], list[int]] = list) -> httpx.AsyncClient:
    check = hecate_check(HECATE, lambda: [rig.ca.crl(revoked())])
    transport = HecateTransport(hecate_ssl_context(rig.ca.root_pem), check)
    return httpx.AsyncClient(transport=transport, timeout=5)


async def settled(server: Server) -> list[bytes]:
    for _ in range(50):
        if server.received:
            break
        await asyncio.sleep(0.05)
    return server.received


async def test_with_the_certificate_of_hecate_the_request_goes_out(rig: Rig) -> None:
    server = await rig.serve(rig.ca.issue("hecate"))
    async with client(rig) as http:
        response = await http.get(f"https://127.0.0.1:{server.port}/v1/keys")
    assert response.status_code == 200
    [received] = await settled(server)
    assert received.startswith(b"GET /v1/keys HTTP/1.1")


async def test_with_the_certificate_of_another_identity_nothing_is_sent(rig: Rig) -> None:
    server = await rig.serve(rig.ca.issue("otro"))
    async with client(rig) as http:
        with pytest.raises(httpx.ConnectError):
            await http.get(f"https://127.0.0.1:{server.port}/v1/keys")
    assert await settled(server) == [b""]


async def test_with_a_revoked_certificate_of_hecate_nothing_is_sent(rig: Rig) -> None:
    leaf = rig.ca.issue("hecate")
    server = await rig.serve(leaf)
    async with client(rig, revoked=lambda: [leaf.serial]) as http:
        with pytest.raises(httpx.ConnectError):
            await http.get(f"https://127.0.0.1:{server.port}/v1/keys")
    assert await settled(server) == [b""]


async def test_a_certificate_of_another_ca_fails_the_handshake(rig: Rig) -> None:
    other = TestCa()
    stranger = Rig(ca=other, workdir=rig.workdir)
    server = await stranger.serve(other.issue("hecate"))
    rig.servers.extend(stranger.servers)
    async with client(rig) as http:
        with pytest.raises(httpx.ConnectError):
            await http.get(f"https://127.0.0.1:{server.port}/v1/keys")


async def test_the_enrollment_takes_the_root_with_the_fingerprint_of_the_token(rig: Rig) -> None:
    server = await rig.serve(rig.ca.issue("hecate"))
    chain = await presented_chain("127.0.0.1", server.port)
    verified = verify_hecate(chain, rig.ca.root_fingerprint)
    assert verified.root == rig.ca.root
    assert verified.spiffe_id == HECATE
    assert await settled(server) == [b""]


async def test_a_root_without_the_fingerprint_of_the_token_stops_before_sending_anything(rig: Rig) -> None:
    server = await rig.serve(rig.ca.issue("hecate"))
    chain = await presented_chain("127.0.0.1", server.port)
    with pytest.raises(HermesError) as raised:
        verify_hecate(chain, TestCa().root_fingerprint)
    assert raised.value.code is ErrorCode.AUTH
    assert await settled(server) == [b""]


async def test_in_the_enrollment_a_server_that_is_not_hecate_is_refused(rig: Rig) -> None:
    server = await rig.serve(rig.ca.issue("otro"))
    chain = await presented_chain("127.0.0.1", server.port)
    with pytest.raises(HermesError) as raised:
        verify_hecate(chain, rig.ca.root_fingerprint)
    assert raised.value.code is ErrorCode.AUTH
