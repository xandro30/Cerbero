from dataclasses import dataclass
from datetime import timedelta


@dataclass(frozen=True)
class KdfLimits:
    """Argon2id cost limits; stored with every passphrase protection so they can be raised later."""

    opslimit: int
    memlimit: int


# libsodium's SENSITIVE profile (4 passes, 1 GiB) and its MODERATE profile (3 passes, 256 MiB).
KDF_SENSITIVE = KdfLimits(opslimit=4, memlimit=1 << 30)
KDF_MODERATE = KdfLimits(opslimit=3, memlimit=256 << 20)


@dataclass(frozen=True)
class SealingSettings:
    kdf: KdfLimits
    pending_ttl: timedelta = timedelta(minutes=15)
