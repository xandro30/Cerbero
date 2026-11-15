"""The configuration as a file (contracts/config-file.md#exportar): the lists by name and the versions by number, with
the references of the secrets and without addresses, codes or the state of the verification."""

from __future__ import annotations

from typing import Any

import yaml

from hermes.domain.configuration.model import Configuration


def write_configuration(configuration: Configuration) -> str:
    document: dict[str, Any] = {"version": 1}
    if configuration.variables:
        document["variables"] = {
            variable.name: variable.value for variable in sorted(configuration.variables, key=lambda item: item.name)
        }
    instances = []
    for instance in sorted(configuration.instances, key=lambda item: item.name):
        entry: dict[str, Any] = {"name": instance.name, "provider": instance.provider}
        if instance.params:
            entry["params"] = dict(instance.params)
        if instance.secrets:
            entry["secrets"] = {field: str(instance.secrets[field]) for field in sorted(instance.secrets)}
        if instance.requests_per_minute is not None:
            entry["limits"] = {"requests_per_minute": instance.requests_per_minute}
        instances.append(entry)
    if instances:
        document["instances"] = instances
    templates = [
        {
            "id": template.id,
            "versions": [
                {
                    "version": version.version,
                    "data_schema": dict(version.data_schema),
                    "message": version.message.as_json(),
                }
                for version in sorted(template.versions, key=lambda item: item.version)
            ],
        }
        for template in sorted(configuration.templates, key=lambda item: item.id)
    ]
    if templates:
        document["templates"] = templates
    contacts = [
        {
            "name": contact.name,
            "channels": [
                {"service": channel.service, "provider": channel.provider, "instance": channel.instance}
                for channel in sorted(contact.channels)
            ],
        }
        for contact in sorted(configuration.contacts, key=lambda item: item.name)
    ]
    if contacts:
        document["contacts"] = contacts
    return yaml.safe_dump(document, sort_keys=False, allow_unicode=True, default_flow_style=False, width=4096)
