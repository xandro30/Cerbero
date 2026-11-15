"""The manifest of a provider library (FR-026): what it implements, its capabilities, its limits, the permissions it
needs in the provider and the configuration of its instances."""

from __future__ import annotations

import re
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from hermes.domain.shared.names import is_name

KINDS = frozenset({"query", "command"})
DURATION = re.compile(r"([1-9][0-9]{0,3})s")
FIELD_TYPES = frozenset({"secret", "param"})


class ManifestError(ValueError):
    """A manifest that does not follow its format: the library is not loaded."""


@dataclass(frozen=True)
class FunctionSpec:
    name: str
    kind: str
    mode: str
    timeout_s: int


@dataclass(frozen=True)
class Implementation:
    service: str
    contract: str
    functions: tuple[FunctionSpec, ...]


@dataclass(frozen=True)
class FieldSpec:
    name: str
    type: str
    required: bool


@dataclass(frozen=True)
class Manifest:
    provider: str
    version: str
    implements: tuple[Implementation, ...]
    max_length: int
    formats: tuple[str, ...]
    requests_per_minute: int
    permissions: tuple[str, ...]
    instance_config: tuple[FieldSpec, ...]

    def function(self, service: str, name: str) -> FunctionSpec | None:
        for implementation in self.implements:
            if implementation.service == service:
                return next((function for function in implementation.functions if function.name == name), None)
        return None

    @property
    def secret_fields(self) -> tuple[str, ...]:
        return tuple(field.name for field in self.instance_config if field.type == "secret")

    @classmethod
    def from_json(cls, data: Any) -> Manifest:
        document = _object(data, "manifest")
        capabilities = _object(document.get("capabilities"), "capabilities")
        limits = _object(document.get("limits"), "limits")
        provider = _text(document.get("provider"), "provider")
        if not is_name(provider):
            raise ManifestError("provider must follow ^[a-z][a-z0-9_]{0,62}$")
        return cls(
            provider=provider,
            version=_text(document.get("version"), "version"),
            implements=tuple(_implementation(item) for item in _list(document.get("implements"), "implements")),
            max_length=_positive(capabilities.get("max_length"), "capabilities.max_length"),
            formats=tuple(
                _text(item, "capabilities.formats") for item in _list(capabilities.get("formats"), "formats")
            ),
            requests_per_minute=_positive(limits.get("requests_per_minute"), "limits.requests_per_minute"),
            permissions=tuple(_text(item, "permissions") for item in _list(document.get("permissions"), "permissions")),
            instance_config=tuple(
                _field(name, spec) for name, spec in _object(document.get("instance_config"), "instance_config").items()
            ),
        )


def _object(value: Any, path: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise ManifestError(f"{path} must be an object")
    return value


def _list(value: Any, path: str) -> Sequence[Any]:
    if not isinstance(value, list):
        raise ManifestError(f"{path} must be a list")
    return value


def _text(value: Any, path: str) -> str:
    if not isinstance(value, str) or not value:
        raise ManifestError(f"{path} must be a text")
    return value


def _positive(value: Any, path: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise ManifestError(f"{path} must be a positive whole number")
    return value


def _function(name: str, spec: Any) -> FunctionSpec:
    fields = _object(spec, f"functions.{name}")
    kind = _text(fields.get("kind"), f"functions.{name}.kind")
    if kind not in KINDS:
        raise ManifestError(f"functions.{name}.kind must be query or command")
    if fields.get("mode") != "sync":
        raise ManifestError(f"functions.{name}.mode must be sync")
    timeout = DURATION.fullmatch(_text(fields.get("timeout"), f"functions.{name}.timeout"))
    if timeout is None:
        raise ManifestError(f"functions.{name}.timeout must be a number of seconds, such as 10s")
    return FunctionSpec(name=name, kind=kind, mode="sync", timeout_s=int(timeout.group(1)))


def _implementation(value: Any) -> Implementation:
    fields = _object(value, "implements")
    functions = _object(fields.get("functions"), "implements.functions")
    return Implementation(
        service=_text(fields.get("service"), "implements.service"),
        contract=_text(fields.get("contract"), "implements.contract"),
        functions=tuple(_function(name, spec) for name, spec in functions.items()),
    )


def _field(name: str, spec: Any) -> FieldSpec:
    fields = _object(spec, f"instance_config.{name}")
    kind = fields.get("type")
    if kind not in FIELD_TYPES:
        raise ManifestError(f"instance_config.{name}.type must be secret or param")
    required = fields.get("required", False)
    if not isinstance(required, bool):
        raise ManifestError(f"instance_config.{name}.required must be true or false")
    return FieldSpec(name=name, type=kind, required=required)
