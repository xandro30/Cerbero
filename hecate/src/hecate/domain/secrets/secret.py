from dataclasses import dataclass
from datetime import datetime, timedelta
from enum import StrEnum

from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from hecate.domain.secrets.path import SecretPath
from hecate.domain.secrets.value_type import ValueType

DEFAULT_MAX_VERSIONS = 10
MIN_VERSIONS = 2  # so the version just rotated out keeps its grace period (SC-005)
MAX_VERSIONS_LIMIT = 100


class VersionState(StrEnum):
    ACTIVE = "active"
    DEPRECATED = "deprecated"
    REVOKED = "revoked"
    DESTROYED = "destroyed"


class DestroyReason(StrEnum):
    ADMIN = "admin"
    MAX_VERSIONS = "max_versions"


# -- events: they carry metadata only, never a value or a key ----------------------------------


@dataclass(frozen=True)
class SecretCreated:
    at: datetime
    path: str
    owner: str
    value_type: str
    max_versions: int
    rotate_every: str | None
    expires_at: datetime | None


@dataclass(frozen=True)
class VersionAdded:
    at: datetime
    version: int
    fingerprint: str
    key_version: int


@dataclass(frozen=True)
class VersionDeprecated:
    at: datetime
    version: int


@dataclass(frozen=True)
class VersionRevoked:
    at: datetime
    version: int


@dataclass(frozen=True)
class VersionRestored:
    at: datetime
    version: int


@dataclass(frozen=True)
class VersionPromoted:
    at: datetime
    version: int


@dataclass(frozen=True)
class VersionDestroyed:
    at: datetime
    version: int
    reason: str


SECRET_EVENTS: dict[str, type] = {
    cls.__name__: cls
    for cls in (
        SecretCreated, VersionAdded, VersionDeprecated, VersionRevoked,
        VersionRestored, VersionPromoted, VersionDestroyed,
    )
}

Event = (
    SecretCreated | VersionAdded | VersionDeprecated | VersionRevoked
    | VersionRestored | VersionPromoted | VersionDestroyed
)


@dataclass
class SecretVersion:
    number: int
    state: VersionState
    created_at: datetime
    deprecated_at: datetime | None
    fingerprint: str
    key_version: int


class Secret:
    """A secret with its versions. Its state is only what its events say (Event Sourcing).

    The decision methods return the events of a change without applying them; the caller
    stores them and applies them.
    """

    def __init__(self) -> None:
        self.path: SecretPath
        self.owner: str
        self.value_type: ValueType
        self.max_versions = DEFAULT_MAX_VERSIONS
        self.rotate_every: str | None = None
        self.expires_at: datetime | None = None
        self.versions: dict[int, SecretVersion] = {}
        self.stream_version = 0

    # -- rebuilding ----------------------------------------------------------------------------

    @classmethod
    def from_events(cls, events: list[Event]) -> "Secret":
        if not events or not isinstance(events[0], SecretCreated):
            raise ValueError("a secret stream starts with SecretCreated")
        secret = cls()
        for event in events:
            secret.apply(event)
        return secret

    def apply(self, event: Event) -> None:
        match event:
            case SecretCreated():
                self.path = SecretPath.parse(event.path)
                self.owner = event.owner
                self.value_type = ValueType(event.value_type)
                self.max_versions = event.max_versions
                self.rotate_every = event.rotate_every
                self.expires_at = event.expires_at
            case VersionAdded():
                self.versions[event.version] = SecretVersion(
                    event.version, VersionState.ACTIVE, event.at, None, event.fingerprint, event.key_version
                )
            case VersionDeprecated() | VersionRestored():
                version = self.versions[event.version]
                version.state = VersionState.DEPRECATED
                version.deprecated_at = event.at
            case VersionRevoked():
                self.versions[event.version].state = VersionState.REVOKED
            case VersionPromoted():
                version = self.versions[event.version]
                version.state = VersionState.ACTIVE
                version.deprecated_at = None
            case VersionDestroyed():
                self.versions[event.version].state = VersionState.DESTROYED
        self.stream_version += 1

    # -- reading -------------------------------------------------------------------------------

    def state_of(self, number: int, now: datetime, grace: timedelta) -> VersionState:
        """Effective state: a `deprecated` version past its grace period counts as `revoked`."""
        version = self.versions.get(number)
        if version is None:
            raise NotFound("version not found")
        if (
            version.state is VersionState.DEPRECATED
            and version.deprecated_at is not None
            and now >= version.deprecated_at + grace
        ):
            return VersionState.REVOKED
        return version.state

    def active_version(self) -> int | None:
        for number, version in self.versions.items():
            if version.state is VersionState.ACTIVE:
                return number
        return None

    def live_versions(self) -> list[int]:
        return sorted(n for n, v in self.versions.items() if v.state is not VersionState.DESTROYED)

    # -- deciding changes ----------------------------------------------------------------------

    @classmethod
    def create(
        cls,
        path: SecretPath,
        value_type: ValueType,
        at: datetime,
        fingerprint: str,
        key_version: int,
        *,
        max_versions: int = DEFAULT_MAX_VERSIONS,
        rotate_every: str | None = None,
        expires_at: datetime | None = None,
    ) -> list[Event]:
        if isinstance(max_versions, bool) or not isinstance(max_versions, int) \
                or not MIN_VERSIONS <= max_versions <= MAX_VERSIONS_LIMIT:
            raise InvalidInput(f"max_versions must be a number from {MIN_VERSIONS} to {MAX_VERSIONS_LIMIT}")
        return [
            SecretCreated(at, str(path), path.owner, value_type.value, max_versions, rotate_every, expires_at),
            VersionAdded(at, 1, fingerprint, key_version),
        ]

    def add_version(self, at: datetime, fingerprint: str, key_version: int) -> list[Event]:
        number = max(self.versions, default=0) + 1
        events: list[Event] = []
        active = self.active_version()
        if active is not None:
            events.append(VersionDeprecated(at, active))
        events.append(VersionAdded(at, number, fingerprint, key_version))
        live = self.live_versions() + [number]
        if len(live) > self.max_versions:
            events.append(VersionDestroyed(at, live[0], DestroyReason.MAX_VERSIONS.value))
        return events

    def _require(self, number: int, now: datetime, grace: timedelta, allowed: set[VersionState]) -> VersionState:
        state = self.state_of(number, now, grace)
        if state not in allowed:
            raise StateConflict(message=f"version {number} is {state.value}: the change is not allowed")
        return state

    def revoke(self, number: int, now: datetime, grace: timedelta) -> list[Event]:
        self._require(number, now, grace, {VersionState.ACTIVE, VersionState.DEPRECATED})
        return [VersionRevoked(now, number)]

    def restore(self, number: int, now: datetime, grace: timedelta) -> list[Event]:
        self._require(number, now, grace, {VersionState.REVOKED})
        return [VersionRestored(now, number)]

    def promote(self, number: int, now: datetime, grace: timedelta) -> list[Event]:
        self._require(number, now, grace, {VersionState.DEPRECATED})
        events: list[Event] = []
        active = self.active_version()
        if active is not None:
            events.append(VersionDeprecated(now, active))
        events.append(VersionPromoted(now, number))
        return events

    def destroy(self, number: int, now: datetime, grace: timedelta) -> list[Event]:
        self._require(number, now, grace, {VersionState.REVOKED})
        return [VersionDestroyed(now, number, DestroyReason.ADMIN.value)]
