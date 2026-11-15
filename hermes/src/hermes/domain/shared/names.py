"""The names of contacts, instances, templates and variables."""

import re

from hermes.domain.shared.errors import ErrorCode, HermesError

NAME_PATTERN = re.compile(r"[a-z][a-z0-9_]{0,62}")


def is_name(value: str) -> bool:
    return NAME_PATTERN.fullmatch(value) is not None


def require_name(value: str, what: str) -> str:
    if not is_name(value):
        raise HermesError(ErrorCode.VALIDATION, f"the {what} name must follow ^[a-z][a-z0-9_]{{0,62}}$")
    return value
