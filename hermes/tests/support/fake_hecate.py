"""A double of the API of Hécate with the rules that the tests of the identity need (docs/hecate/integracion.md).

It signs the key of each CSR with the test CA, gives the same certificate to the same request, treats another request
with a certificate already renewed as a second attempt (`auth`), can lose an answer after doing the work, can be sealed,
resolves its secrets with the cache times of Hécate and gives the access list of Hermes, which the admin changes with
`grant` and `revoke`. After `revoke_urgently` it refuses with `auth` every call made with that certificate, as Hécate
does after an urgent revocation; it counts the authenticated calls, and `hold_next` holds the next one until released.
"""

from __future__ import annotations

import asyncio
import base64
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from datetime import timedelta
from typing import Any

from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519

from hermes.application.ports import AccessList, IssuedCertificate, PublishedKeys, ResolvedSecret, RevocationLists
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference, SpiffeId
from hermes.domain.shared.secret import SecretValue
from support.clock import FakeClock
from support.pki import TestCa, spiffe, thumbprint


def call_key_id(public_pem: str) -> str:
    key = serialization.load_pem_public_key(public_pem.encode("ascii"))
    assert isinstance(key, ed25519.Ed25519PublicKey)
    raw = key.public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    return thumbprint(base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii"))


def pem(certificate: x509.Certificate) -> str:
    return certificate.public_bytes(serialization.Encoding.PEM).decode("ascii")


@dataclass
class FakeHecate:
    ca: TestCa
    clock: FakeClock
    lifetime: timedelta = timedelta(days=3)
    identity: str = "hermes"
    caller_serial: Callable[[], str | None] = lambda: None
    secrets: dict[str, str] = field(default_factory=dict)
    revoked: list[int] = field(default_factory=list)
    sealed: bool = False
    down: bool = False
    fail_next: list[HermesError] = field(default_factory=list)
    drop_next: int = 0
    renewals: list[tuple[str, str]] = field(default_factory=list)
    enrollments: list[str] = field(default_factory=list)
    resolutions: int = 0
    second_attempts: int = 0
    key_lookups: list[str] = field(default_factory=list)
    access: list[str] = field(default_factory=list)
    access_version: int = 0
    access_requests: int = 0
    refused: list[str] = field(default_factory=list)
    authenticated_calls: int = 0
    hold_next: asyncio.Event | None = None
    _renewed: dict[str, tuple[str, str]] = field(default_factory=dict)
    _issued: dict[tuple[str, str], IssuedCertificate] = field(default_factory=dict)
    _call_keys: dict[str, dict[str, Any]] = field(default_factory=dict)

    def _answering(self) -> None:
        if self.down:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer")
        if self.sealed:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate is sealed")
        if self.fail_next:
            raise self.fail_next.pop(0)

    def revoke_urgently(self, serial: str) -> None:
        """`hecate certificates revoke <serial> --urgent`, with the serial in hexadecimal as `IssuedCertificate.serial`:
        from now on every call made with that certificate is refused, and the serial is in the revocation list."""
        self.refused.append(serial)
        self.revoked.append(int(serial, 16))

    async def _authenticated(self) -> str | None:
        """Each authenticated call counts and reads its serial when it starts; `hold_next` holds it until released, and
        then it is decided with that serial: `auth` if Hécate revoked that certificate urgently."""
        self.authenticated_calls += 1
        serial = self.caller_serial()
        hold, self.hold_next = self.hold_next, None
        if hold is not None:
            await hold.wait()
        self._answering()
        if serial is not None and serial in self.refused:
            raise HermesError(ErrorCode.AUTH, "the certificate is revoked")
        return serial

    def _issue(self, csr_pem: str, call_public_pem: str) -> IssuedCertificate:
        request = x509.load_pem_x509_csr(csr_pem.encode("ascii"))
        public_key = request.public_key()
        assert isinstance(public_key, ec.EllipticCurvePublicKey)
        certificate = self.ca.certificate_for(
            public_key, self.identity, not_before=self.clock.now(), lifetime=self.lifetime
        )
        kid = call_key_id(call_public_pem)
        self._call_keys[kid] = {"kid": kid, "cerbero_identity": spiffe(self.identity), "cerbero_status": "active"}
        return IssuedCertificate(
            spiffe_id=SpiffeId.parse(spiffe(self.identity)),
            serial=format(certificate.serial_number, "x"),
            certificate_pem=pem(certificate),
            chain_pem=(self.ca.intermediate_pem, self.ca.root_pem),
            not_after=certificate.not_valid_after_utc,
            call_key_id=kid,
        )

    def _lose_the_answer(self) -> None:
        if self.drop_next:
            self.drop_next -= 1
            raise HermesError(ErrorCode.UNAVAILABLE, "the answer of hecate was lost")

    async def enroll(
        self, token: SecretValue, csr_pem: str, call_key_pem: str, idempotency_key: str
    ) -> IssuedCertificate:
        self._answering()
        if not token.reveal().endswith(self.ca.root_fingerprint):
            raise HermesError(ErrorCode.AUTH, "hecate does not have the fingerprint of the token")
        request = (idempotency_key, csr_pem)
        if request not in self._issued:
            self._issued[request] = self._issue(csr_pem, call_key_pem)
            self.enrollments.append(idempotency_key)
        self._lose_the_answer()
        return self._issued[request]

    async def renew(self, csr_pem: str, call_key_pem: str, idempotency_key: str) -> IssuedCertificate:
        caller = await self._authenticated()
        if caller is None:
            raise HermesError(ErrorCode.AUTH, "the call is not authenticated")
        request = (idempotency_key, csr_pem)
        if caller in self._renewed:
            if self._renewed[caller] != request:
                self.second_attempts += 1
                raise HermesError(ErrorCode.AUTH, "a second attempt to renew")
        else:
            self._issued[request] = self._issue(csr_pem, call_key_pem)
            self._renewed[caller] = request
            self.renewals.append(request)
        self._lose_the_answer()
        return self._issued[request]

    async def revocations(self) -> RevocationLists:
        await self._authenticated()
        crl = self.ca.crl(self.revoked, this_update=self.clock.now())
        return RevocationLists(
            crls_der=(crl.public_bytes(serialization.Encoding.DER),),
            expires_at=self.clock.now() + timedelta(seconds=300),
        )

    async def keys(self) -> PublishedKeys:
        await self._authenticated()
        return PublishedKeys(keys=tuple(self._call_keys.values()), expires_at=self.clock.now() + timedelta(seconds=300))

    def publish(self, jwk: dict[str, Any]) -> None:
        """A call key that Hécate has from now on, as an enrollment or a renewal of another component leaves it."""
        self._call_keys[jwk["kid"]] = jwk

    async def key(self, kid: str) -> Mapping[str, Any]:
        self.key_lookups.append(kid)
        await self._authenticated()
        if kid not in self._call_keys:
            raise HermesError(ErrorCode.NOT_FOUND, "the key does not exist")
        return self._call_keys[kid]

    def grant(self, identity: str) -> None:
        """`hecate access grant <identity> hermes`: a new version only if it changes something."""
        if identity not in self.access:
            self.access.append(identity)
            self.access_version += 1

    def revoke(self, identity: str) -> None:
        """`hecate access revoke <identity> hermes`."""
        if identity in self.access:
            self.access.remove(identity)
            self.access_version += 1

    async def callers(self) -> AccessList:
        self.access_requests += 1
        await self._authenticated()
        return AccessList(
            app=self.identity,
            callers=tuple(sorted(self.access)),
            version=self.access_version,
            fetched_at=self.clock.now(),
        )

    async def resolve(self, references: Sequence[SecretReference]) -> tuple[ResolvedSecret, ...]:
        await self._authenticated()
        self.resolutions += 1
        if any(str(reference) not in self.secrets for reference in references):
            raise HermesError(ErrorCode.NOT_FOUND, "a reference does not resolve")
        return tuple(
            ResolvedSecret(
                reference=reference,
                version=1,
                value=SecretValue(self.secrets[str(reference)]),
                cache_ttl_soft_s=300,
                cache_ttl_hard_s=3600,
            )
            for reference in references
        )
