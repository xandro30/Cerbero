"""A test CA shaped like Hécate's (docs/hecate/integracion.md).

Root and intermediate in P-256, leaf certificates whose only name is a SPIFFE ID, X.509 CRLs signed by the
intermediate, Ed25519 call keys with their RFC 8037 JWK, and short call tokens.
"""

from __future__ import annotations

import base64
import hashlib
import json
import secrets
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any

import jwt
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

TRUST_DOMAIN = "cerbero.test"


def spiffe(identity: str, trust_domain: str = TRUST_DOMAIN) -> str:
    return f"spiffe://{trust_domain}/{identity}"


def _b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def _pem(certificate: x509.Certificate) -> str:
    return certificate.public_bytes(serialization.Encoding.PEM).decode("ascii")


def thumbprint(x: str) -> str:
    """The `kid` of a call key as Hécate gives it: the RFC 7638 thumbprint of its JWK, in base64url."""
    members = json.dumps({"crv": "Ed25519", "kty": "OKP", "x": x}, separators=(",", ":"), sort_keys=True)
    return _b64url(hashlib.sha256(members.encode("ascii")).digest())


@dataclass(frozen=True)
class Leaf:
    certificate: x509.Certificate
    key: ec.EllipticCurvePrivateKey

    @property
    def pem(self) -> str:
        return _pem(self.certificate)

    @property
    def key_pem(self) -> str:
        return self.key.private_bytes(
            serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()
        ).decode("ascii")

    @property
    def serial(self) -> int:
        return self.certificate.serial_number


@dataclass(frozen=True)
class CallKey:
    private: ed25519.Ed25519PrivateKey
    kid: str
    identity: str

    def jwk(self, *, status: str = "active", not_after: datetime | None = None) -> dict[str, Any]:
        """The key as `GET /v1/keys` of Hécate publishes it (RFC 8037 with the members of Cerbero)."""
        raw = self.private.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
        return {
            "kty": "OKP",
            "crv": "Ed25519",
            "x": _b64url(raw),
            "kid": self.kid,
            "alg": "EdDSA",
            "use": "sig",
            "cerbero_identity": self.identity,
            "cerbero_status": status,
            "cerbero_not_after": (not_after or datetime(2026, 11, 17, tzinfo=UTC)).strftime("%Y-%m-%dT%H:%M:%S.%fZ"),
        }

    def token(
        self,
        *,
        issuer: str,
        audience: str,
        issued_at: datetime,
        lifetime: timedelta = timedelta(seconds=300),
        jti: str | None = None,
        extra: dict[str, Any] | None = None,
        kid: str | None = None,
    ) -> str:
        claims: dict[str, Any] = {
            "iss": issuer,
            "aud": audience,
            "iat": int(issued_at.timestamp()),
            "exp": int((issued_at + lifetime).timestamp()),
            "jti": jti if jti is not None else secrets.token_hex(16),
            **(extra or {}),
        }
        return jwt.encode(claims, self.private, algorithm="EdDSA", headers={"kid": kid or self.kid, "typ": "JWT"})


