"""The substitution in the texts of a template: `${data.<path>}` and `${vars.<name>}`, without logic (research R8).

It is the syntax of the expressions of the Playbooks. A value is written as it is and never substituted again; a value
that is not text is written as its JSON.
"""

from __future__ import annotations

import json
import re
from collections.abc import Collection, Mapping
from typing import Any

from hermes.domain.shared.errors import ErrorCode, HermesError

OPEN = "${"
PLACEHOLDER = re.compile(r"\$\{(?:data((?:\.[A-Za-z0-9_-]+)+)|vars\.([a-z][a-z0-9_]{0,62}))\}")
SHOWN = 80


class TemplateError(HermesError):
    """A placeholder that cannot be filled: its message names the placeholder, never the data."""

    def __init__(self, reason: str) -> None:
        super().__init__(ErrorCode.VALIDATION, reason)
        self.reason = reason


def _fragment(text: str, start: int) -> str:
    end = text.find("}", start)
    fragment = text[start:] if end == -1 else text[start : end + 1]
    return fragment if len(fragment) <= SHOWN else fragment[:SHOWN] + "…"


def _written(value: Any) -> str:
    return value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)


def _data(path: str, data: Mapping[str, Any]) -> str:
    current: Any = data
    for segment in path.split(".")[1:]:
        if not isinstance(current, Mapping) or segment not in current:
            raise TemplateError(f"${{data{path}}} is not in the data")
        current = current[segment]
    return _written(current)


def substitute(text: str, data: Mapping[str, Any], variables: Mapping[str, str]) -> str:
    """The text with each placeholder filled; any other `${...}` is an error."""
    out: list[str] = []
    position = 0
    while (start := text.find(OPEN, position)) != -1:
        match = PLACEHOLDER.match(text, start)
        if match is None:
            raise TemplateError(f"{_fragment(text, start)} is not ${{data.<path>}} or ${{vars.<name>}}")
        out.append(text[position:start])
        path, name = match.group(1), match.group(2)
        if path is not None:
            out.append(_data(path, data))
        elif name in variables:
            out.append(variables[name])
        else:
            raise TemplateError(f"${{vars.{name}}} is not a variable")
        position = match.end()
    out.append(text[position:])
    return "".join(out)


def placeholder_problem(text: str, variables: Collection[str]) -> str | None:
    """Why a text of a template would not fill: a `${...}` of another form, or a variable that does not exist."""
    position = 0
    while (start := text.find(OPEN, position)) != -1:
        match = PLACEHOLDER.match(text, start)
        if match is None:
            return f"{_fragment(text, start)} is not ${{data.<path>}} or ${{vars.<name>}}"
        name = match.group(2)
        if name is not None and name not in variables:
            return f"${{vars.{name}}} is not a variable of the configuration"
        position = match.end()
    return None
