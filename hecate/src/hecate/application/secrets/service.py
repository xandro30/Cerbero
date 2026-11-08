from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Any

from hecate.application.ports import (
    AccessLog,
    ConcurrencyConflict,
    Clock,
    CryptoProvider,
    EventRecord,
    SecretStore,
)
from hecate.application.sealing.service import SealingService
from hecate.application.secrets.envelope import Envelope
from hecate.domain.events import decode_event, encode_event
from hecate.domain.keys.key import KEY_EVENTS, Key
from hecate.domain.sealing.access_log import AccessLogEntry, Method, Operation, Result
from hecate.domain.sealing.errors import (
    AuthenticationFailed,
    HecateError,
    InvalidInput,
    NotFound,
    RateLimited,
    StateConflict,
)
from hecate.domain.secrets.path import SecretPath, SecretReference, parse_identity
from hecate.domain.secrets.secret import SECRET_EVENTS, Secret, VersionAdded, VersionDestroyed, VersionState
from hecate.domain.secrets.value_type import ValueType

ADMIN = "admin"
MAX_ROTATE_EVERY = 64
MAX_REFERENCES = 100
# One message for every reason a reference cannot be resolved (FR-013, FR-014, SC-007).
NOT_RESOLVED = "secret not found"

_RESULT_FOR_ERROR: tuple[tuple[type[Exception], Result], ...] = (
    (InvalidInput, Result.INVALID_INPUT),
    (StateConflict, Result.REJECTED_CONFLICT),
    (RateLimited, Result.REJECTED_RATE_LIMITED),
    (AuthenticationFailed, Result.FAILURE),
    (NotFound, Result.NOT_FOUND),
)


@dataclass(frozen=True)
class SecretsSettings:
    grace: timedelta = timedelta(hours=24)
    default_max_versions: int = 10
    cache_ttl_soft_s: int = 300
    cache_ttl_hard_s: int = 3600
    expiring_soon: timedelta = timedelta(days=7)


@dataclass
class _Trace:
    """What an operation tells the access log about itself (never a value or a fingerprint)."""

    path: str | None = None
    version: int | None = None


def _result_for(exc: Exception) -> Result:
    for kind, result in _RESULT_FOR_ERROR:
        if isinstance(exc, kind):
            return result
    return Result.FAILURE


def _instant(value: object) -> datetime | None:
    """An optional ISO 8601 instant; a naive one is read as UTC."""
    if value is None:
        return None
    if not isinstance(value, str):
        raise InvalidInput("expires_at must be an ISO 8601 date and time")
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError:
        raise InvalidInput("expires_at must be an ISO 8601 date and time") from None
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=UTC)


