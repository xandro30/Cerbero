"""The client of the API of Hécate (research R4, docs/hecate/integracion.md).

The enrollment checks Hécate with the fingerprint of the token before sending anything. Every other call goes with the
certificate of Hermes and a new short token signed with its call key, over the transport that checks Hécate's SPIFFE
ID. The failures of Hécate come back as `HermesError`s with its code; `unavailable` when it does not answer, and
`HecateTrustChanged` when it presents a root CA that Hermes does not trust.
"""

from __future__ import annotations

import asyncio
import ssl
import uuid
from collections.abc import Callable, Mapping, Sequence
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.parse import quote, urlsplit

import httpx
import jwt
from cryptography import x509
from cryptography.hazmat.primitives import serialization

from hermes.application.ports import (
    HecateTrustChanged,
    IssuedCertificate,
    PublishedKeys,
    ResolvedSecret,
    RevocationLists,
    StoredIdentity,
)
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference, SpiffeId
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import load_call_key
from hermes.infrastructure.hecate.tls import (
    HecateTransport,
    hecate_check,
    hecate_ssl_context,
    presented_chain,
    verify_hecate,
)

TOKEN_LIFETIME_S = 60
NOT_ANSWERING = "hecate does not answer"
CODES = {code.value: code for code in ErrorCode}


def _caused_by_verification(error: BaseException) -> bool:
    seen: BaseException | None = error
    while seen is not None:
        if isinstance(seen, ssl.SSLCertVerificationError):
            return True
        seen = seen.__cause__ or seen.__context__
    return False


def _issued(data: Mapping[str, Any]) -> IssuedCertificate:
    return IssuedCertificate(
        spiffe_id=SpiffeId.parse(data["identity"]),
        serial=data["serial"],
        certificate_pem=data["certificate"],
        chain_pem=tuple(data["chain"]),
        not_after=datetime.fromisoformat(data["not_after"]),
        call_key_id=data["call_key_id"],
    )


