"""The identity of Hermes in Hécate (FR-005 to FR-008, FR-010, research R4).

- **Enrollment:** with the token of the admin; the client of Hécate checks it with the fingerprint of the token before
  sending anything. A lost answer is repeated with the very same request, which gives the same certificate.
- **Renewal:** at two thirds of the life of the certificate, once per certificate, with new keys saved before sending.
  Without an answer, or with Hécate sealed or down, the same request is repeated every 60 s; `auth` means that the
  certificate is no longer valid.
- **Publications:** the CRLs and the call keys of Hécate and the access list of Hermes, refreshed every 240 s and kept
  on disk; while Hécate does not answer, the last copy holds. A caller that is not in the copy of the list makes Hermes
  ask for it again, with one request at a time, so a grant counts in the next call.
- **Validity:** without a valid certificate (missing, expired, in the CRL, refused at renewal, or Hécate with another
  root CA) Hermes is not ready and needs a new enrollment token.
"""

from __future__ import annotations

import asyncio
import logging
import re
import uuid
from collections.abc import Callable, Mapping
from datetime import datetime
from typing import Any

from cryptography import x509

from hermes.application.ports import (
    AccessList,
    GeneratedKeys,
    HecateApi,
    HecateTrustChanged,
    IdentityStore,
    IssuedCertificate,
    PendingRenewal,
    PublishedKeys,
    RevocationLists,
    StoredIdentity,
    TrustMaterial,
)
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue

NEEDS_TOKEN = "needs a new enrollment token"  # noqa: S105 (a reason, not a credential)
RETRY_S = 60.0
PUBLICATIONS_INTERVAL_S = 240
ENROLL_ATTEMPTS = 3
TOKEN = re.compile(r"HET1\.[a-z][a-z0-9-]{0,62}\.[0-9a-f]{16}\.[A-Za-z0-9_-]{43}\.[0-9a-f]{64}")
TOKEN_RULE = "the enrollment token does not have the format of Hécate (HET1....)"  # noqa: S105 (a rule)

logger = logging.getLogger(__name__)


def _certificate(pem: str) -> x509.Certificate:
    return x509.load_pem_x509_certificate(pem.encode("ascii"))


