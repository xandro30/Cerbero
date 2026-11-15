"""The canonical form of a configuration and its SHA-256: equal contents give equal digests, in any order."""

from __future__ import annotations

import hashlib
import json

from hermes.domain.configuration.model import Configuration, JsonValue, TemplateVersion


def _dumps(value: JsonValue) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def version_json(version: TemplateVersion) -> dict[str, JsonValue]:
    return {"version": version.version, "data_schema": dict(version.data_schema), "message": version.message.as_json()}


def canonical(configuration: Configuration) -> dict[str, JsonValue]:
    return {
        "version": configuration.version,
        "variables": {variable.name: variable.value for variable in configuration.variables},
        "instances": [
            {
                "name": instance.name,
                "provider": instance.provider,
                "params": dict(instance.params),
                "secrets": {name: str(reference) for name, reference in instance.secrets.items()},
                "requests_per_minute": instance.requests_per_minute,
            }
            for instance in sorted(configuration.instances, key=lambda item: item.name)
        ],
        "templates": [
            {
                "id": template.id,
                "versions": [version_json(v) for v in sorted(template.versions, key=lambda item: item.version)],
            }
            for template in sorted(configuration.templates, key=lambda item: item.id)
        ],
        "contacts": [
            {
                "name": contact.name,
                "channels": [
                    {"service": c.service, "provider": c.provider, "instance": c.instance}
                    for c in sorted(contact.channels)
                ],
            }
            for contact in sorted(configuration.contacts, key=lambda item: item.name)
        ],
    }


def configuration_digest(configuration: Configuration) -> bytes:
    return hashlib.sha256(_dumps(canonical(configuration))).digest()


def version_digest(version: TemplateVersion) -> bytes:
    """The digest of a template version, to refuse a change to a version that already exists."""
    return hashlib.sha256(_dumps(version_json(version))).digest()