class HecateClient:
    def __init__(
        self,
        url: str,
        *,
        identity: Callable[[], StoredIdentity | None],
        files: Callable[[], tuple[Path, Path, Path] | None],
        revocations: Callable[[], Sequence[x509.CertificateRevocationList]],
        clock: Clock,
        timeout: float = 10.0,
    ) -> None:
        self._url = url.rstrip("/")
        parts = urlsplit(self._url)
        self._host, self._port = parts.hostname or "", parts.port or 443
        self._identity = identity
        self._files = files
        self._revocations = revocations
        self._clock = clock
        self._timeout = timeout
        self._current: tuple[str, httpx.AsyncClient] | None = None
        self._previous: httpx.AsyncClient | None = None

    async def aclose(self) -> None:
        for client in (self._current[1] if self._current else None, self._previous):
            if client is not None:
                await client.aclose()
        self._current = self._previous = None

    # --- Requests --------------------------------------------------------------------------------------------------

    async def _send(self, client: httpx.AsyncClient, method: str, path: str, **options: Any) -> Any:
        try:
            response = await client.request(method, path, **options)
        except httpx.ConnectError as error:
            if _caused_by_verification(error):
                raise HecateTrustChanged() from None
            raise HermesError(ErrorCode.UNAVAILABLE, NOT_ANSWERING) from None
        except httpx.HTTPError:
            raise HermesError(ErrorCode.UNAVAILABLE, NOT_ANSWERING) from None
        try:
            envelope = response.json()
        except ValueError:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate answered without its envelope") from None
        if isinstance(envelope, dict) and envelope.get("status") == "completed":
            return envelope.get("data", {})
        failure = envelope.get("error") if isinstance(envelope, dict) else None
        failure = failure if isinstance(failure, dict) else {}
        code = CODES.get(str(failure.get("code")), ErrorCode.INTERNAL)
        retry_after = failure.get("retry_after_s")
        raise HermesError(
            code, f"hecate answered {code.value}", retry_after_s=retry_after if isinstance(retry_after, int) else None
        )

    async def _authenticated(self) -> tuple[httpx.AsyncClient, StoredIdentity]:
        """The client of the current certificate; the previous one stays open for the calls still in flight."""
        identity = self._identity()
        files = self._files()
        if identity is None or files is None:
            raise HermesError(ErrorCode.UNAVAILABLE, "hermes has no identity in hecate")
        serial = identity.certificate.serial
        if self._current is None or self._current[0] != serial:
            certificate_chain, key, _ = files
            hecate = SpiffeId(trust_domain=identity.certificate.spiffe_id.trust_domain, identity="hecate")
            context = hecate_ssl_context(
                identity.certificate.chain_pem[-1], certificate_chain=certificate_chain, key=key
            )
            transport = HecateTransport(context, hecate_check(hecate, self._revocations))
            client = httpx.AsyncClient(transport=transport, base_url=self._url, timeout=self._timeout)
            if self._previous is not None:
                await self._previous.aclose()
            self._previous = self._current[1] if self._current is not None else None
            self._current = (serial, client)
        return self._current[1], identity

    def _token(self, identity: StoredIdentity) -> str:
        now = int(self._clock.now().timestamp())
        spiffe_id = identity.certificate.spiffe_id
        claims = {
            "iss": str(spiffe_id),
            "aud": f"spiffe://{spiffe_id.trust_domain}/hecate",
            "iat": now,
            "exp": now + TOKEN_LIFETIME_S,
            "jti": uuid.uuid4().hex,
        }
        headers = {"kid": identity.certificate.call_key_id, "typ": "JWT"}
        return jwt.encode(claims, load_call_key(identity.call_key_pem), algorithm="EdDSA", headers=headers)

    async def _call(self, method: str, path: str, *, body: Any = None, idempotency_key: str | None = None) -> Any:
        client, identity = await self._authenticated()
        headers = {"Authorization": f"Bearer {self._token(identity)}"}
        if idempotency_key is not None:
            headers["Idempotency-Key"] = idempotency_key
        return await self._send(client, method, path, json=body, headers=headers)

    # --- Operations ------------------------------------------------------------------------------------------------

    async def enroll(
        self, token: SecretValue, csr_pem: str, call_key_pem: str, idempotency_key: str
    ) -> IssuedCertificate:
        fingerprint = token.reveal().rsplit(".", 1)[-1]
        try:
            async with asyncio.timeout(self._timeout):
                chain = await presented_chain(self._host, self._port)
        except OSError, TimeoutError, ssl.SSLError:
            raise HermesError(ErrorCode.UNAVAILABLE, NOT_ANSWERING) from None
        verified = verify_hecate(chain, fingerprint, self._clock.now())
        root_pem = verified.root.public_bytes(serialization.Encoding.PEM).decode("ascii")
        transport = HecateTransport(hecate_ssl_context(root_pem), hecate_check(verified.spiffe_id, list))
        async with httpx.AsyncClient(transport=transport, base_url=self._url, timeout=self._timeout) as client:
            body = {"token": token.reveal(), "csr": csr_pem, "call_key": call_key_pem}
            data = await self._send(
                client, "POST", "/v1/identities:enroll", json=body, headers={"Idempotency-Key": idempotency_key}
            )
        return _issued(data)

    async def renew(self, csr_pem: str, call_key_pem: str, idempotency_key: str) -> IssuedCertificate:
        body = {"csr": csr_pem, "call_key": call_key_pem}
        return _issued(await self._call("POST", "/v1/certificates:renew", body=body, idempotency_key=idempotency_key))

    async def revocations(self) -> RevocationLists:
        data = await self._call("GET", "/v1/revocations")
        crls = tuple(
            x509.load_pem_x509_crl(item.encode("ascii")).public_bytes(serialization.Encoding.DER)
            for item in data["crls"]
        )
        return RevocationLists(crls_der=crls, expires_at=datetime.fromisoformat(data["expires_at"]))

    async def keys(self) -> PublishedKeys:
        data = await self._call("GET", "/v1/keys")
        return PublishedKeys(keys=tuple(data["keys"]), expires_at=datetime.fromisoformat(data["expires_at"]))

    async def key(self, kid: str) -> Mapping[str, Any]:
        data = await self._call("GET", f"/v1/keys/{quote(kid, safe='')}")
        key: Mapping[str, Any] = data["key"]
        return key

    async def resolve(self, references: Sequence[SecretReference]) -> tuple[ResolvedSecret, ...]:
        data = await self._call("POST", "/v1/secrets:resolve", body={"references": [str(item) for item in references]})
        return tuple(
            ResolvedSecret(
                reference=SecretReference.parse(item["reference"]),
                version=item["version"],
                value=SecretValue(item["value"]),
                cache_ttl_soft_s=item["cache_ttl_soft_s"],
                cache_ttl_hard_s=item["cache_ttl_hard_s"],
            )
            for item in data["secrets"]
        )
