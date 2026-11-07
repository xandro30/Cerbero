from contextlib import AbstractContextManager
from datetime import datetime
from typing import Protocol

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
