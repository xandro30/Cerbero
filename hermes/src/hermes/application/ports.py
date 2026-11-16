"""The ports of the use cases: what the application needs from the database, Hécate, the files and the caller.

The chat provider port lives with the contract of its service type, in `hermes.domain.services.chat`.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from typing import Any, Protocol

from cryptography import x509

from hermes.domain.configuration.model import Configuration
from hermes.domain.idempotency.record import KeyRecord, StoredResponse
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference, SpiffeId
from hermes.domain.shared.secret import SecretValue

__all__ = [
    "AccessList",
    "AuditEntry",
    "AuditKind",
    "AuditLog",
    "Authenticator",
    "CallCredentials",
    "Caller",
    "ChannelKey",
    "ChannelRepository",
    "ChannelState",
    "Clock",
    "ConfigRepository",
    "GeneratedKeys",
    "HecateApi",
    "HecateTrustChanged",
    "IdempotencyRepository",
    "IdentityStore",
    "IssuedCertificate",
    "PendingRenewal",
    "PresentedCertificate",
    "PublishedKeys",
    "ResolvedSecret",
    "RevocationLists",
    "SecretSource",
    "StoredIdentity",
    "TrustMaterial",
    "TrustSource",
    "UsedTokens",
]


# --- The caller -------------------------------------------------------------------------------------------------


@dataclass(frozen=True)
class PresentedCertificate:
    """What the proxy forwards: the certificate of the connection, the chain that the client sent, its URI and the
    SPIFFE ID of the service it was presented to (`By`). Envoy proves the key; Hermes checks everything else."""

    certificate: x509.Certificate
    chain: tuple[x509.Certificate, ...]
    uri: str
    by: str | None


@dataclass(frozen=True)
class CallCredentials:
    """What arrives with a call: the certificate that the proxy forwarded, if any, and the Authorization header."""

    certificate: PresentedCertificate | None
    authorization: str | None


@dataclass(frozen=True)
class Caller:
    """An authenticated component; Hermes takes no permission from what its token says (FR-004)."""

    spiffe_id: SpiffeId
    audit_context: Mapping[str, str] | None = None


class Authenticator(Protocol):
    async def authenticate(self, credentials: CallCredentials) -> Caller:
        """The caller, or `auth` with the same message whatever check failed."""
        ...


@dataclass(frozen=True)
class AccessList:
    """The access list of Hermes as Hécate gave it: the identities that may call it, by name, the version of the list
    and when Hermes obtained it. Hermes replaces its copy whole; it holds names only."""

    app: str
    callers: tuple[str, ...]
    version: int
    fetched_at: datetime

    def allows(self, identity: str) -> bool:
        return identity in self.callers


@dataclass(frozen=True)
class TrustMaterial:
    """What checks a caller: the current root of Cerbero, the SPIFFE ID of Hermes, the latest CRLs and call keys that
    Hécate published and the copy of the access list of Hermes (FR-008: the last copy holds while Hécate does not
    answer)."""

    root: x509.Certificate
    hermes: SpiffeId
    revocations: tuple[x509.CertificateRevocationList, ...]
    keys: tuple[Mapping[str, Any], ...]
    access: AccessList


class TrustSource(Protocol):
    def material(self) -> TrustMaterial | None:
        """None while Hermes has no identity, no publications of Hécate or no copy of its access list yet."""
        ...

    async def key(self, kid: str) -> Mapping[str, Any] | None:
        """A call key that the last published list does not have yet, asked to Hécate; None if Hécate does not know
        it, and `unavailable` if it does not answer."""
        ...

    async def lookup_access(self) -> AccessList:
        """The access list asked to Hécate again, because a caller is not in the copy: the lookups at once share one
        request, and the new list replaces the copy; `unavailable` if Hécate does not answer."""
        ...


# --- Secrets ----------------------------------------------------------------------------------------------------


class SecretSource(Protocol):
    async def resolve(self, reference: SecretReference) -> SecretValue:
        """The value of a secret, from Hécate or its cache; never written to disk or logs."""
        ...


# --- The database -----------------------------------------------------------------------------------------------


class ConfigRepository(Protocol):
    async def load(self) -> Configuration: ...

    async def replace(self, configuration: Configuration) -> None:
        """The whole configuration in one transaction, keeping the verification of the channels that stay."""
        ...

    async def replace_if(self, expected: bytes, configuration: Configuration, audit: AuditEntry) -> Configuration:
        """As `replace`, only while the configuration in the database still has the digest `expected`; otherwise
        `conflict`. The check, the write, the audit entry and the read of what stays go under one lock, in one
        transaction (FR-029, research R7): it returns the configuration as the database keeps it."""
        ...


@dataclass(frozen=True)
class ChannelKey:
    contact: str
    service: str
    provider: str
    instance: str


@dataclass(frozen=True)
class ChannelState:
    key: ChannelKey
    address: str | None
    verified_at: datetime | None
    code_hash: bytes | None
    code_expires_at: datetime | None

    @property
    def verified(self) -> bool:
        return self.verified_at is not None


class ChannelRepository(Protocol):
    async def state(self, key: ChannelKey) -> ChannelState | None: ...

    async def issue_code(self, key: ChannelKey, code_hash: bytes, expires_at: datetime, audit: AuditEntry) -> None:
        """The channel without address or verification, waiting for this code, with its audit entry in the same
        transaction (FR-029)."""
        ...

    async def pending(self, instance: str) -> Sequence[ChannelState]:
        """The channels of an instance with a pending code."""
        ...

    async def bind(
        self, key: ChannelKey, code_hash: bytes, address: str, verified_at: datetime, audit: AuditEntry
    ) -> bool:
        """The channel verified on this address and without its code, with its audit entry in the same transaction
        (FR-029), only if `code_hash` is its pending code and it has not expired at `verified_at`; False otherwise,
        and nothing changes."""
        ...


class AuditKind(StrEnum):
    SEND = "send"
    CONFIG_APPLIED = "config_applied"
    VERIFICATION_CODE_ISSUED = "verification_code_issued"
    CHANNEL_VERIFIED = "channel_verified"


@dataclass(frozen=True)
class AuditEntry:
    """An append-only entry; it never carries secrets, verification codes or addresses (FR-029)."""

    at: datetime
    kind: AuditKind
    actor: str
    outcome: Mapping[str, Any]
    function: str | None = None
    instance: str | None = None
    recipients: tuple[str, ...] | None = None
    side_effect: str | None = None
    discloses_data: str | None = None
    audit_context: Mapping[str, str] | None = None
    idempotency_key: str | None = None


class AuditLog(Protocol):
    async def append(self, entry: AuditEntry) -> None: ...


class IdempotencyRepository(Protocol):
    async def find(self, caller: str, key: str) -> KeyRecord | None: ...

    async def reserve(self, caller: str, key: str, fingerprint: bytes, unknown: AuditEntry) -> None:
        """The row `in_progress` from `unknown.at`, committed before the provider, keeping `unknown`: the audit entry
        of the send if its outcome stays unknown. `conflict` if the key already has a row."""
        ...

    async def complete(self, caller: str, key: str, response: StoredResponse, audit: AuditEntry) -> None:
        """The row `completed` with its response, in the same transaction as the audit entry of the send."""
        ...

    async def complete_unknown(self, caller: str, key: str, response: StoredResponse) -> None:
        """A row left `in_progress` by a cut, `completed` with the response of an unknown outcome, in the same
        transaction as the entry that its reservation kept (FR-029)."""
        ...

    async def purge(self, before: datetime) -> int:
        """The rows reserved before `before` go away; one still `in_progress` leaves the entry that its reservation
        kept, in the same transaction."""
        ...


class UsedTokens(Protocol):
    async def remember(self, issuer: str, jti: str, expires_at: datetime) -> bool:
        """False if this `jti` of this issuer was already used (FR-003)."""
        ...

    async def purge(self, now: datetime) -> int: ...


# --- Hécate -----------------------------------------------------------------------------------------------------


@dataclass(frozen=True)
class IssuedCertificate:
    """What enrollment and renewal return (docs/hecate/integracion.md)."""

    spiffe_id: SpiffeId
    serial: str
    certificate_pem: str
    chain_pem: tuple[str, ...]
    not_after: datetime
    call_key_id: str


@dataclass(frozen=True)
class RevocationLists:
    crls_der: tuple[bytes, ...]
    expires_at: datetime


@dataclass(frozen=True)
class PublishedKeys:
    keys: tuple[Mapping[str, Any], ...]
    expires_at: datetime


@dataclass(frozen=True)
class ResolvedSecret:
    reference: SecretReference
    version: int
    value: SecretValue
    cache_ttl_soft_s: int
    cache_ttl_hard_s: int


class HecateTrustChanged(HermesError):
    """Hécate presents a root CA that is not the one Hermes enrolled with: Hermes needs a new enrollment token."""

    def __init__(self) -> None:
        super().__init__(ErrorCode.AUTH, "hecate presents another root CA")


class HecateApi(Protocol):
    """The API of Hécate (docs/hecate/integracion.md). Its failures are `HermesError`s with the code that Hécate gave,
    `unavailable` when it does not answer, and `HecateTrustChanged` when it presents another root CA."""

    async def enroll(
        self, token: SecretValue, csr_pem: str, call_key_pem: str, idempotency_key: str
    ) -> IssuedCertificate:
        """Checks Hécate with the fingerprint of the token before sending anything; `call_key_pem` is public."""
        ...

    async def renew(self, csr_pem: str, call_key_pem: str, idempotency_key: str) -> IssuedCertificate: ...

    async def revocations(self) -> RevocationLists: ...

    async def keys(self) -> PublishedKeys: ...

    async def key(self, kid: str) -> Mapping[str, Any]: ...

    async def callers(self) -> AccessList:
        """The access list of Hermes, `GET /v1/callers`: Hécate gives each app its own and no other."""
        ...

    async def resolve(self, references: Sequence[SecretReference]) -> tuple[ResolvedSecret, ...]: ...


@dataclass(frozen=True)
class GeneratedKeys:
    """New keys of Hermes: the TLS key (P-256), the call-signing key (Ed25519), the CSR and the public call key."""

    tls_key_pem: SecretValue
    call_key_pem: SecretValue
    csr_pem: str
    call_public_pem: str


@dataclass(frozen=True)
class StoredIdentity:
    certificate: IssuedCertificate
    tls_key_pem: SecretValue
    call_key_pem: SecretValue
    root_fingerprint: str


@dataclass(frozen=True)
class PendingRenewal:
    """A renewal in flight, kept until the new certificate arrives so the very same request can be repeated."""

    tls_key_pem: SecretValue
    call_key_pem: SecretValue
    csr_pem: str
    call_public_pem: str
    idempotency_key: str


class IdentityStore(Protocol):
    """The identity in files: the certificate for the proxy in a new generation, the rest in the state directory."""

    def load(self) -> StoredIdentity | None: ...

    def save(self, identity: StoredIdentity) -> None: ...

    def load_pending_renewal(self) -> PendingRenewal | None: ...

    def save_pending_renewal(self, pending: PendingRenewal) -> None: ...

    def clear_pending_renewal(self) -> None: ...

    def load_revocations(self) -> RevocationLists | None: ...

    def save_revocations(self, revocations: RevocationLists) -> None: ...

    def load_keys(self) -> PublishedKeys | None: ...

    def save_keys(self, keys: PublishedKeys) -> None: ...

    def load_access(self) -> AccessList | None: ...

    def save_access(self, access: AccessList) -> None: ...
