import json
from dataclasses import dataclass
from datetime import datetime
from typing import Literal

AEAD_NAME = "AES-256-GCM"
KDF_NAME = "argon2id"
SHARE_THRESHOLD = 2
SHARE_COUNT = 3


def associated_data(protection: Literal["passphrase", "recovery"], version: int) -> bytes:
    """AEAD associated data binding a ciphertext to its purpose, protection and version."""
    return json.dumps(
        {"purpose": "root", "protection": protection, "config_version": version},
        sort_keys=True,
        separators=(",", ":"),
    ).encode()


@dataclass(frozen=True)
class PassphraseProtection:
    """The root encrypted with a key derived from the admin's passphrase."""

    version: int  # config_version in force when this protection was written (used in the AAD)
    kdf: str
    kdf_opslimit: int
    kdf_memlimit: int
    salt: bytes
    aead: str
    nonce: bytes
    ciphertext: bytes

    def aad(self) -> bytes:
        return associated_data("passphrase", self.version)


@dataclass(frozen=True)
class RecoveryProtection:
    """The root encrypted with the recovery key that is split into Shamir shares."""

    version: int
    share_set_id: bytes
    threshold: int
    share_count: int
    aead: str
    nonce: bytes
    ciphertext: bytes

    def aad(self) -> bytes:
        return associated_data("recovery", self.version)


@dataclass(frozen=True)
class SealConfig:
    config_version: int
    initialized_at: datetime
    updated_at: datetime
    passphrase: PassphraseProtection
    recovery: RecoveryProtection
