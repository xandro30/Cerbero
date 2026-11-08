import re
from dataclasses import dataclass

from hecate.domain.sealing.errors import InvalidInput

SCHEME = "secret://"
_IDENTITY = re.compile(r"[a-z][a-z0-9-]{0,62}")
_SEGMENT = re.compile(r"[a-z0-9_-]{1,64}")
_VERSION = re.compile(r"[1-9][0-9]{0,8}")


def parse_identity(text: object) -> str:
    if not isinstance(text, str) or not _IDENTITY.fullmatch(text):
        raise InvalidInput("the identity must be lowercase letters, digits and '-', starting with a letter")
    return text


@dataclass(frozen=True)
class SecretPath:
    """`secret://<identity>/<segment>[/<segment>...]`; the owner is the first segment."""

    owner: str
    segments: tuple[str, ...]

    @classmethod
    def parse(cls, text: object) -> "SecretPath":
        if not isinstance(text, str) or not text.startswith(SCHEME):
            raise InvalidInput(f"a secret path starts with {SCHEME}")
        parts = text[len(SCHEME):].split("/")
        if len(parts) < 2:
            raise InvalidInput("a secret path is secret://<identity>/<path>")
        owner, *segments = parts
        if not _IDENTITY.fullmatch(owner):
            raise InvalidInput("the identity must be lowercase letters, digits and '-', starting with a letter")
        if not all(_SEGMENT.fullmatch(s) for s in segments):
            raise InvalidInput("path segments are 1-64 characters of a-z, 0-9, '_' and '-'")
        return cls(owner, tuple(segments))

    @property
    def relative(self) -> str:
        return "/".join((self.owner, *self.segments))

    @property
    def stream_name(self) -> str:
        return f"secret/{self.relative}"

    def __str__(self) -> str:
        return SCHEME + self.relative


@dataclass(frozen=True)
class SecretReference:
    """A path with an optional explicit version: `secret://<identity>/<path>[@<n>]`."""

    path: SecretPath
    version: int | None = None

    @classmethod
    def parse(cls, text: object) -> "SecretReference":
        if not isinstance(text, str):
            raise InvalidInput("a secret reference must be a text")
        base, at, version = text.partition("@")
        if not at:
            return cls(SecretPath.parse(base))
        if not _VERSION.fullmatch(version):
            raise InvalidInput("the version after '@' must be a number of 1 or more")
        return cls(SecretPath.parse(base), int(version))
