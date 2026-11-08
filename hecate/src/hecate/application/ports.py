from contextlib import AbstractContextManager
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Protocol

from hecate.domain.sealing.access_log import AccessLogEntry
from hecate.domain.sealing.attempts import AttemptPolicy
from hecate.domain.sealing.protection import SealConfig


class CryptoError(Exception):
    """A cryptographic operation failed (wrong key, altered data, not enough shares)."""


class CryptoProvider(Protocol):
    """Port over the cryptographic primitives; the first adapter is software-only."""

    def random_bytes(self, n: int) -> bytes: ...

    def derive_key(self, passphrase: str, salt: bytes, opslimit: int, memlimit: int) -> bytes:
        """Always receives its limits: the port has no default values."""

    def aead_encrypt(self, key: bytes, plaintext: bytes, aad: bytes) -> tuple[bytes, bytes]:
        """Return (nonce, ciphertext)."""

    def aead_decrypt(self, key: bytes, nonce: bytes, ciphertext: bytes, aad: bytes) -> bytes: ...

    def hmac_sha256(self, key: bytes, data: bytes) -> bytes: ...

    def hkdf_sha256(self, key: bytes, info: bytes, length: int) -> bytes: ...

    def shamir_split(self, secret32: bytes) -> list[tuple[int, bytes]]:
        """Split 32 bytes 2-of-3; return [(index, 32-byte fragments)]."""

    def shamir_combine(self, shares: list[tuple[int, bytes]]) -> bytes: ...


class SealRepository(Protocol):
    def load_config(self) -> SealConfig | None: ...

    def save_config(self, config: SealConfig) -> None: ...

    def load_attempts(self) -> AttemptPolicy: ...

    def save_attempts(self, attempts: AttemptPolicy) -> None: ...

    def transaction(self) -> AbstractContextManager[None]:
        """Group writes so that a failure leaves no changes."""


class AccessLog(Protocol):
    def append(self, entry: AccessLogEntry) -> None: ...


class Clock(Protocol):
    def now(self) -> datetime: ...


class ConcurrencyConflict(Exception):
    """An append used a stale expected version of the stream."""

    def __init__(self, current_version: int) -> None:
        super().__init__(f"the stream is at version {current_version}")
        self.current_version = current_version


@dataclass(frozen=True)
class EventRecord:
    """A stored event: its type, its JSON-able data (never a value or a key) and its instant."""

    type: str
    data: dict[str, Any]
    at: datetime


@dataclass(frozen=True)
class KeyMaterial:
    """A scope key wrapped by the root."""

    nonce: bytes
    ciphertext: bytes


@dataclass(frozen=True)
class SecretMaterial:
    """The encrypted value of one version and its version key wrapped by the scope key."""

    key_version: int
    value_nonce: bytes
    value_ciphertext: bytes
    dek_nonce: bytes
    dek_ciphertext: bytes


class EventStore(Protocol):
    def load(self, stream: str) -> list[EventRecord]: ...

    def append(self, stream: str, events: list[EventRecord], expected_version: int) -> int:
        """Append after `expected_version` events and return the new version; else ConcurrencyConflict."""

    def streams(self, prefix: str) -> list[str]: ...

    def transaction(self) -> AbstractContextManager[None]:
        """Group writes (events and material) so that a failure leaves no changes."""


class MaterialStore(Protocol):
    def save_secret_material(self, path: str, version: int, material: SecretMaterial) -> None: ...

    def load_secret_material(self, path: str, version: int) -> SecretMaterial | None: ...

    def delete_secret_material(self, path: str, version: int) -> None: ...

    def save_key_material(self, identity: str, key_version: int, material: KeyMaterial) -> None: ...

    def load_key_material(self, identity: str, key_version: int) -> KeyMaterial | None: ...

    def transaction(self) -> AbstractContextManager[None]: ...


class SecretStore(EventStore, MaterialStore, Protocol):
    """One adapter for events and material, with a single shared transaction()."""