class SecretsService:
    """Stores, versions and resolves secrets; every operation needs Hécate unsealed (FR-015)."""

    def __init__(
        self,
        *,
        sealing: SealingService,
        store: SecretStore,
        crypto: CryptoProvider,
        access_log: AccessLog,
        clock: Clock,
        settings: SecretsSettings | None = None,
    ) -> None:
        self._sealing = sealing
        self._store = store
        self._access_log = access_log
        self._clock = clock
        self._settings = settings or SecretsSettings()
        self._envelope = Envelope(crypto)

    # -- shared helpers -----------------------------------------------------------------------

    def _now(self) -> datetime:
        return self._clock.now()

    def _log(
        self, operation: Operation, subject: str, method: Method, result: Result,
        path: str | None, version: int | None, detail: str,
    ) -> None:
        self._access_log.append(
            AccessLogEntry(self._now(), operation, method, result, detail, subject, path, version)
        )

    @staticmethod
    def _loggable_path(text: object) -> str | None:
        """Only a well-formed path reaches the log: a malformed one could be a pasted value."""
        try:
            return str(SecretPath.parse(text))
        except InvalidInput:
            return None

    @contextmanager
    def _audited(
        self, operation: Operation, *, path: object = None, subject: str = ADMIN,
        method: Method = Method.LOCAL, detail: str = "",
    ) -> Iterator[_Trace]:
        """Leave a row for the operation: its success or why it was refused."""
        trace = _Trace(self._loggable_path(path) if path is not None else None)
        try:
            yield trace
        except Exception as exc:
            self._log(operation, subject, method, _result_for(exc), trace.path, trace.version, detail)
            raise
        self._log(operation, subject, method, Result.SUCCESS, trace.path, trace.version, detail)

    def record_invalid_request(self, operation: Operation, detail: str) -> None:
        """Leave a row for a recognized operation whose arguments were malformed."""
        self._log(operation, ADMIN, Method.LOCAL, Result.INVALID_INPUT, None, None, detail)

    def _root(self) -> bytes:
        return self._sealing.root()

    def _load_secret(self, path: SecretPath) -> Secret | None:
        records = self._store.load(path.stream_name)
        if not records:
            return None
        return Secret.from_events([decode_event(SECRET_EVENTS, r.type, r.data, r.at) for r in records])

    def _require_secret(self, path: SecretPath) -> Secret:
        secret = self._load_secret(path)
        if secret is None:
            raise NotFound("secret not found")
        return secret

    def _append(self, stream: str, events: list[Any], expected_version: int) -> int:
        records = [EventRecord(*encode_event(event)) for event in events]
        try:
            return self._store.append(stream, records, expected_version)
        except ConcurrencyConflict as exc:
            raise StateConflict(
                message="the secret changed since it was read", current_version=exc.current_version
            ) from None

    @staticmethod
    def _check_expected(secret: Secret, expected_version: object) -> None:
        if isinstance(expected_version, bool) or not isinstance(expected_version, int):
            raise InvalidInput("expected_version is required and must be a number")
        if expected_version != secret.stream_version:
            raise StateConflict(
                message="the secret changed since it was read", current_version=secret.stream_version
            )

    def _scope_key(self, root: bytes, identity: str, now: datetime) -> tuple[bytes, int]:
        """The scope key of an identity and its version; created the first time (FR-018)."""
        stream = f"key/{identity}"
        records = self._store.load(stream)
        if records:
            key = Key.from_events([decode_event(KEY_EVENTS, r.type, r.data, r.at) for r in records])
            material = self._store.load_key_material(identity, key.current_version)
            assert material is not None, "a scope key without material"
            return self._envelope.open_scope_key(root, identity, key.current_version, material), key.current_version
        events = Key.create(identity, now)
        scope_key, material = self._envelope.new_scope_key(root, identity, 1)
        self._append(stream, events, 0)
        self._store.save_key_material(identity, 1, material)
        return scope_key, 1

    def _effective_state(self, secret: Secret, number: int, now: datetime) -> VersionState:
        return secret.state_of(number, now, self._settings.grace)

    def _secret_from_stream(self, stream: str) -> Secret:
        records = self._store.load(stream)
        return Secret.from_events([decode_event(SECRET_EVENTS, r.type, r.data, r.at) for r in records])

    def _max_versions(self, requested: object) -> int:
        if requested is None:
            return self._settings.default_max_versions
        return requested  # type: ignore[return-value]  # the aggregate validates it

    @staticmethod
    def _rotate_every(requested: object) -> str | None:
        if requested is None:
            return None
        if not isinstance(requested, str) or not 1 <= len(requested) <= MAX_ROTATE_EVERY \
                or not requested.isprintable():
            raise InvalidInput(f"rotate_every must be a text of 1 to {MAX_ROTATE_EVERY} characters")
        return requested

    # -- writing a new secret (FR-001, FR-002, FR-018) -----------------------------------------

    def create(
        self,
        path: str,
        value: str,
        value_type: str,
        *,
        max_versions: int | None = None,
        rotate_every: str | None = None,
        expires_at: str | None = None,
    ) -> dict[str, Any]:
        with self._audited(Operation.SECRET_CREATE, path=path, detail="create") as trace:
            root = self._root()
            secret_path = SecretPath.parse(path)
            kind = ValueType.parse(value_type)
            data = kind.validate(value)
            limit = self._max_versions(max_versions)
            rotation = self._rotate_every(rotate_every)
            expiry = _instant(expires_at)
            now = self._now()
            text = str(secret_path)
            with self._store.transaction():
                if self._store.load(secret_path.stream_name):
                    raise StateConflict(message="a secret already exists at this path")
                scope_key, key_version = self._scope_key(root, secret_path.owner, now)
                fingerprint = self._envelope.fingerprint(scope_key, data)
                events = Secret.create(
                    secret_path, kind, now, fingerprint, key_version,
                    max_versions=limit, rotate_every=rotation, expires_at=expiry,
                )
                material = self._envelope.seal_value(scope_key, text, 1, secret_path.owner, key_version, data)
                stream_version = self._append(secret_path.stream_name, events, 0)
                self._store.save_secret_material(text, 1, material)
            trace.version = 1
        return {"path": text, "version": 1, "fingerprint": fingerprint, "stream_version": stream_version}

    # -- consulting (FR-019, FR-020, FR-021) ---------------------------------------------------

    def list_secrets(self, identity: str | None = None) -> dict[str, Any]:
        with self._audited(Operation.SECRET_LIST, detail="list"):
            self._root()
            prefix = "secret/" if identity is None else f"secret/{parse_identity(identity)}/"
            items = []
            for stream in self._store.streams(prefix):
                secret = self._secret_from_stream(stream)
                items.append({
                    "path": str(secret.path),
                    "value_type": secret.value_type.value,
                    "active_version": secret.active_version(),
                    "version_count": len(secret.live_versions()),
                    "rotate_every": secret.rotate_every,
                    "expires_at": secret.expires_at.isoformat() if secret.expires_at else None,
                })
        return {"secrets": sorted(items, key=lambda item: item["path"])}

    def get(self, path: str) -> dict[str, Any]:
        with self._audited(Operation.SECRET_GET, path=path, detail="get"):
            self._root()
            secret = self._require_secret(SecretPath.parse(path))
            now = self._now()
            versions = [
                {
                    "version": number,
                    "state": self._effective_state(secret, number, now).value,
                    "created_at": version.created_at.isoformat(),
                    "deprecated_at": version.deprecated_at.isoformat() if version.deprecated_at else None,
                    "fingerprint": version.fingerprint,
                }
                for number, version in sorted(secret.versions.items())
            ]
            return {
                "path": str(secret.path),
                "owner": secret.owner,
                "value_type": secret.value_type.value,
                "max_versions": secret.max_versions,
                "rotate_every": secret.rotate_every,
                "expires_at": secret.expires_at.isoformat() if secret.expires_at else None,
                "stream_version": secret.stream_version,
                "versions": versions,
            }

    # -- resolving, by an identity (FR-011 .. FR-014) ---------------------------------------------

    def resolve(self, identity: str, references: list[str]) -> dict[str, Any]:
        """Resolve secrets of `identity`; all or nothing, with one `not_found` for every refusal."""
        subject: str | None = None
        requested: str | None = None
        try:
            subject = parse_identity(identity)
            root = self._root()
            refs = self._parse_references(references)
            now = self._now()
            scope_keys: dict[int, bytes] = {}
            resolved = []
            for ref in refs:
                requested = str(ref.path)
                resolved.append(self._resolve_one(root, subject, ref, now, scope_keys))
        except Exception as exc:
            self._log(Operation.SECRET_RESOLVE, subject, Method.IDENTITY, _result_for(exc), requested, None, "resolve")
            raise
        for item in resolved:
            self._log(
                Operation.SECRET_RESOLVE, subject, Method.IDENTITY, Result.SUCCESS,
                item["path"], item["version"], "resolve",
            )
        return {"secrets": resolved}

    @staticmethod
    def _parse_references(references: object) -> list[SecretReference]:
        if not isinstance(references, list) or not 1 <= len(references) <= MAX_REFERENCES:
            raise InvalidInput(f"send a list of 1 to {MAX_REFERENCES} references")
        return [SecretReference.parse(text) for text in references]

    def _resolve_one(
        self, root: bytes, identity: str, ref: SecretReference, now: datetime, scope_keys: dict[int, bytes],
    ) -> dict[str, Any]:
        if ref.path.owner != identity:
            raise NotFound(NOT_RESOLVED)
        secret = self._load_secret(ref.path)
        if secret is None:
            raise NotFound(NOT_RESOLVED)
        number = ref.version if ref.version is not None else secret.active_version()
        if number is None or number not in secret.versions:
            raise NotFound(NOT_RESOLVED)
        if self._effective_state(secret, number, now) not in (VersionState.ACTIVE, VersionState.DEPRECATED):
            raise NotFound(NOT_RESOLVED)
        text = str(ref.path)
        material = self._store.load_secret_material(text, number)
        if material is None:
            raise NotFound(NOT_RESOLVED)
        if material.key_version not in scope_keys:
            wrapped = self._store.load_key_material(identity, material.key_version)
            if wrapped is None:
                raise NotFound(NOT_RESOLVED)
            scope_keys[material.key_version] = self._envelope.open_scope_key(
                root, identity, material.key_version, wrapped
            )
        value = self._envelope.open_value(scope_keys[material.key_version], text, number, identity, material)
        return {
            "path": text,
            "version": number,
            "value": value.decode(),
            "cache_ttl_soft_s": self._settings.cache_ttl_soft_s,
            "cache_ttl_hard_s": self._settings.cache_ttl_hard_s,
        }

    # -- rotating: a new version (FR-006, FR-007, FR-010, FR-017) -----------------------------------

    @staticmethod
    def _version_number(version: object) -> int:
        if isinstance(version, bool) or not isinstance(version, int) or version < 1:
            raise InvalidInput("version must be a number of 1 or more")
        return version

    def add_version(self, path: str, value: str, expected_version: int) -> dict[str, Any]:
        with self._audited(Operation.SECRET_ADD_VERSION, path=path, detail="add_version") as trace:
            root = self._root()
            secret_path = SecretPath.parse(path)
            now = self._now()
            text = str(secret_path)
            with self._store.transaction():
                secret = self._require_secret(secret_path)
                self._check_expected(secret, expected_version)
                data = secret.value_type.validate(value)
                scope_key, key_version = self._scope_key(root, secret.owner, now)
                events = secret.add_version(now, self._envelope.fingerprint(scope_key, data), key_version)
                added = next(e for e in events if isinstance(e, VersionAdded))
                material = self._envelope.seal_value(
                    scope_key, text, added.version, secret.owner, key_version, data
                )
                stream_version = self._append(secret_path.stream_name, events, secret.stream_version)
                self._store.save_secret_material(text, added.version, material)
                for event in events:
                    if isinstance(event, VersionDestroyed):  # the oldest one, over the maximum
                        self._store.delete_secret_material(text, event.version)
            trace.version = added.version
        return {"version": added.version, "fingerprint": added.fingerprint, "stream_version": stream_version}

    # -- revoking, restoring and promoting (FR-008) ----------------------------------------------

    def _transition(
        self, operation: Operation, name: str, path: str, version: int, expected_version: int,
        decide: Callable[[Secret, int, datetime], list[Any]],
    ) -> dict[str, Any]:
        with self._audited(operation, path=path, detail=name) as trace:
            self._root()
            secret_path = SecretPath.parse(path)
            number = trace.version = self._version_number(version)
            now = self._now()
            with self._store.transaction():
                secret = self._require_secret(secret_path)
                self._check_expected(secret, expected_version)
                events = decide(secret, number, now)
                stream_version = self._append(secret_path.stream_name, events, secret.stream_version)
        return {"stream_version": stream_version}

    def revoke(self, path: str, version: int, expected_version: int) -> dict[str, Any]:
        return self._transition(
            Operation.SECRET_REVOKE, "revoke", path, version, expected_version,
            lambda secret, number, now: secret.revoke(number, now, self._settings.grace),
        )

    def restore(self, path: str, version: int, expected_version: int) -> dict[str, Any]:
        return self._transition(
            Operation.SECRET_RESTORE, "restore", path, version, expected_version,
            lambda secret, number, now: secret.restore(number, now, self._settings.grace),
        )

    def promote(self, path: str, version: int, expected_version: int) -> dict[str, Any]:
        return self._transition(
            Operation.SECRET_PROMOTE, "promote", path, version, expected_version,
            lambda secret, number, now: secret.promote(number, now, self._settings.grace),
        )

    # -- destroying: cryptographic erasure of a revoked version (FR-009) ---------------------------

    def destroy(self, path: str, version: int, expected_version: int, proof: Any) -> dict[str, Any]:
        method = SealingService.proof_method(proof)
        with self._audited(Operation.SECRET_DESTROY, path=path, method=method, detail="destroy") as trace:
            self._root()
            secret_path = SecretPath.parse(path)
            number = trace.version = self._version_number(version)
            now = self._now()
            secret = self._require_secret(secret_path)
            self._check_expected(secret, expected_version)
            secret.destroy(number, now, self._settings.grace)  # refuses before any attempt is spent
            self._sealing.verify_proof(proof)
            with self._store.transaction():
                secret = self._require_secret(secret_path)
                self._check_expected(secret, expected_version)
                events = secret.destroy(number, now, self._settings.grace)
                stream_version = self._append(secret_path.stream_name, events, secret.stream_version)
                self._store.delete_secret_material(str(secret_path), number)
        return {"stream_version": stream_version}

    # -- expiry counts for the state of Hécate (FR-022) -----------------------------------------------

    def expiry_counts(self, now: datetime | None = None) -> dict[str, int]:
        """How many secrets have expired or expire soon; metadata only, so it works sealed too."""
        now = now or self._now()
        horizon = now + self._settings.expiring_soon
        expired = expiring_soon = 0
        for stream in self._store.streams("secret/"):
            secret = self._secret_from_stream(stream)
            if secret.expires_at is None or not secret.live_versions():
                continue
            if secret.expires_at <= now:
                expired += 1
            elif secret.expires_at <= horizon:
                expiring_soon += 1
        return {"secrets_expired": expired, "secrets_expiring_soon": expiring_soon}
