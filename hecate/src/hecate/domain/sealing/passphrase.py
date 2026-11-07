from hecate.domain.sealing.errors import InvalidInput

MIN_LENGTH = 12


class Passphrase:
    """A new passphrase chosen by the admin; never shows its value."""

    __slots__ = ("_value",)

    def __init__(self, value: str) -> None:
        if not isinstance(value, str) or len(value) < MIN_LENGTH:
            raise InvalidInput(f"passphrase must have at least {MIN_LENGTH} characters")
        self._value = value

    @property
    def value(self) -> str:
        return self._value

    def __repr__(self) -> str:
        return "Passphrase(<hidden>)"

    __str__ = __repr__
