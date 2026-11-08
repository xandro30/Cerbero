from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum


class KeyState(StrEnum):
    ACTIVE = "active"


@dataclass(frozen=True)
class ScopeKeyCreated:
    at: datetime
    identity: str
    key_version: int


@dataclass
class KeyVersion:
    number: int
    state: KeyState
    created_at: datetime


KEY_EVENTS: dict[str, type] = {"ScopeKeyCreated": ScopeKeyCreated}


class Key:
    """The scope key of an identity (Event Sourcing). Its rotation is out of scope: one version."""

    def __init__(self) -> None:
        self.identity: str
        self.versions: dict[int, KeyVersion] = {}
        self.stream_version = 0

    @classmethod
    def from_events(cls, events: list[ScopeKeyCreated]) -> "Key":
        if not events or not isinstance(events[0], ScopeKeyCreated):
            raise ValueError("a key stream starts with ScopeKeyCreated")
        key = cls()
        for event in events:
            key.apply(event)
        return key

    def apply(self, event: ScopeKeyCreated) -> None:
        self.identity = event.identity
        self.versions[event.key_version] = KeyVersion(event.key_version, KeyState.ACTIVE, event.at)
        self.stream_version += 1

    @property
    def current_version(self) -> int:
        return max(self.versions)

    @staticmethod
    def create(identity: str, at: datetime) -> list[ScopeKeyCreated]:
        return [ScopeKeyCreated(at, identity, 1)]
