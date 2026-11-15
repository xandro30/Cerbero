"""The models of the configuration file (research R7, contracts/config-file.md#campos): strict Pydantic, without unknown
fields or implicit conversions, and then the entities of the domain. Every fault comes at once, with its path in the
file and never with its value."""

from __future__ import annotations

from collections import Counter
from collections.abc import Callable, Iterable, Mapping, Sequence
from typing import Annotated, Any, Literal

from pydantic import AfterValidator, BaseModel, ConfigDict, Field, JsonValue, StringConstraints, ValidationError

from hermes.domain.configuration.model import (
    MAX_VARIABLE,
    ChannelSpec,
    Configuration,
    ConfigurationError,
    Contact,
    Fact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
    Variable,
)
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.infrastructure.config_file.loader import ROOT, load_yaml

SECRET_RULE = "must be a secret:// reference with the rules of Hécate"  # noqa: S105  # a rule, not a value


def _secret_reference(value: str) -> str:
    try:
        SecretReference.parse(value)
    except HermesError:
        raise ValueError(SECRET_RULE) from None
    return value


Name = Annotated[str, StringConstraints(pattern=r"^[a-z][a-z0-9_]{0,62}$")]
SecretField = Annotated[str, AfterValidator(_secret_reference)]


class _Strict(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class LimitsFile(_Strict):
    requests_per_minute: Annotated[int, Field(ge=1, le=600)]


class InstanceFile(_Strict):
    name: Name
    provider: Name
    params: dict[str, JsonValue] = {}
    secrets: dict[str, SecretField] = {}
    limits: LimitsFile | None = None


class FactFile(_Strict):
    label: str
    value: str


class MessageFile(_Strict):
    title: str
    severity: str
    summary: str | None = None
    facts: list[FactFile] = []
    actions_done: list[str] = []
    expected: str | None = None
    link: str | None = None


class VersionFile(_Strict):
    version: Annotated[int, Field(ge=1)]
    data_schema: dict[str, JsonValue]
    message: MessageFile


class TemplateFile(_Strict):
    id: Name
    versions: Annotated[list[VersionFile], Field(min_length=1)]


class ChannelFile(_Strict):
    service: Literal["chat"]
    provider: Name
    instance: Name


class ContactFile(_Strict):
    name: Name
    channels: Annotated[list[ChannelFile], Field(min_length=1)]


class ConfigFile(_Strict):
    version: Literal[1]
    variables: dict[Name, Annotated[str, StringConstraints(max_length=MAX_VARIABLE)]] = {}
    instances: list[InstanceFile] = []
    templates: list[TemplateFile] = []
    contacts: list[ContactFile] = []


def _location(parts: Sequence[int | str]) -> str:
    out = ""
    for part in parts:
        if part == "[key]":
            continue
        out += f"[{part}]" if isinstance(part, int) else (f".{part}" if out else str(part))
    return out or ROOT


def _message(item: Mapping[str, Any]) -> str:
    """The reason of a fault: the one of a validator of this module without the `Value error, ` that Pydantic adds."""
    context = item.get("ctx") or {}
    if item.get("type") == "value_error" and isinstance(context.get("error"), ValueError):
        return str(context["error"])
    return str(item["msg"])


def invalid(faults: Iterable[tuple[str, str]]) -> HermesError:
    details = [{"path": path, "message": message} for path, message in faults]
    noun = "error" if len(details) == 1 else "errors"
    return HermesError(ErrorCode.VALIDATION, f"the configuration has {len(details)} {noun}", details=details)


def _repeated(names: Sequence[str], path: Callable[[int], str]) -> list[tuple[str, str]]:
    seen: Counter[str] = Counter()
    faults = []
    for index, name in enumerate(names):
        seen[name] += 1
        if seen[name] == 2:
            faults.append((path(index), "repeats"))
    return faults


def _version(model: VersionFile, where: str, faults: list[tuple[str, str]]) -> TemplateVersion | None:
    message = model.message
    try:
        texts = MessageTexts(
            title=message.title,
            severity=message.severity,
            summary=message.summary,
            facts=tuple(Fact(label=fact.label, value=fact.value) for fact in message.facts),
            actions_done=tuple(message.actions_done),
            expected=message.expected,
            link=message.link,
        )
    except ConfigurationError as error:
        faults.append((f"{where}.{error.path}", error.details[0]["message"]))
        return None
    return TemplateVersion(version=model.version, data_schema=model.data_schema, message=texts)


def parse_configuration(data: Any) -> Configuration:
    try:
        model = ConfigFile.model_validate(data)
    except ValidationError as error:
        raise invalid((_location(item["loc"]), _message(item)) for item in error.errors(include_url=False)) from None
    faults: list[tuple[str, str]] = []
    faults += _repeated([item.name for item in model.instances], "instances[{}].name".format)
    faults += _repeated([item.id for item in model.templates], "templates[{}].id".format)
    faults += _repeated([item.name for item in model.contacts], "contacts[{}].name".format)
    templates = []
    for t_index, template in enumerate(model.templates):
        faults += _repeated(
            [str(version.version) for version in template.versions],
            f"templates[{t_index}].versions[{{}}].version".format,
        )
        versions = [
            _version(version, f"templates[{t_index}].versions[{v_index}]", faults)
            for v_index, version in enumerate(template.versions)
        ]
        if all(version is not None for version in versions):
            templates.append((template.id, tuple(version for version in versions if version is not None)))
    for c_index, contact in enumerate(model.contacts):
        faults += _repeated(
            [channel.instance for channel in contact.channels], f"contacts[{c_index}].channels[{{}}].instance".format
        )
    if faults:
        raise invalid(faults)
    return Configuration(
        instances=tuple(
            Instance(
                name=item.name,
                provider=item.provider,
                params=item.params,
                secrets={field: SecretReference.parse(value) for field, value in item.secrets.items()},
                requests_per_minute=item.limits.requests_per_minute if item.limits is not None else None,
            )
            for item in model.instances
        ),
        templates=tuple(Template(id=key, versions=versions) for key, versions in templates),
        variables=tuple(Variable(name=name, value=value) for name, value in model.variables.items()),
        contacts=tuple(
            Contact(
                name=item.name,
                channels=tuple(ChannelSpec(c.service, c.provider, c.instance) for c in item.channels),
            )
            for item in model.contacts
        ),
    )


def read_configuration(text: str) -> Configuration:
    """The configuration of a file, with every fault of its format and its fields."""
    return parse_configuration(load_yaml(text.encode("utf-8")))
