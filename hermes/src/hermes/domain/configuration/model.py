"""The configuration of Hermes (data-model.md): instances, templates, variables and contacts.

Each entity checks its own rules when it is built; the checks across entities (a channel on an instance that exists,
the placeholders of a template, the secrets of a manifest) belong to the validation of a file.
"""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from typing import Any
from urllib.parse import urlsplit

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.names import is_name
from hermes.domain.shared.references import SecretReference

type JsonValue = bool | int | float | str | list[JsonValue] | dict[str, JsonValue] | None

SEVERITIES = frozenset({"info", "low", "medium", "high", "critical"})
MAX_TITLE = 256
MAX_SUMMARY = 2048
MAX_FACTS = 20
MAX_FACT_TEXT = 256
MAX_ACTIONS = 10
MAX_ACTION = 256
MAX_EXPECTED = 512
MAX_LINK = 2048
MAX_VARIABLE = 4096


class ConfigurationError(HermesError):
    """A rule of the configuration that does not hold, with where it fails."""

    def __init__(self, path: str, message: str) -> None:
        super().__init__(ErrorCode.VALIDATION, f"{path}: {message}", details=[{"path": path, "message": message}])
        self.path = path


def templated(text: str) -> bool:
    """A text with `${...}` is checked after substitution, when it is sent."""
    return "${" in text


def _name(value: str, path: str) -> None:
    if not is_name(value):
        raise ConfigurationError(path, "must follow ^[a-z][a-z0-9_]{0,62}$")


def _length(text: str, path: str, *, most: int, least: int = 0) -> None:
    if not templated(text) and not least <= len(text) <= most:
        raise ConfigurationError(path, f"must have from {least} to {most} characters")


def check_link(text: str, path: str) -> None:
    parts = urlsplit(text)
    if parts.scheme != "https" or not parts.netloc or len(text) > MAX_LINK:
        raise ConfigurationError(path, f"must be an https URL of up to {MAX_LINK} characters")


def _unique(names: Iterable[str], path: str) -> None:
    repeated = sorted(name for name, count in Counter(names).items() if count > 1)
    if repeated:
        raise ConfigurationError(path, f"repeated: {', '.join(repeated)}")


@dataclass(frozen=True)
class Instance:
    """A configured provider library: parameters and references to its secrets, never their values."""

    name: str
    provider: str
    params: Mapping[str, JsonValue]
    secrets: Mapping[str, SecretReference]
    requests_per_minute: int | None = None

    def __post_init__(self) -> None:
        _name(self.name, "instances.name")
        _name(self.provider, f"instances.{self.name}.provider")
        if self.requests_per_minute is not None and not 1 <= self.requests_per_minute <= 600:
            raise ConfigurationError(f"instances.{self.name}.limits.requests_per_minute", "must be from 1 to 600")


@dataclass(frozen=True)
class Fact:
    label: str
    value: str


@dataclass(frozen=True)
class MessageTexts:
    """The texts that fill the neutral message; each one may hold `${data.<path>}` and `${vars.<name>}`."""

    title: str
    severity: str
    summary: str | None = None
    facts: tuple[Fact, ...] = ()
    actions_done: tuple[str, ...] = ()
    expected: str | None = None
    link: str | None = None

    def __post_init__(self) -> None:
        if not self.title:
            raise ConfigurationError("message.title", "must not be empty")
        _length(self.title, "message.title", least=1, most=MAX_TITLE)
        if not templated(self.severity) and self.severity not in SEVERITIES:
            raise ConfigurationError("message.severity", "must be info, low, medium, high or critical")
        if self.summary is not None:
            _length(self.summary, "message.summary", most=MAX_SUMMARY)
        if len(self.facts) > MAX_FACTS:
            raise ConfigurationError("message.facts", f"must have up to {MAX_FACTS} pairs")
        for index, fact in enumerate(self.facts):
            _length(fact.label, f"message.facts[{index}].label", most=MAX_FACT_TEXT)
            _length(fact.value, f"message.facts[{index}].value", most=MAX_FACT_TEXT)
        if len(self.actions_done) > MAX_ACTIONS:
            raise ConfigurationError("message.actions_done", f"must have up to {MAX_ACTIONS} actions")
        for index, action in enumerate(self.actions_done):
            _length(action, f"message.actions_done[{index}]", most=MAX_ACTION)
        if self.expected is not None:
            _length(self.expected, "message.expected", most=MAX_EXPECTED)
        if self.link is not None and not templated(self.link):
            check_link(self.link, "message.link")

    def as_json(self) -> dict[str, JsonValue]:
        out: dict[str, JsonValue] = {"title": self.title, "severity": self.severity}
        if self.summary is not None:
            out["summary"] = self.summary
        if self.facts:
            out["facts"] = [{"label": fact.label, "value": fact.value} for fact in self.facts]
        if self.actions_done:
            out["actions_done"] = list(self.actions_done)
        if self.expected is not None:
            out["expected"] = self.expected
        if self.link is not None:
            out["link"] = self.link
        return out

    @classmethod
    def from_json(cls, data: Mapping[str, Any]) -> MessageTexts:
        return cls(
            title=data["title"],
            severity=data["severity"],
            summary=data.get("summary"),
            facts=tuple(Fact(label=item["label"], value=item["value"]) for item in data.get("facts", [])),
            actions_done=tuple(data.get("actions_done", [])),
            expected=data.get("expected"),
            link=data.get("link"),
        )