class IdentityService:
    def __init__(
        self,
        *,
        store: IdentityStore,
        hecate: HecateApi,
        clock: Clock,
        new_keys: Callable[[], GeneratedKeys],
        new_request_key: Callable[[], str] = lambda: uuid.uuid4().hex,
    ) -> None:
        self._store = store
        self._hecate = hecate
        self._clock = clock
        self._new_keys = new_keys
        self._new_request_key = new_request_key
        self._identity: StoredIdentity | None = None
        self._intermediate: x509.Certificate | None = None
        self._root: x509.Certificate | None = None
        self._pending: PendingRenewal | None = None
        self._crls: tuple[x509.CertificateRevocationList, ...] = ()
        self._keys: PublishedKeys | None = None
        self._asked_keys: dict[str, Mapping[str, Any]] = {}
        self._lookups: dict[str, asyncio.Future[Mapping[str, Any] | None]] = {}
        self._access: AccessList | None = None
        self._access_lookup: asyncio.Future[AccessList] | None = None
        self._refused = False
        self._trust_changed = False
        self._waiting: list[asyncio.Event] = []

    # --- State -----------------------------------------------------------------------------------------------------

    def load(self) -> None:
        """What the store kept: the identity, a renewal in flight and the last publications, with the access list."""
        self._set_identity(self._store.load())
        self._pending = self._store.load_pending_renewal()
        self._set_revocations(self._store.load_revocations())
        self._keys = self._store.load_keys()
        self._access = self._store.load_access()

    def _set_identity(self, identity: StoredIdentity | None) -> None:
        self._identity = identity
        self._intermediate = _certificate(identity.certificate.chain_pem[0]) if identity is not None else None
        self._root = _certificate(identity.certificate.chain_pem[-1]) if identity is not None else None

    def _set_revocations(self, revocations: RevocationLists | None) -> None:
        crls = revocations.crls_der if revocations is not None else ()
        self._crls = tuple(x509.load_der_x509_crl(item) for item in crls)

    @property
    def identity(self) -> StoredIdentity | None:
        return self._identity

    def revocation_lists(self) -> tuple[x509.CertificateRevocationList, ...]:
        return self._crls

    def _revoked(self, identity: StoredIdentity) -> bool:
        if self._intermediate is None:
            return False
        serial = int(identity.certificate.serial, 16)
        return any(
            crl.issuer == self._intermediate.subject and crl.get_revoked_certificate_by_serial_number(serial)
            for crl in self._crls
        )

    def problem(self) -> str | None:
        """None with a valid identity; otherwise why Hermes is not ready."""
        identity = self._identity
        if identity is None or self._refused or self._trust_changed:
            return NEEDS_TOKEN
        if self._clock.now() >= identity.certificate.not_after or self._revoked(identity):
            return NEEDS_TOKEN
        return None

    def access_loaded(self) -> bool:
        """Whether Hermes has a copy of its access list, saved or obtained from Hécate; without one it is not ready."""
        return self._access is not None

    def material(self) -> TrustMaterial | None:
        """The trust to check the callers (`TrustSource`), with a valid identity and the publications of Hécate, the
        access list among them."""
        identity, access = self._identity, self._access
        if identity is None or self._root is None or self.problem() is not None:
            return None
        if self._keys is None or not self._crls or access is None:
            return None
        return TrustMaterial(
            root=self._root,
            hermes=identity.certificate.spiffe_id,
            revocations=self._crls,
            keys=self._keys.keys,
            access=access,
        )

    async def wait_for_enrollment(self) -> None:
        event = asyncio.Event()
        self._waiting.append(event)
        await event.wait()

    # --- Enrollment ------------------------------------------------------------------------------------------------

    async def enroll(self, token: SecretValue) -> IssuedCertificate:
        if self._identity is not None and self.problem() is None:
            raise HermesError(ErrorCode.CONFLICT, "hermes already has a valid identity")
        if TOKEN.fullmatch(token.reveal()) is None:
            raise HermesError(ErrorCode.VALIDATION, TOKEN_RULE, details=[{"path": "token", "message": TOKEN_RULE}])
        keys = self._new_keys()
        request_key = self._new_request_key()
        for attempt in range(1, ENROLL_ATTEMPTS + 1):
            try:
                issued = await self._hecate.enroll(token, keys.csr_pem, keys.call_public_pem, request_key)
                break
            except HermesError as error:
                if error.code is not ErrorCode.UNAVAILABLE or attempt == ENROLL_ATTEMPTS:
                    raise
        identity = StoredIdentity(
            certificate=issued,
            tls_key_pem=keys.tls_key_pem,
            call_key_pem=keys.call_key_pem,
            root_fingerprint=token.reveal().rsplit(".", 1)[-1],
        )
        self._store.save(identity)
        self._store.clear_pending_renewal()
        self._set_identity(identity)
        self._pending = None
        self._refused = self._trust_changed = False
        logger.info(
            "identity.enrolled", extra={"fields": {"spiffe_id": str(issued.spiffe_id), "serial": issued.serial}}
        )
        await self.refresh_publications()
        waiting, self._waiting = self._waiting, []
        for event in waiting:
            event.set()
        return issued

    # --- Renewal ---------------------------------------------------------------------------------------------------

    def renew_at(self) -> datetime | None:
        identity = self._identity
        if identity is None:
            return None
        start = _certificate(identity.certificate.certificate_pem).not_valid_before_utc
        return start + (identity.certificate.not_after - start) * 2 / 3

    async def renew(self) -> IssuedCertificate:
        """Now, with the request in flight if there is one; a new request is saved before it is sent."""
        identity = self._identity
        if identity is None:
            raise HermesError(ErrorCode.UNAVAILABLE, "hermes has no identity in hecate")
        pending = self._pending
        if pending is None:
            keys = self._new_keys()
            pending = PendingRenewal(
                tls_key_pem=keys.tls_key_pem,
                call_key_pem=keys.call_key_pem,
                csr_pem=keys.csr_pem,
                call_public_pem=keys.call_public_pem,
                idempotency_key=self._new_request_key(),
            )
            self._store.save_pending_renewal(pending)
            self._pending = pending
        issued = await self._hecate.renew(pending.csr_pem, pending.call_public_pem, pending.idempotency_key)
        renewed = StoredIdentity(
            certificate=issued,
            tls_key_pem=pending.tls_key_pem,
            call_key_pem=pending.call_key_pem,
            root_fingerprint=identity.root_fingerprint,
        )
        self._store.save(renewed)
        self._store.clear_pending_renewal()
        self._set_identity(renewed)
        self._pending = None
        logger.info("identity.renewed", extra={"fields": {"serial": issued.serial}})
        return issued

    async def renewal_tick(self) -> float:
        """Renews when it is due and says how many seconds to wait until the next look."""
        renew_at = self.renew_at()
        if renew_at is None or self.problem() is not None:
            return RETRY_S
        now = self._clock.now()
        if self._pending is None and now < renew_at:
            return (renew_at - now).total_seconds()
        try:
            await self.renew()
        except HecateTrustChanged:
            self._trust_changed = True
            logger.warning("identity.renewal_failed", extra={"fields": {"reason": "hecate_trust_changed"}})
            return RETRY_S
        except HermesError as error:
            if error.code is ErrorCode.AUTH:
                self._refused = True
            logger.warning("identity.renewal_failed", extra={"fields": {"reason": error.code.value}})
            return RETRY_S
        renew_at = self.renew_at()
        return max((renew_at - self._clock.now()).total_seconds(), 0.0) if renew_at is not None else RETRY_S

    # --- Publications ----------------------------------------------------------------------------------------------

    def _set_access(self, access: AccessList) -> None:
        self._access = access
        self._store.save_access(access)

    async def _fetch_publications(self) -> None:
        revocations = await self._hecate.revocations()
        keys = await self._hecate.keys()
        access = await self._hecate.callers()
        self._set_revocations(revocations)
        self._keys = keys
        self._asked_keys.clear()
        self._store.save_revocations(revocations)
        self._store.save_keys(keys)
        self._set_access(access)
        self._trust_changed = False

    async def refresh_publications(self) -> None:
        if self._identity is None:
            return
        try:
            await self._fetch_publications()
        except HecateTrustChanged:
            self._trust_changed = True
            logger.warning("publications.refresh_failed", extra={"fields": {"reason": "hecate_trust_changed"}})
        except HermesError as error:
            logger.warning("publications.refresh_failed", extra={"fields": {"reason": error.code.value}})

    async def key(self, kid: str) -> Mapping[str, Any] | None:
        """A key that the last list does not have yet, because a component has just enrolled or renewed (research R3):
        one request to Hécate per `kid` at a time, and the key is kept until the next refresh of the list."""
        if kid in self._asked_keys:
            return self._asked_keys[kid]
        lookup = self._lookups.get(kid)
        if lookup is None:
            lookup = asyncio.ensure_future(self._ask_key(kid))
            self._lookups[kid] = lookup
            lookup.add_done_callback(lambda _: self._lookups.pop(kid, None))
        return await asyncio.shield(lookup)

    async def _ask_key(self, kid: str) -> Mapping[str, Any] | None:
        try:
            found = await self._hecate.key(kid)
        except HermesError as error:
            if error.code is ErrorCode.NOT_FOUND:
                return None
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer about the key") from None
        self._asked_keys[kid] = found
        return found

    async def lookup_access(self) -> AccessList:
        """The access list asked to Hécate again, because a caller is not in the copy: the lookups at once share one
        request, and the new list replaces the copy and is saved. Nothing remembers who was not in it."""
        lookup = self._access_lookup
        if lookup is None:
            lookup = asyncio.ensure_future(self._ask_access())
            self._access_lookup = lookup
            lookup.add_done_callback(self._access_asked)
        return await asyncio.shield(lookup)

    def _access_asked(self, _: asyncio.Future[AccessList]) -> None:
        self._access_lookup = None

    async def _ask_access(self) -> AccessList:
        try:
            access = await self._hecate.callers()
        except HermesError:
            raise HermesError(ErrorCode.UNAVAILABLE, "hecate does not answer about the access list") from None
        self._set_access(access)
        return access

    async def ensure_publications(self) -> None:
        """At start: the saved copy if there is one; otherwise Hécate must give them, the access list among them."""
        if self._crls and self._keys is not None and self._access is not None:
            return
        await self._fetch_publications()
