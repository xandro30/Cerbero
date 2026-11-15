"""References to secrets and SPIFFE IDs, with the rules of Hécate (docs/hecate/secretos.md, identidades.md)."""

from __future__ import annotations

import re
from dataclasses import dataclass

from hermes.domain.shared.errors import ErrorCode, HermesError

IDENTITY = r"[a-z][a-z0-9-]{0,62}"
SEGMENT = r"[a-z0-9_-]{1,64}"
TRUST_DOMAIN = r"[a-z0-9._-]{1,255}"

_REFERENCE = re.compile(rf"secret://({IDENTITY})/({SEGMENT}(?:/{SEGMENT})*)(?:@([1-9][0-9]{{0,8}}))?")
_SPIFFE = re.compile(rf"spiffe://({TRUST_DOMAIN})/({IDENTITY})")


@dataclass(frozen=True)
class SecretReference:
    """`secret://<identity>/<path>` with an optional `@<version>`; it names a secret, it never holds its value."""

    identity: str
    path: tuple[str, ...]
    version: int | None = None

    @classmethod
    def parse(cls, value: str) -> SecretReference:
        match = _REFERENCE.fullmatch(value)
        if match is None:
            raise HermesError(ErrorCode.VALIDATION, "a secret must be a secret:// reference with the rules of Hécate")
        version = int(match.group(3)) if match.group(3) else None
        return cls(identity=match.group(1), path=tuple(match.group(2).split("/")), version=version)

    def __str__(self) -> str:
        suffix = f"@{self.version}" if self.version is not None else ""
        return f"secret://{self.identity}/{'/'.join(self.path)}{suffix}"


@dataclass(frozen=True)
class SpiffeId:
    """`spiffe://<trust domain>/<identity>`, the only name of a Cerbero certificate."""

    trust_domain: str
    identity: str

    @classmethod
    def parse(cls, value: str) -> SpiffeId:
        match = _SPIFFE.fullmatch(value)
        if match is None:
            raise HermesError(ErrorCode.VALIDATION, "not a SPIFFE ID of Cerbero")
        return cls(trust_domain=match.group(1), identity=match.group(2))

    def __str__(self) -> str:
        return f"spiffe://{self.trust_domain}/{self.identity}"