@dataclass(frozen=True)
class TemplateVersion:
    """A version of a template: the JSON Schema of its data and its texts. A version never changes."""

    version: int
    data_schema: Mapping[str, JsonValue]
    message: MessageTexts

    def __post_init__(self) -> None:
        if self.version < 1:
            raise ConfigurationError("templates.versions.version", "must be a number from 1")


@dataclass(frozen=True)
class Template:
    id: str
    versions: tuple[TemplateVersion, ...]

    def __post_init__(self) -> None:
        _name(self.id, "templates.id")
        if not self.versions:
            raise ConfigurationError(f"templates.{self.id}.versions", "must have at least one version")
        _unique((str(version.version) for version in self.versions), f"templates.{self.id}.versions")

    def latest(self) -> TemplateVersion:
        return max(self.versions, key=lambda version: version.version)

    def version(self, number: int) -> TemplateVersion | None:
        return next((version for version in self.versions if version.version == number), None)


@dataclass(frozen=True)
class Variable:
    name: str
    value: str

    def __post_init__(self) -> None:
        _name(self.name, "variables")
        if len(self.value) > MAX_VARIABLE:
            raise ConfigurationError(f"variables.{self.name}", f"must have up to {MAX_VARIABLE} characters")


@dataclass(frozen=True, order=True)
class ChannelSpec:
    """The part of a channel that the configuration gives; its address comes from the verification."""

    service: str
    provider: str
    instance: str


@dataclass(frozen=True)
class Contact:
    name: str
    channels: tuple[ChannelSpec, ...]

    def __post_init__(self) -> None:
        _name(self.name, "contacts.name")
        if not self.channels:
            raise ConfigurationError(f"contacts.{self.name}.channels", "must have at least one channel")
        keys = [f"{c.service}/{c.provider}/{c.instance}" for c in self.channels]
        _unique(keys, f"contacts.{self.name}.channels: one channel per service, provider and instance")


@dataclass(frozen=True)
class Configuration:
    """The whole configuration, replaced at once when a file is applied."""

    instances: tuple[Instance, ...]
    templates: tuple[Template, ...]
    variables: tuple[Variable, ...]
    contacts: tuple[Contact, ...]
    version: int = field(default=1)

    def __post_init__(self) -> None:
        _unique((instance.name for instance in self.instances), "instances")
        _unique((template.id for template in self.templates), "templates")
        _unique((variable.name for variable in self.variables), "variables")
        _unique((contact.name for contact in self.contacts), "contacts")

    @classmethod
    def empty(cls) -> Configuration:
        return cls(instances=(), templates=(), variables=(), contacts=())

    def instance(self, name: str) -> Instance | None:
        return next((instance for instance in self.instances if instance.name == name), None)

    def template(self, template_id: str) -> Template | None:
        return next((template for template in self.templates if template.id == template_id), None)

    def contact(self, name: str) -> Contact | None:
        return next((contact for contact in self.contacts if contact.name == name), None)

    def variable_values(self) -> dict[str, str]:
        return {variable.name: variable.value for variable in self.variables}
