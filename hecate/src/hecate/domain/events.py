"""Plain encoding of domain events: a type name, a JSON-able dict and the instant.

By convention the fields named `at` or ending in `_at` are instants; every other field is
a JSON scalar. Events never carry a value or a key.
"""

import dataclasses
from datetime import datetime
from typing import Any


def _is_instant(name: str) -> bool:
    return name == "at" or name.endswith("_at")


def encode_event(event: Any) -> tuple[str, dict[str, Any], datetime]:
    """Return (type name, data without `at`, instant)."""
    data: dict[str, Any] = {}
    for field in dataclasses.fields(event):
        value = getattr(event, field.name)
        if field.name == "at":
            continue
        data[field.name] = value.isoformat() if _is_instant(field.name) and value is not None else value
    return type(event).__name__, data, event.at


def decode_event(classes: dict[str, type], type_name: str, data: dict[str, Any], at: datetime) -> Any:
    cls = classes[type_name]
    values = {
        name: (datetime.fromisoformat(value) if _is_instant(name) and value is not None else value)
        for name, value in data.items()
    }
    return cls(at=at, **values)