class TestCa:
    """A root and an intermediate that issue certificates, CRLs and call keys for the tests."""

    __test__ = False

    def __init__(self, trust_domain: str = TRUST_DOMAIN, now: datetime | None = None) -> None:
        self.trust_domain = trust_domain
        self.now = now or datetime.now(UTC)
        self.root_key = ec.generate_private_key(ec.SECP256R1())
        self.root = self._ca_certificate("Cerbero test root", self.root_key, self.root_key, None, path_length=1)
        self.intermediate_key = ec.generate_private_key(ec.SECP256R1())
        self.intermediate = self._ca_certificate(
            "Cerbero test intermediate", self.intermediate_key, self.root_key, self.root, path_length=0
        )

    def _ca_certificate(
        self,
        name: str,
        key: ec.EllipticCurvePrivateKey,
        issuer_key: ec.EllipticCurvePrivateKey,
        issuer: x509.Certificate | None,
        *,
        path_length: int,
    ) -> x509.Certificate:
        subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
        builder = (
            x509.CertificateBuilder()
            .subject_name(subject)
            .issuer_name(issuer.subject if issuer else subject)
            .public_key(key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(self.now - timedelta(days=1))
            .not_valid_after(self.now + timedelta(days=365))
            .add_extension(x509.BasicConstraints(ca=True, path_length=path_length), critical=True)
            .add_extension(
                x509.KeyUsage(
                    digital_signature=False,
                    content_commitment=False,
                    key_encipherment=False,
                    data_encipherment=False,
                    key_agreement=False,
                    key_cert_sign=True,
                    crl_sign=True,
                    encipher_only=False,
                    decipher_only=False,
                ),
                critical=True,
            )
            .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False)
            .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(issuer_key.public_key()), critical=False)
        )
        return builder.sign(issuer_key, hashes.SHA256())

    def issue(
        self,
        identity: str,
        *,
        not_before: datetime | None = None,
        lifetime: timedelta = timedelta(days=3),
        names: list[x509.GeneralName] | None = None,
    ) -> Leaf:
        """A leaf certificate whose only name is the SPIFFE ID of `identity`, valid as server and as client."""
        key = ec.generate_private_key(ec.SECP256R1())
        certificate = self.certificate_for(
            key.public_key(), identity, not_before=not_before, lifetime=lifetime, names=names
        )
        return Leaf(certificate=certificate, key=key)

    def certificate_for(
        self,
        public_key: ec.EllipticCurvePublicKey,
        identity: str,
        *,
        not_before: datetime | None = None,
        lifetime: timedelta = timedelta(days=3),
        names: list[x509.GeneralName] | None = None,
    ) -> x509.Certificate:
        """The certificate of a key that the CA did not generate, as Hécate signs the key of a CSR."""
        start = not_before or self.now - timedelta(minutes=5)
        general_names = names or [x509.UniformResourceIdentifier(spiffe(identity, self.trust_domain))]
        return (
            x509.CertificateBuilder()
            .subject_name(x509.Name([]))
            .issuer_name(self.intermediate.subject)
            .public_key(public_key)
            .serial_number(x509.random_serial_number())
            .not_valid_before(start)
            .not_valid_after(start + lifetime)
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.SubjectAlternativeName(general_names), critical=True)
            .add_extension(
                x509.KeyUsage(
                    digital_signature=True,
                    content_commitment=False,
                    key_encipherment=False,
                    data_encipherment=False,
                    key_agreement=False,
                    key_cert_sign=False,
                    crl_sign=False,
                    encipher_only=False,
                    decipher_only=False,
                ),
                critical=True,
            )
            .add_extension(
                x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH, ExtendedKeyUsageOID.CLIENT_AUTH]),
                critical=False,
            )
            .add_extension(x509.SubjectKeyIdentifier.from_public_key(public_key), critical=False)
            .add_extension(
                x509.AuthorityKeyIdentifier.from_issuer_public_key(self.intermediate_key.public_key()), critical=False
            )
            .sign(self.intermediate_key, hashes.SHA256())
        )

    def crl(
        self, revoked: list[int] | None = None, *, this_update: datetime | None = None
    ) -> x509.CertificateRevocationList:
        """A CRL of the intermediate, valid 300 s, with the given serials revoked."""
        start = this_update or self.now
        builder = (
            x509.CertificateRevocationListBuilder()
            .issuer_name(self.intermediate.subject)
            .last_update(start)
            .next_update(start + timedelta(seconds=300))
        )
        for serial in revoked or []:
            builder = builder.add_revoked_certificate(
                x509.RevokedCertificateBuilder().serial_number(serial).revocation_date(start).build()
            )
        return builder.sign(self.intermediate_key, hashes.SHA256())

    def call_key(self, identity: str) -> CallKey:
        """An Ed25519 call key of `identity`, with the thumbprint as its `kid` and its SPIFFE ID as its identity."""
        private = ed25519.Ed25519PrivateKey.generate()
        raw = private.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
        return CallKey(private=private, kid=thumbprint(_b64url(raw)), identity=spiffe(identity, self.trust_domain))

    @property
    def root_pem(self) -> str:
        return _pem(self.root)

    @property
    def root_fingerprint(self) -> str:
        """The SHA-256 of the DER of the root, in hexadecimal, as the enrollment token of Hécate carries it."""
        return hashlib.sha256(self.root.public_bytes(serialization.Encoding.DER)).hexdigest()

    def enrollment_token(self, identity: str = "hermes") -> str:
        """A token with the format of Hécate (`HET1.<identity>.<id>.<secret>.<fingerprint>`)."""
        return f"HET1.{identity}.{secrets.token_hex(8)}.{_b64url(secrets.token_bytes(32))}.{self.root_fingerprint}"

    @property
    def intermediate_pem(self) -> str:
        return _pem(self.intermediate)

    @property
    def chain_pem(self) -> str:
        """The intermediate and the root, in that order, as Hécate returns `chain`."""
        return self.intermediate_pem + self.root_pem
