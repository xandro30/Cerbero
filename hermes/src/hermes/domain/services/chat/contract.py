"""The common contract of the chat service type: `chat.send_message` (FR-017, FR-018, FR-021, contracts/api.md)."""

from __future__ import annotations

import re
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from hermes.domain.configuration.model import Configuration, TemplateVersion
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.names import is_name

CATEGORY = "communication"
SERVICE = "chat"
CONTRACT = "v1"
_VALUE = re.compile(r"\$\{([a-z_]+)\}")


@dataclass(frozen=True)
class FunctionContract:
    """The contract of a function, as `x-cerbero-function` publishes it in the OpenAPI."""

    service: str
    name: str
    kind: str
    mode: str
    timeout_s: int
    side_effect: str
    discloses_data: str
    non_reversible: bool

    @property
    def qualified_name(self) -> str:
        return f"{self.service}.{self.name}"

    def side_effect_for(self, values: Mapping[str, str]) -> str:
        return _VALUE.sub(lambda match: values[match.group(1)], self.side_effect)

    def discloses_data_for(self, values: Mapping[str, str]) -> str:
        return _VALUE.sub(lambda match: values[match.group(1)], self.discloses_data)


SEND_MESSAGE = FunctionContract(
    service=SERVICE,
    name="send_message",
    kind="command",
    mode="sync",
    timeout_s=10,
    side_effect="Envía a ${recipient} el mensaje «${title}» por la instancia ${instance}",
    discloses_data="${provider} recibe el texto del mensaje y el chat de ${recipient}",
    non_reversible=True,
)


def _invalid(path: str, message: str) -> HermesError:
    return HermesError(ErrorCode.VALIDATION, f"{path}: {message}", details=[{"path": path, "message": message}])


@dataclass(frozen=True)
class SendRequest:
    """What the caller asks: one contact, a template with its optional version, its data and the optional instance."""

    recipients: tuple[str, ...]
    template_id: str
    data: Mapping[str, Any]
    template_version: int | None = None
    instance: str | None = None

    def __post_init__(self) -> None:
        if len(self.recipients) != 1:
            raise _invalid("recipients", "must hold exactly one contact")
        rule = "must follow ^[a-z][a-z0-9_]{0,62}$"
        if not is_name(self.recipients[0]):
            raise _invalid("recipients[0]", rule)
        if not is_name(self.template_id):
            raise _invalid("template.id", rule)
        if self.template_version is not None and self.template_version < 1:
            raise _invalid("template.version", "must be a number from 1")
        if self.instance is not None and not is_name(self.instance):
            raise _invalid("instance", rule)

    @property
    def recipient(self) -> str:
        return self.recipients[0]


def resolve_template(configuration: Configuration, request: SendRequest) -> TemplateVersion:
    """The version that the request names, or the latest one."""
    template = configuration.template(request.template_id)
    if template is None:
        raise HermesError(ErrorCode.NOT_FOUND, "the template does not exist")
    if request.template_version is None:
        return template.latest()
    version = template.version(request.template_version)
    if version is None:
        raise HermesError(ErrorCode.NOT_FOUND, "the template version does not exist")
    return version
