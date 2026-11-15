"""TLS from Hermes to Hécate (research R4).

- Each call goes with an `SSLContext` of TLS 1.3 that trusts only the stored root CA and compares no DNS name, because
  the certificates of Cerbero only carry a SPIFFE ID.
- A network backend of httpcore checks, after the handshake and before the request is written, that the certificate of
  Hécate has the SPIFFE ID `spiffe://<td>/hecate` and is out of the CRL; otherwise it closes the connection.
- In the enrollment, a first connection without trust takes the chain that Hécate presents, and the root CA with the
  fingerprint of the token must verify the certificate of Hécate; only then is the enrollment sent.
"""

from __future__ import annotations

import asyncio
import contextlib
import hashlib
import ssl
import typing
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

import httpcore
import httpx
from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.x509.verification import PolicyBuilder, Store, VerificationError

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId

Check = Callable[[bytes | None], None]


def hecate_ssl_context(
    root_pem: str, *, certificate_chain: Path | None = None, key: Path | None = None
) -> ssl.SSLContext:
    context = ssl.create_default_context(ssl.Purpose.SERVER_AUTH, cadata=root_pem)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.maximum_version = ssl.TLSVersion.TLSv1_3
    context.check_hostname = False
    if certificate_chain is not None and key is not None:
        context.load_cert_chain(certificate_chain, key)
    return context


def only_spiffe_id(certificate: x509.Certificate) -> SpiffeId | None:
    """The SPIFFE ID of a certificate whose only name is one; otherwise None."""
    try:
        names = certificate.extensions.get_extension_for_class(x509.SubjectAlternativeName).value
    except x509.ExtensionNotFound:
        return None
    entries = list(names)
    if len(entries) != 1 or not isinstance(entries[0], x509.UniformResourceIdentifier):
        return None
    try:
        return SpiffeId.parse(entries[0].value)
    except HermesError:
        return None


def hecate_check(expected: SpiffeId, revocations: Callable[[], Sequence[x509.CertificateRevocationList]]) -> Check:
    def check(der: bytes | None) -> None:
        if der is None:
            raise ValueError("hecate presented no certificate")
        certificate = x509.load_der_x509_certificate(der)
        if only_spiffe_id(certificate) != expected:
            raise ValueError("the certificate is not the one of hecate")
        for crl in revocations():
            if crl.issuer == certificate.issuer and crl.get_revoked_certificate_by_serial_number(
                certificate.serial_number
            ):
                raise ValueError("the certificate of hecate is revoked")

    return check


class _CheckedStream(httpcore.AsyncNetworkStream):
    def __init__(self, inner: httpcore.AsyncNetworkStream, check: Check) -> None:
        self._inner = inner
        self._check = check

    async def read(self, max_bytes: int, timeout: float | None = None) -> bytes:  # noqa: ASYNC109 (httpcore)
        return await self._inner.read(max_bytes, timeout)

    async def write(self, buffer: bytes, timeout: float | None = None) -> None:  # noqa: ASYNC109 (httpcore)
        await self._inner.write(buffer, timeout)

    async def aclose(self) -> None:
        await self._inner.aclose()

    async def start_tls(
        self,
        ssl_context: ssl.SSLContext,
        server_hostname: str | None = None,
        timeout: float | None = None,  # noqa: ASYNC109 (the interface of httpcore)
    ) -> httpcore.AsyncNetworkStream:
        stream = await self._inner.start_tls(ssl_context, server_hostname, timeout)
        ssl_object = stream.get_extra_info("ssl_object")
        try:
            self._check(ssl_object.getpeercert(binary_form=True) if ssl_object is not None else None)
        except ValueError as error:
            await stream.aclose()
            raise httpcore.ConnectError(str(error)) from None
        return stream

    def get_extra_info(self, info: str) -> typing.Any:
        return self._inner.get_extra_info(info)


class SpiffeBackend(httpcore.AsyncNetworkBackend):
    """The network backend of httpcore with the check of the certificate of Hécate after each handshake."""

    def __init__(self, check: Check) -> None:
        self._inner = httpcore.AnyIOBackend()
        self._check = check

    async def connect_tcp(
        self,
        host: str,
        port: int,
        timeout: float | None = None,  # noqa: ASYNC109 (the interface of httpcore)
        local_address: str | None = None,
        socket_options: Iterable[httpcore.SOCKET_OPTION] | None = None,
    ) -> httpcore.AsyncNetworkStream:
        stream = await self._inner.connect_tcp(host, port, timeout, local_address, socket_options)
        return _CheckedStream(stream, self._check)

    async def connect_unix_socket(
        self,
        path: str,
        timeout: float | None = None,  # noqa: ASYNC109 (the interface of httpcore)
        socket_options: Iterable[httpcore.SOCKET_OPTION] | None = None,
    ) -> httpcore.AsyncNetworkStream:
        raise httpcore.ConnectError("hecate is reached over TCP")

    async def sleep(self, seconds: float) -> None:
        await self._inner.sleep(seconds)


class HecateTransport(httpx.AsyncHTTPTransport):
    """The transport of httpx with the backend that checks Hécate; httpx does not take a backend, so the pool is
    built here with the same parameters."""

    def __init__(self, ssl_context: ssl.SSLContext, check: Check) -> None:
        super().__init__(verify=ssl_context)
        self._pool = httpcore.AsyncConnectionPool(
            ssl_context=ssl_context, network_backend=SpiffeBackend(check), max_connections=10, keepalive_expiry=30
        )


@dataclass(frozen=True)
class VerifiedHecate:
    root: x509.Certificate
    spiffe_id: SpiffeId


def _der(certificate: x509.Certificate) -> bytes:
    return certificate.public_bytes(serialization.Encoding.DER)


async def presented_chain(host: str, port: int) -> list[x509.Certificate]:
    """The chain that the server presents to a connection without a client certificate; nothing is sent over it.

    The caller bounds it in time with `asyncio.timeout`.
    """
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    _, writer = await asyncio.open_connection(host, port, ssl=context)
    try:
        ssl_object = writer.get_extra_info("ssl_object")
        chain = ssl_object.get_unverified_chain() if ssl_object is not None else []
    finally:
        writer.close()
        with contextlib.suppress(OSError, ssl.SSLError):
            await writer.wait_closed()
    return [x509.load_der_x509_certificate(item) for item in chain or []]


def verify_hecate(chain: Sequence[x509.Certificate], fingerprint: str, now: datetime | None = None) -> VerifiedHecate:
    """The root CA with the fingerprint of the token must verify the certificate of Hécate, whose only name is
    `spiffe://<td>/hecate`; any other case is `auth`, before anything is sent."""
    root = next((item for item in chain if hashlib.sha256(_der(item)).hexdigest() == fingerprint.lower()), None)
    if root is None or not chain:
        raise HermesError(ErrorCode.AUTH, "hecate does not have the root CA of the token")
    leaf = chain[0]
    verifier = PolicyBuilder().store(Store([root])).time(now or datetime.now(UTC)).build_client_verifier()
    try:
        verifier.verify(leaf, [item for item in chain[1:] if item != root])
    except VerificationError:
        raise HermesError(
            ErrorCode.AUTH, "the certificate of hecate does not verify with the root CA of the token"
        ) from None
    identity = only_spiffe_id(leaf)
    if identity is None or identity.identity != "hecate":
        raise HermesError(ErrorCode.AUTH, "the server is not hecate")
    return VerifiedHecate(root=root, spiffe_id=identity)
