import hashlib
import re
from dataclasses import dataclass

from hecate.domain.sealing.errors import InvalidInput

PREFIX = "HEC1"
VERSION = 1
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"  # Crockford Base32
_DECODE = {c: i for i, c in enumerate(ALPHABET)}
_CONFUSED = {"O": "0", "I": "1", "L": "1"}
_CHECKSUM_LEN = 4
_SET_ID_LEN = 4
_FRAGMENTS_LEN = 32
_BODY_LEN = 1 + 1 + _SET_ID_LEN + _FRAGMENTS_LEN  # version, index, set id, fragments


def _b32_encode(data: bytes) -> str:
    bits = int.from_bytes(data, "big")
    total = len(data) * 8
    pad = (-total) % 5
    bits <<= pad
    chars = (total + pad) // 5
    return "".join(ALPHABET[(bits >> (5 * (chars - 1 - i))) & 31] for i in range(chars))


def _b32_decode(text: str, length: int) -> bytes:
    bits = 0
    for ch in text:
        bits = (bits << 5) | _DECODE[ch]
    total = len(text) * 5
    pad = total - length * 8
    if pad < 0 or pad >= 5:
        raise InvalidInput("recovery share has an invalid length")
    if bits & ((1 << pad) - 1):
        raise InvalidInput("recovery share is not valid: check how it was copied")
    return (bits >> pad).to_bytes(length, "big")


@dataclass(frozen=True)
class RecoveryShare:
    """One of the three recovery shares; shown once, never stored."""

    index: int
    share_set_id: bytes
    fragments: bytes

    def __post_init__(self) -> None:
        if not 1 <= self.index <= 3:
            raise InvalidInput("recovery share index must be 1, 2 or 3")
        if len(self.share_set_id) != _SET_ID_LEN or len(self.fragments) != _FRAGMENTS_LEN:
            raise InvalidInput("recovery share has an invalid length")

    def _body(self) -> bytes:
        return bytes([VERSION, self.index]) + self.share_set_id + self.fragments

    def encode(self) -> str:
        body = self._body()
        checksum = hashlib.sha256(body).digest()[:_CHECKSUM_LEN]
        text = _b32_encode(body + checksum)
        groups = [text[i : i + 4] for i in range(0, len(text), 4)]
        return PREFIX + "-" + "-".join(groups)

    @classmethod
    def parse(cls, text: str) -> "RecoveryShare":
        if not isinstance(text, str):
            raise InvalidInput("recovery share must be text")
        cleaned = "".join(_CONFUSED.get(c, c) for c in re.sub(r"[\s-]", "", text).upper())
        if not cleaned.startswith(PREFIX):
            raise InvalidInput("recovery share must start with HEC1")
        body_text = cleaned[len(PREFIX) :]
        if any(c not in _DECODE for c in body_text):
            raise InvalidInput("recovery share has invalid characters")
        raw = _b32_decode(body_text, _BODY_LEN + _CHECKSUM_LEN)
        body, checksum = raw[:_BODY_LEN], raw[_BODY_LEN:]
        if hashlib.sha256(body).digest()[:_CHECKSUM_LEN] != checksum:
            raise InvalidInput("recovery share checksum does not match: check how it was copied")
        if body[0] != VERSION:
            raise InvalidInput("unsupported recovery share version")
        return cls(index=body[1], share_set_id=body[2:6], fragments=body[6:])

    def __repr__(self) -> str:
        return f"RecoveryShare(index={self.index}, <hidden>)"
