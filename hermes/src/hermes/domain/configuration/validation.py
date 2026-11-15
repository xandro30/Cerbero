"""The validation of a configuration before it is applied, rules 2 to 7 of
contracts/config-file.md#validación-al-aplicar.

Rules 1 and 6 hold when the file is read, rule 8 in the plan and rule 9 against Hécate. Every problem comes at once,
with its path in the file (the position of each entity, as the file gives them) and never with a value.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping
from dataclasses import dataclass

from jsonschema import Draft202012Validator
from jsonschema.exceptions import SchemaError

from hermes.domain.configuration.model import Configuration, MessageTexts
from hermes.domain.messages.templating import placeholder_problem
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.errors import ErrorCode, HermesError

FAKE = "fake"
HERMES = "hermes"


@dataclass(frozen=True)
class Problem:
    path: str
    message: str


def problems_error(problems: list[Problem]) -> HermesError:
    noun = "error" if len(problems) == 1 else "errors"
    return HermesError(
        ErrorCode.VALIDATION,
        f"the configuration has {len(problems)} {noun}",
        details=[{"path": problem.path, "message": problem.message} for problem in problems],
    )


def _texts(message: MessageTexts) -> Iterator[tuple[str, str]]:
    yield "title", message.title
    yield "severity", message.severity
    if message.summary is not None:
        yield "summary", message.summary
    for index, fact in enumerate(message.facts):
        yield f"facts[{index}].label", fact.label
        yield f"facts[{index}].value", fact.value
    for index, action in enumerate(message.actions_done):
        yield f"actions_done[{index}]", action
    if message.expected is not None:
        yield "expected", message.expected
    if message.link is not None:
        yield "link", message.link


def _instances(configuration: Configuration, manifests: Mapping[str, Manifest], fake_enabled: bool) -> list[Problem]:
    problems = []
    for index, instance in enumerate(configuration.instances):
        base = f"instances[{index}]"
        if instance.provider == FAKE and not fake_enabled:
            problems.append(Problem(f"{base}.provider", "the fake provider needs HERMES_FAKE_PROVIDERS=enabled"))
            continue
        manifest = manifests.get(instance.provider)
        if manifest is None:
            problems.append(Problem(f"{base}.provider", "Hermes has no library for this provider"))
            continue
        secrets = {field.name: field for field in manifest.instance_config if field.type == "secret"}
        params = {field.name: field for field in manifest.instance_config if field.type == "param"}
        for name in sorted(set(secrets) - set(instance.secrets)):
            if secrets[name].required:
                problems.append(Problem(f"{base}.secrets.{name}", f"the library {manifest.provider} requires it"))
        for name in sorted(set(instance.secrets) - set(secrets)):
            problems.append(Problem(f"{base}.secrets.{name}", f"not a secret field of {manifest.provider}"))
        for name in sorted(set(instance.secrets) & set(secrets)):
            reference = instance.secrets[name]
            if reference.identity != HERMES or reference.path != (instance.provider, instance.name, name):
                expected = f"secret://{HERMES}/{instance.provider}/{instance.name}/{name}"
                problems.append(Problem(f"{base}.secrets.{name}", f"must be {expected}"))
        for name in sorted(set(params) - set(instance.params)):
            if params[name].required:
                problems.append(Problem(f"{base}.params.{name}", f"the library {manifest.provider} requires it"))
        for name in sorted(set(instance.params) - set(params)):
            problems.append(Problem(f"{base}.params.{name}", f"not a parameter of {manifest.provider}"))
    return problems


def _templates(configuration: Configuration) -> list[Problem]:
    variables = {variable.name for variable in configuration.variables}
    problems = []
    for t_index, template in enumerate(configuration.templates):
        for v_index, version in enumerate(template.versions):
            base = f"templates[{t_index}].versions[{v_index}]"
            try:
                Draft202012Validator.check_schema(dict(version.data_schema))
            except SchemaError:
                problems.append(Problem(f"{base}.data_schema", "is not a valid JSON Schema (Draft 2020-12)"))
            for path, text in _texts(version.message):
                reason = placeholder_problem(text, variables)
                if reason is not None:
                    problems.append(Problem(f"{base}.message.{path}", reason))
    return problems


def _channels(configuration: Configuration) -> list[Problem]:
    instances = {instance.name: instance for instance in configuration.instances}
    problems = []
    for c_index, contact in enumerate(configuration.contacts):
        for h_index, channel in enumerate(contact.channels):
            base = f"contacts[{c_index}].channels[{h_index}]"
            instance = instances.get(channel.instance)
            if instance is None:
                problems.append(Problem(f"{base}.instance", "the instance does not exist"))
            elif instance.provider != channel.provider:
                problems.append(Problem(f"{base}.provider", f"the instance {instance.name} is of another provider"))
    return problems


def validate(configuration: Configuration, *, manifests: Mapping[str, Manifest], fake_enabled: bool) -> list[Problem]:
    return [
        *_instances(configuration, manifests, fake_enabled),
        *_templates(configuration),
        *_channels(configuration),
    ]
