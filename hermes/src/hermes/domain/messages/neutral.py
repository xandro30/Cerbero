"""The neutral message: what every chat provider draws with its own capabilities (FR-018, data-model.md).

Its fields keep their rules after the substitution; a template may only skip them while its texts hold placeholders.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from hermes.domain.configuration.model import (
    MAX_ACTION,
    MAX_ACTIONS,
    MAX_EXPECTED,
    MAX_FACT_TEXT,
    MAX_FACTS,
    MAX_SUMMARY,
    MAX_TITLE,
    SEVERITIES,
    ConfigurationError,
    Fact,
    MessageTexts,
    check_link,
)
from hermes.domain.messages.templating import TemplateError, substitute
from hermes.domain.shared.errors import ErrorCode, HermesError


class MessageError(HermesError):
    """A field of the message that does not keep its rule after the substitution."""

    def __init__(self, path: str, message: str) -> None:
        super().__init__(ErrorCode.VALIDATION, f"{path}: {message}", details=[{"path": path, "message": message}])


def _length(text: str, path: str, *, most: int, least: int = 0) -> None:
    if not least <= len(text) <= most:
        raise MessageError(path, f"must have from {least} to {most} characters")


@dataclass(frozen=True)
class NeutralMessage:
    title: str
    severity: str
    summary: str | None = None
    facts: tuple[Fact, ...] = ()
    actions_done: tuple[str, ...] = ()
    expected: str | None = None
    link: str | None = None

    def __post_init__(self) -> None:
        _length(self.title, "message.title", least=1, most=MAX_TITLE)
        if self.severity not in SEVERITIES:
            raise MessageError("message.severity", "must be info, low, medium, high or critical")
        if self.summary is not None:
            _length(self.summary, "message.summary", most=MAX_SUMMARY)
        if len(self.facts) > MAX_FACTS:
            raise MessageError("message.facts", f"must have up to {MAX_FACTS} pairs")
        for index, fact in enumerate(self.facts):
            _length(fact.label, f"message.facts[{index}].label", most=MAX_FACT_TEXT)
            _length(fact.value, f"message.facts[{index}].value", most=MAX_FACT_TEXT)
        if len(self.actions_done) > MAX_ACTIONS:
            raise MessageError("message.actions_done", f"must have up to {MAX_ACTIONS} actions")
        for index, action in enumerate(self.actions_done):
            _length(action, f"message.actions_done[{index}]", most=MAX_ACTION)
        if self.expected is not None:
            _length(self.expected, "message.expected", most=MAX_EXPECTED)
        if self.link is not None:
            try:
                check_link(self.link, "message.link")
            except ConfigurationError as error:
                raise MessageError("message.link", error.details[0]["message"]) from None


def _filled(text: str, path: str, data: Mapping[str, Any], variables: Mapping[str, str]) -> str:
    try:
        return substitute(text, data, variables)
    except TemplateError as error:
        raise MessageError(path, error.reason) from None


def compose(texts: MessageTexts, data: Mapping[str, Any], variables: Mapping[str, str]) -> NeutralMessage:
    """The message of a template version with the data of the request and the variables of the configuration."""

    def fill(text: str | None, path: str) -> str | None:
        return None if text is None else _filled(text, path, data, variables)

    return NeutralMessage(
        title=_filled(texts.title, "message.title", data, variables),
        severity=_filled(texts.severity, "message.severity", data, variables),
        summary=fill(texts.summary, "message.summary"),
        facts=tuple(
            Fact(
                label=_filled(fact.label, f"message.facts[{index}].label", data, variables),
                value=_filled(fact.value, f"message.facts[{index}].value", data, variables),
            )
            for index, fact in enumerate(texts.facts)
        ),
        actions_done=tuple(
            _filled(action, f"message.actions_done[{index}]", data, variables)
            for index, action in enumerate(texts.actions_done)
        ),
        expected=fill(texts.expected, "message.expected"),
        link=fill(texts.link, "message.link"),
    )
