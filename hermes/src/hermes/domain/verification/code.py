"""The one-time code that verifies a channel (FR-022, research R12).

Eight characters of the alphabet of Crockford without I, L, O or U (40 bits), from `secrets`; shown as `XXXX-XXXX` and
compared without case, spaces or hyphens. Only its SHA-256 is kept, with the time it expires.
"""

from __future__ import annotations

import hashlib
import re
import secrets
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime, timedelta

ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
LENGTH = 8
MIN_MINUTES = 5
MAX_MINUTES = 1440
_SEPARATORS = re.compile(r"[\s-]+")


@dataclass(frozen=True)
class VerificationCode:
    value: str = field(repr=False)

    @classmethod
    def new(cls, choice: Callable[[str], str] = secrets.choice) -> VerificationCode:
        return cls("".join(choice(ALPHABET) for _ in range(LENGTH)))

    @property
    def shown(self) -> str:
        return f"{self.value[:4]}-{self.value[4:]}"

    @property
    def digest(self) -> bytes:
        return hashlib.sha256(self.value.encode("ascii")).digest()


def normalized(text: str) -> str | None:
    """The code in a text, without case, spaces or hyphens; None if the text is not a code."""
    candidate = _SEPARATORS.sub("", text).upper()
    if len(candidate) != LENGTH or any(char not in ALPHABET for char in candidate):
        return None
    return candidate


def digest_of(text: str) -> bytes | None:
    code = normalized(text)
    return VerificationCode(code).digest if code is not None else None


def expires_at(now: datetime, minutes: int) -> datetime:
    if not MIN_MINUTES <= minutes <= MAX_MINUTES:
        raise ValueError(f"a code lasts from {MIN_MINUTES} to {MAX_MINUTES} minutes")
    return now + timedelta(minutes=minutes)
