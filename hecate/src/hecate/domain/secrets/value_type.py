import base64
import binascii
import json
import re
from enum import StrEnum

from hecate.domain.sealing.errors import InvalidInput

MAX_VALUE_BYTES = 64 * 1024

_PEM_BLOCK = re.compile(
    r"-----BEGIN ([A-Z0-9 ]+)-----\s*\n(?P<body>[A-Za-z0-9+/=\s]+?)\s*\n-----END \1-----"
)


class ValueType(StrEnum):
    STRING = "string"
    JSON = "json"
    PEM = "pem"

    @classmethod
    def parse(cls, text: object) -> "ValueType":
        try:
            return cls(text)
        except ValueError:
            raise InvalidInput("the value type must be 'string', 'json' or 'pem'") from None

    def validate(self, value: object) -> bytes:
        """Check a value for this type and return its UTF-8 bytes."""
        if not isinstance(value, str):
            raise InvalidInput("the value must be a text")
        try:
            data = value.encode()
        except UnicodeEncodeError:
            raise InvalidInput("the value is not valid UTF-8 text") from None
        if not data:
            raise InvalidInput("the value must not be empty")
        if len(data) > MAX_VALUE_BYTES:
            raise InvalidInput(f"the value is larger than {MAX_VALUE_BYTES // 1024} KiB")
        if self is ValueType.JSON:
            try:
                json.loads(value)
            except ValueError:
                raise InvalidInput("the value is not valid JSON") from None
        elif self is ValueType.PEM:
            _check_pem(value)
        return data


def _check_pem(value: str) -> None:
    position = 0
    blocks = 0
    text = value.strip()
    while position < len(text):
        match = _PEM_BLOCK.match(text, position)
        if match is None:
            raise InvalidInput("the value is not a valid PEM")
        try:
            base64.b64decode("".join(match["body"].split()), validate=True)
        except (binascii.Error, ValueError):
            raise InvalidInput("a PEM block does not contain valid base64") from None
        blocks += 1
        position = match.end()
        while position < len(text) and text[position].isspace():
            position += 1
    if blocks == 0:
        raise InvalidInput("the value is not a valid PEM")
