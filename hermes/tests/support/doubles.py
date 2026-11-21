"""Doubles of the ports for the tests: the caller, the channels, the audit, the secrets, a chat provider and the trust
of Hermes, with the copy of its access list."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import replace
from datetime import datetime

from cryptography import x509

from hermes.application.ports import (
    AccessList,
    AuditEntry,
    CallCredentials,
    Caller,
    ChannelKey,
    ChannelState,
    PendingRenewal,
    PublishedKeys,
    RevocationLists,
    StoredIdentity,
    TrustMaterial,
)
from hermes.domain.configuration.digest import configuration_digest
from hermes.domain.configuration.model import Configuration
from hermes.domain.idempotency.record import KeyRecord, KeyState, StoredResponse
from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.drawing import PLAIN, draw
from hermes.domain.services.chat.provider import (
    ChatProvider,
    Delivery,
    ProviderFailure,
    Rendered,
    SentMessage,
)
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference, SpiffeId
from hermes.domain.shared.secret import SecretValue
from support.clock import START

CALL_TOKEN = "test-call-token"
CALLER = Caller(SpiffeId.parse("spiffe://cerbero.test/hades"), audit_context={"source": "test", "playbook": "pb-1"})


class AcceptingAuthenticator:
    """Accepts `Authorization: Bearer <token>` and nothing else; the real one checks the certificate (US2)."""

    def __init__(self, caller: Caller = CALLER, token: str = CALL_TOKEN) -> None:
        self.caller = caller
        self.token = token

    async def authenticate(self, credentials: CallCredentials) -> Caller:
        if credentials.authorization != f"Bearer {self.token}":
            raise HermesError(ErrorCode.AUTH, "the call is not authenticated")
        return self.caller


class MemoryChannels:
    """The channels in memory, with the semantics of the SQL repository: a code or a binding goes with its audit
    entry."""

    def __init__(self, audit: MemoryAudit | None = None) -> None:
        self._states: dict[ChannelKey, ChannelState] = {}
        self.reads = 0
        self.audit = audit if audit is not None else MemoryAudit()

    def verify(self, contact: str, provider: str, instance: str, address: str, at: datetime = START) -> None:
        key = ChannelKey(contact, "chat", provider, instance)
        self._states[key] = ChannelState(key, address, at, None, None)

    def unverified(self, contact: str, provider: str, instance: str) -> None:
        key = ChannelKey(contact, "chat", provider, instance)
        self._states[key] = ChannelState(key, None, None, None, None)

    async def state(self, key: ChannelKey) -> ChannelState | None:
        self.reads += 1
        return self._states.get(key)

    async def issue_code(self, key: ChannelKey, code_hash: bytes, expires_at: datetime, audit: AuditEntry) -> None:
        self._states[key] = ChannelState(key, None, None, code_hash, expires_at)
        await self.audit.append(audit)

    async def pending(self, instance: str) -> Sequence[ChannelState]:
        return [state for state in self._states.values() if state.key.instance == instance and state.code_hash]

    async def bind(
        self, key: ChannelKey, code_hash: bytes, address: str, verified_at: datetime, audit: AuditEntry
    ) -> bool:
        state = self._states.get(key)
        if state is None or state.code_hash != code_hash or state.code_expires_at is None:
            return False
        if state.code_expires_at <= verified_at:
            return False
        self._states[key] = ChannelState(key, address, verified_at, None, None)
        await self.audit.append(audit)
        return True


class MemoryAudit:
    def __init__(self) -> None:
        self.entries: list[AuditEntry] = []

    async def append(self, entry: AuditEntry) -> None:
        self.entries.append(entry)


class MemoryIdempotency:
    """The keys of idempotency in memory, with the semantics of the SQL repository: the response of a key and its audit
    entry go together, a key released by a send that failed without sending anything goes away with its audit entry,
    and a key in progress keeps the entry of its send with an unknown outcome until it completes or the purge removes
    it."""

    def __init__(self, audit: MemoryAudit | None = None) -> None:
        self.audit = audit if audit is not None else MemoryAudit()
        self.rows: dict[tuple[str, str], tuple[KeyRecord, datetime]] = {}
        self.pending: dict[tuple[str, str], AuditEntry] = {}

    async def find(self, caller: str, key: str) -> KeyRecord | None:
        row = self.rows.get((caller, key))
        return row[0] if row is not None else None

    async def reserve(self, caller: str, key: str, fingerprint: bytes, unknown: AuditEntry) -> None:
        if (caller, key) in self.rows:
            raise HermesError(ErrorCode.CONFLICT, "another request with this Idempotency-Key is in progress")
        self.rows[(caller, key)] = (KeyRecord(fingerprint, KeyState.IN_PROGRESS, None), unknown.at)
        self.pending[(caller, key)] = unknown

    async def complete(self, caller: str, key: str, response: StoredResponse, audit: AuditEntry) -> None:
        record, at = self.rows[(caller, key)]
        await self.audit.append(audit)
        self.rows[(caller, key)] = (replace(record, state=KeyState.COMPLETED, response=response), at)
        self.pending.pop((caller, key), None)

    async def release(self, caller: str, key: str, audit: AuditEntry) -> None:
        await self.audit.append(audit)
        row = self.rows.get((caller, key))
        if row is not None and row[0].state is KeyState.IN_PROGRESS:
            del self.rows[(caller, key)]
        self.pending.pop((caller, key), None)

    async def complete_unknown(self, caller: str, key: str, response: StoredResponse) -> None:
        record, at = self.rows[(caller, key)]
        unknown = self.pending.pop((caller, key), None)
        if unknown is not None:
            await self.audit.append(unknown)
        self.rows[(caller, key)] = (replace(record, state=KeyState.COMPLETED, response=response), at)

    async def purge(self, before: datetime) -> int:
        old = sorted((key for key, (_, at) in self.rows.items() if at < before), key=lambda key: self.rows[key][1])
        for key in old:
            unknown = self.pending.pop(key, None)
            if unknown is not None:
                await self.audit.append(unknown)
            del self.rows[key]
        return len(old)


class StaticSecrets:
    def __init__(self, values: Mapping[str, str]) -> None:
        self._values = dict(values)
        self.asked: list[str] = []

    async def resolve(self, reference: SecretReference) -> SecretValue:
        key = str(reference)
        self.asked.append(key)
        if key not in self._values:
            raise HermesError(ErrorCode.INTERNAL, "the secret is not available")
        return SecretValue(self._values[key])


class RecordingProvider:
    """A chat provider that keeps each delivery and draws in plain text; it can fail on demand."""

    def __init__(self, manifest: Manifest, failure: ProviderFailure | Exception | None = None) -> None:
        self.manifest = manifest
        self.failure = failure
        self.deliveries: list[Delivery] = []

    def render(self, message: NeutralMessage) -> Rendered:
        return draw(message, max_length=self.manifest.max_length, markup=PLAIN)

    async def send(self, delivery: Delivery) -> SentMessage:
        self.deliveries.append(delivery)
        if self.failure is not None:
            raise self.failure
        return SentMessage(f"ref-{len(self.deliveries)}")

    async def receive(self, instance: object, credentials: object) -> list[object]:
        return []

    async def check_connection(self, instance: object, credentials: object) -> None:
        if self.failure is not None:
            raise self.failure


class Providers:
    def __init__(self, **providers: ChatProvider) -> None:
        self._providers = providers

    def chat(self, provider: str) -> ChatProvider | None:
        return self._providers.get(provider)

    def manifests(self) -> tuple[Manifest, ...]:
        return tuple(self._providers[name].manifest for name in sorted(self._providers))


class MemoryUsedTokens:
    def __init__(self) -> None:
        self.rows: dict[tuple[str, str], datetime] = {}

    async def remember(self, issuer: str, jti: str, expires_at: datetime) -> bool:
        if (issuer, jti) in self.rows:
            return False
        self.rows[(issuer, jti)] = expires_at
        return True

    async def purge(self, now: datetime) -> int:
        old = [row for row, expires_at in self.rows.items() if expires_at < now]
        for row in old:
            del self.rows[row]
        return len(old)


def access_list(callers: Sequence[str] = (), *, version: int | None = None, at: datetime = START) -> AccessList:
    """The access list of Hermes as Hécate gives it: the identities by name and, by default, the version that granting
    them one by one leaves."""
    return AccessList(
        app="hermes",
        callers=tuple(sorted(callers)),
        version=len(callers) if version is None else version,
        fetched_at=at,
    )


class StaticTrust:
    """The trust of Hermes from the test CA: its root, the SPIFFE ID of Hermes, the CRLs, the published keys and the
    copy of its access list, empty by default and None without a copy.

    `granted` is the list that Hécate has, the copy unless a test says otherwise: a lookup brings it and puts it in the
    copy, so a test grants in Hécate by adding to it. `access_lookups` counts the lookups, and with `down` Hécate does
    not answer about keys or lists.
    """

    def __init__(
        self,
        root: x509.Certificate | None,
        hermes: str,
        revocations: Sequence[x509.CertificateRevocationList] = (),
        keys: Sequence[Mapping[str, object]] = (),
        *,
        callers: Sequence[str] | None = (),
        granted: Sequence[str] | None = None,
        later: Mapping[str, Mapping[str, object]] | None = None,
        asked: list[str] | None = None,
        down: bool = False,
    ) -> None:
        self.root = root
        self.hermes = SpiffeId.parse(hermes)
        self.revocations = tuple(revocations)
        self.keys = tuple(keys)
        self.access = access_list(callers) if callers is not None else None
        self.granted = list(granted if granted is not None else callers or ())
        self.access_lookups = 0
        self.later = later if later is not None else {}
        self.asked = asked if asked is not None else []
        self.down = down

    def material(self) -> TrustMaterial | None:
        if self.root is None or self.access is None:
            return None
        return TrustMaterial(
            root=self.root, hermes=self.hermes, revocations=self.revocations, keys=self.keys, access=self.access
        )

    async def key(self, kid: str) -> Mapping[str, object] | None:
        """A key that Hécate knows and the list does not have yet; it keeps each kid it is asked for."""
        self.asked.append(kid)
        if self.down:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer about the key")
        return self.later.get(kid)

    async def lookup_access(self) -> AccessList:
        """The list that Hécate has now, which replaces the copy."""
        self.access_lookups += 1
        if self.down:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer about the access list")
        self.access = access_list(self.granted)
        return self.access


class MemoryIdentityStore:
    """The identity store in memory; `saved` keeps each identity in the order it was written."""

    def __init__(self) -> None:
        self.identity: StoredIdentity | None = None
        self.saved: list[StoredIdentity] = []
        self.pending: PendingRenewal | None = None
        self.revocations: RevocationLists | None = None
        self.keys: PublishedKeys | None = None
        self.access: AccessList | None = None

    def load(self) -> StoredIdentity | None:
        return self.identity

    def save(self, identity: StoredIdentity) -> None:
        self.identity = identity
        self.saved.append(identity)

    def load_pending_renewal(self) -> PendingRenewal | None:
        return self.pending

    def save_pending_renewal(self, pending: PendingRenewal) -> None:
        self.pending = pending

    def clear_pending_renewal(self) -> None:
        self.pending = None

    def load_revocations(self) -> RevocationLists | None:
        return self.revocations

    def save_revocations(self, revocations: RevocationLists) -> None:
        self.revocations = revocations

    def load_keys(self) -> PublishedKeys | None:
        return self.keys

    def save_keys(self, keys: PublishedKeys) -> None:
        self.keys = keys

    def load_access(self) -> AccessList | None:
        return self.access

    def save_access(self, access: AccessList) -> None:
        self.access = access


class MemoryConfigStore(MemoryChannels):
    """The configuration and the channels in memory, with the semantics of the SQL repositories: replacing the
    configuration keeps the run-time of the channels that stay, and a channel that goes loses it; an application goes
    with its audit entry."""

    def __init__(self, initial: Configuration | None = None, audit: MemoryAudit | None = None) -> None:
        super().__init__(audit)
        self.configuration = Configuration.empty()
        self.writes = 0
        if initial is not None:
            self._write(initial)

    def _write(self, configuration: Configuration) -> None:
        wanted = {
            ChannelKey(contact.name, spec.service, spec.provider, spec.instance)
            for contact in configuration.contacts
            for spec in contact.channels
        }
        for key in list(self._states):
            if key not in wanted:
                del self._states[key]
        for key in wanted:
            self._states.setdefault(key, ChannelState(key, None, None, None, None))
        self.configuration = configuration
        self.writes += 1

    async def load(self) -> Configuration:
        return self.configuration

    async def replace(self, configuration: Configuration) -> None:
        self._write(configuration)

    async def replace_if(self, expected_digest: str, configuration: Configuration, audit: AuditEntry) -> Configuration:
        if configuration_digest(self.configuration) != expected_digest:
            raise HermesError(ErrorCode.CONFLICT, "the configuration changed since the plan")
        self._write(configuration)
        await self.audit.append(audit)
        return self.configuration

    async def issue_code(self, key: ChannelKey, code_hash: bytes, expires_at: datetime, audit: AuditEntry) -> None:
        if key not in self._states:
            raise HermesError(ErrorCode.NOT_FOUND, "the channel does not exist")
        await super().issue_code(key, code_hash, expires_at, audit)
