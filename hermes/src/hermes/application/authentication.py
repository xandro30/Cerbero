"""The authentication of each call to the API (FR-003, FR-004, research R3, contracts/api.md#autenticación).

In order: the certificate that the proxy forwarded, with its chain up to the current root of Cerbero, its only name a
SPIFFE ID of the trust domain and its serial out of the CRL of its intermediate; then the short token, signed with an
active call key that Hécate publishes for that identity, for Hermes, with its times and a `jti` used once. Any failure
answers `auth` with the same message; only the log says why, never with the token or the certificate.
"""

from __future__ import annotations

import base64
import json
import logging
import re
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any

import jwt
from cryptography import x509
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.x509.verification import PolicyBuilder, Store, VerificationError

from hermes.application.ports import (
    CallCredentials,
    Caller,
    PresentedCertificate,
    TrustMaterial,
    TrustSource,
    UsedTokens,
)
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId

AUTH_MESSAGE = "the call is not authenticated"
MAX_LIFETIME_S = 300
MARGIN = timedelta(seconds=30)
THUMBPRINT = re.compile(r"[A-Za-z0-9_-]{43}")
JTI = re.compile(r"[\x20-\x7e]{1,128}")
AUDIT_KEYS = 16
AUDIT_KEY_LENGTH = 64
AUDIT_VALUE_LENGTH = 256
AUDIT_BYTES = 2048
AUDIT_RULE = (
    "cerbero_audit must be an object of up to 16 keys of 1 to 64 characters, with text values of up to 256 characters"
    " and 2 KiB in all"
)

logger = logging.getLogger(__name__)


class _Refused(Exception):
    def __init__(self, reason: str) -> None:
        super().__init__(reason)
        self.reason = reason


@dataclass(frozen=True)
class _Claims:
    jti: str
    expires_at: datetime
    audit: Any


def _public_key(jwk: Mapping[str, Any]) -> Ed25519PublicKey:
    x = jwk.get("x")
    if jwk.get("kty") != "OKP" or jwk.get("crv") != "Ed25519" or not isinstance(x, str):
        raise _Refused("key_format")
    try:
        return Ed25519PublicKey.from_public_bytes(base64.urlsafe_b64decode(x + "=" * (-len(x) % 4)))
    except ValueError:
        raise _Refused("key_format") from None


def _whole_number(value: Any) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise _Refused("token_times")
    return value


def _audit_context(value: Any) -> Mapping[str, str] | None:
    if value is None:
        return None
    valid = (
        isinstance(value, dict)
        and len(value) <= AUDIT_KEYS
        and all(
            isinstance(key, str)
            and 1 <= len(key) <= AUDIT_KEY_LENGTH
            and isinstance(item, str)
            and len(item) <= AUDIT_VALUE_LENGTH
            for key, item in value.items()
        )
        and len(json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) <= AUDIT_BYTES
    )
    if not valid:
        raise HermesError(ErrorCode.VALIDATION, AUDIT_RULE, details=[{"path": "cerbero_audit", "message": AUDIT_RULE}])
    return dict(value)


