"""The plan of an application (contracts/admin-channel.md#cambios): what is deleted, what changes and what is created.

The changes come in that order, the dependents first when they go and last when they come. A channel that goes, or that
changes instance (it goes and another comes), loses its verification. A template version that exists does not change
its content (rule 8): to change it, another version is added.
"""

from __future__ import annotations

import hashlib
from collections.abc import Iterable
from dataclasses import dataclass
from typing import Literal

from hermes.domain.configuration.digest import configuration_digest, version_digest
from hermes.domain.configuration.model import Configuration, Instance
from hermes.domain.configuration.validation import Problem

Action = Literal["create", "update", "delete"]
Kind = Literal["variable", "instance", "template_version", "contact", "channel"]

DELETED: tuple[Kind, ...] = ("channel", "contact", "template_version", "instance", "variable")
UPDATED: tuple[Kind, ...] = ("variable", "instance")
CREATED: tuple[Kind, ...] = ("variable", "instance", "template_version", "contact", "channel")
LOSES_VERIFICATION = "loses its verification"


@dataclass(frozen=True)
class Change:
    action: Action
    kind: Kind
    name: str
    detail: str | None = None


def _instance_detail(old: Instance, new: Instance) -> str | None:
    parts = []
    if old.provider != new.provider:
        parts.append("provider")
    changed_params = sorted(
        key for key in set(old.params) | set(new.params) if old.params.get(key) != new.params.get(key)
    )
    if changed_params:
        parts.append("params: " + ", ".join(changed_params))
    if old.requests_per_minute != new.requests_per_minute:
        parts.append("limits")
    changed_secrets = sorted(
        key for key in set(old.secrets) | set(new.secrets) if old.secrets.get(key) != new.secrets.get(key)
    )
    if changed_secrets:
        parts.append("secrets: " + ", ".join(changed_secrets))
    return ", ".join(parts) or None


def _channel_names(configuration: Configuration) -> set[str]:
    return {
        f"{contact.name} {channel.provider}/{channel.instance}"
        for contact in configuration.contacts
        for channel in contact.channels
    }


def _ordered(changes: Iterable[Change]) -> tuple[Change, ...]:
    def position(change: Change) -> tuple[int, int, str]:
        if change.action == "delete":
            return 0, DELETED.index(change.kind), change.name
        if change.action == "update":
            return 1, UPDATED.index(change.kind), change.name
        return 2, CREATED.index(change.kind), change.name

    return tuple(sorted(changes, key=position))


def diff(current: Configuration, new: Configuration) -> tuple[tuple[Change, ...], list[Problem]]:
    changes: list[Change] = []
    problems: list[Problem] = []

    old_variables = {variable.name: variable.value for variable in current.variables}
    new_variables = {variable.name: variable.value for variable in new.variables}
    changes += [Change("delete", "variable", name) for name in old_variables.keys() - new_variables.keys()]
    changes += [Change("create", "variable", name) for name in new_variables.keys() - old_variables.keys()]
    changes += [
        Change("update", "variable", name, "value")
        for name in old_variables.keys() & new_variables.keys()
        if old_variables[name] != new_variables[name]
    ]

    old_instances = {instance.name: instance for instance in current.instances}
    new_instances = {instance.name: instance for instance in new.instances}
    changes += [Change("delete", "instance", name) for name in old_instances.keys() - new_instances.keys()]
    changes += [Change("create", "instance", name) for name in new_instances.keys() - old_instances.keys()]
    for name in old_instances.keys() & new_instances.keys():
        detail = _instance_detail(old_instances[name], new_instances[name])
        if detail is not None:
            changes.append(Change("update", "instance", name, detail))

    old_versions = {
        (template.id, version.version): version for template in current.templates for version in template.versions
    }
    for t_index, template in enumerate(new.templates):
        for v_index, version in enumerate(template.versions):
            existing = old_versions.get((template.id, version.version))
            if existing is None:
                changes.append(Change("create", "template_version", f"{template.id} v{version.version}"))
            elif version_digest(existing) != version_digest(version):
                problems.append(
                    Problem(
                        f"templates[{t_index}].versions[{v_index}]",
                        "this version already exists with another content: add a new version instead",
                    )
                )
    new_keys = {(template.id, version.version) for template in new.templates for version in template.versions}
    changes += [
        Change("delete", "template_version", f"{template_id} v{number}")
        for template_id, number in old_versions.keys() - new_keys
    ]

    old_contacts = {contact.name for contact in current.contacts}
    new_contacts = {contact.name for contact in new.contacts}
    changes += [Change("delete", "contact", name) for name in old_contacts - new_contacts]
    changes += [Change("create", "contact", name) for name in new_contacts - old_contacts]

    old_channels, new_channels = _channel_names(current), _channel_names(new)
    changes += [Change("delete", "channel", name, LOSES_VERIFICATION) for name in old_channels - new_channels]
    changes += [Change("create", "channel", name) for name in new_channels - old_channels]

    return _ordered(changes), problems


def plan_digest(current: Configuration, new: Configuration) -> str:
    """The digest of a plan: the configuration of the file and the one that was in the database."""
    digest = hashlib.sha256(configuration_digest(current) + configuration_digest(new)).hexdigest()
    return f"sha256:{digest}"