class CallAuthenticator:
    def __init__(self, *, trust: TrustSource, used_tokens: UsedTokens, clock: Clock) -> None:
        self._trust = trust
        self._used = used_tokens
        self._clock = clock

    async def authenticate(self, credentials: CallCredentials) -> Caller:
        material = self._trust.material()
        if material is None:
            raise HermesError(ErrorCode.UNAVAILABLE, "hermes is not ready")
        now = self._clock.now()
        try:
            identity = self._certificate(credentials.certificate, material, now)
            claims = await self._token(credentials.authorization, identity, material, now)
            if not await self._used.remember(str(identity), claims.jti, claims.expires_at + MARGIN):
                raise _Refused("jti_used")
        except _Refused as refused:
            logger.info("auth.refused", extra={"fields": {"reason": refused.reason}})
            raise HermesError(ErrorCode.AUTH, AUTH_MESSAGE) from None
        return Caller(spiffe_id=identity, audit_context=_audit_context(claims.audit))

    @staticmethod
    def _certificate(presented: PresentedCertificate | None, material: TrustMaterial, now: datetime) -> SpiffeId:
        if presented is None:
            raise _Refused("no_certificate")
        if presented.by is not None and presented.by != str(material.hermes):
            raise _Refused("presented_to_another_service")
        verifier = PolicyBuilder().store(Store([material.root])).time(now).build_client_verifier()
        intermediates = [item for item in presented.chain if item != presented.certificate]
        try:
            verified = verifier.verify(presented.certificate, intermediates)
        except VerificationError:
            raise _Refused("chain") from None
        names = verified.subjects or []
        if len(names) != 1 or not isinstance(names[0], x509.UniformResourceIdentifier):
            raise _Refused("names")
        try:
            identity = SpiffeId.parse(names[0].value)
        except HermesError:
            raise _Refused("not_a_spiffe_id") from None
        if identity.trust_domain != material.hermes.trust_domain:
            raise _Refused("trust_domain")
        if presented.uri != str(identity):
            raise _Refused("uri")
        issuer = verified.chain[1]
        issuer_key = issuer.public_key()
        if not isinstance(issuer_key, ec.EllipticCurvePublicKey):
            raise _Refused("issuer_key")
        crl = next(
            (
                item
                for item in material.revocations
                if item.issuer == issuer.subject and item.is_signature_valid(issuer_key)
            ),
            None,
        )
        if crl is None:
            raise _Refused("no_revocation_list")
        if crl.get_revoked_certificate_by_serial_number(presented.certificate.serial_number) is not None:
            raise _Refused("revoked")
        return identity

    async def _key(self, kid: object, material: TrustMaterial) -> Mapping[str, Any]:
        """The key of the token: in the last list, or asked to Hécate if the `kid` has the form of a thumbprint."""
        key = next((item for item in material.keys if item.get("kid") == kid), None)
        if key is not None:
            return key
        if not isinstance(kid, str) or THUMBPRINT.fullmatch(kid) is None:
            raise _Refused("unknown_key")
        asked = await self._trust.key(kid)
        if asked is None or asked.get("kid") != kid:
            raise _Refused("unknown_key")
        return asked

    async def _token(
        self, authorization: str | None, identity: SpiffeId, material: TrustMaterial, now: datetime
    ) -> _Claims:
        if authorization is None or not authorization.startswith("Bearer ") or len(authorization) == len("Bearer "):
            raise _Refused("no_token")
        token = authorization[len("Bearer ") :]
        try:
            header = jwt.get_unverified_header(token)
        except jwt.PyJWTError:
            raise _Refused("token_format") from None
        key = await self._key(header.get("kid"), material)
        if key.get("cerbero_identity") != str(identity):
            raise _Refused("key_of_another_identity")
        if key.get("cerbero_status") != "active":
            raise _Refused("key_not_active")
        try:
            claims = jwt.decode(
                token,
                _public_key(key),
                algorithms=["EdDSA"],
                options={
                    "verify_exp": False,
                    "verify_iat": False,
                    "verify_nbf": False,
                    "verify_aud": False,
                    "verify_iss": False,
                    "require": [],
                },
            )
        except jwt.PyJWTError:
            raise _Refused("signature") from None
        if claims.get("iss") != str(identity):
            raise _Refused("issuer")
        if claims.get("aud") != str(material.hermes):
            raise _Refused("audience")
        issued_at, expires_at = _whole_number(claims.get("iat")), _whole_number(claims.get("exp"))
        if not 0 <= expires_at - issued_at <= MAX_LIFETIME_S:
            raise _Refused("token_lifetime")
        issued, expires = datetime.fromtimestamp(issued_at, UTC), datetime.fromtimestamp(expires_at, UTC)
        if now + MARGIN < issued or now >= expires + MARGIN:
            raise _Refused("token_times")
        jti = claims.get("jti")
        if not isinstance(jti, str) or JTI.fullmatch(jti) is None:
            raise _Refused("jti")
        return _Claims(jti=jti, expires_at=expires, audit=claims.get("cerbero_audit"))
