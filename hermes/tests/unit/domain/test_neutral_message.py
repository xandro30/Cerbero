"""The neutral message after substitution (T027): each field keeps its rule of data-model.md, or it is validation."""

from typing import Any

import pytest

from hermes.domain.configuration.model import Fact, MessageTexts
from hermes.domain.messages.neutral import NeutralMessage, compose
from hermes.domain.shared.errors import ErrorCode, HermesError
from support.configuration import DATA, DEVICE_TEXTS_V1

VARIABLES = {"console_url": "https://cerbero.home"}


def test_compose_fills_every_field_with_the_data_and_the_variables() -> None:
    assert compose(DEVICE_TEXTS_V1, DATA, VARIABLES) == NeutralMessage(
        title="Dispositivo desconocido en la red",
        severity="high",
        summary="Ha aparecido AA:BB:CC:DD:EE:FF con la IP 192.168.1.57.",
        facts=(Fact("MAC", "AA:BB:CC:DD:EE:FF"), Fact("IP", "192.168.1.57")),
        actions_done=("Bloqueado mientras decides",),
        expected="Acepta o bloquea el dispositivo",
        link="https://cerbero.home/cases/case-1234",
    )


def test_the_optional_fields_may_be_absent() -> None:
    message = compose(MessageTexts(title="Aviso", severity="info"), {}, VARIABLES)
    assert message == NeutralMessage(title="Aviso", severity="info")


def templated(field: str) -> MessageTexts:
    base: dict[str, Any] = {"title": "Aviso", "severity": "info"}
    if field == "facts.label":
        base["facts"] = (Fact("${data.v}", "valor"),)
    elif field == "facts.value":
        base["facts"] = (Fact("etiqueta", "${data.v}"),)
    elif field == "actions_done":
        base["actions_done"] = ("${data.v}",)
    else:
        base[field] = "${data.v}"
    return MessageTexts(**base)


@pytest.mark.parametrize(
    ("field", "value", "path"),
    [
        ("title", "", "message.title"),
        ("title", "x" * 257, "message.title"),
        ("severity", "urgent", "message.severity"),
        ("summary", "x" * 2049, "message.summary"),
        ("facts.label", "x" * 257, "message.facts[0].label"),
        ("facts.value", "x" * 257, "message.facts[0].value"),
        ("actions_done", "x" * 257, "message.actions_done[0]"),
        ("expected", "x" * 513, "message.expected"),
        ("link", "http://cerbero.home/cases/1", "message.link"),
        ("link", "not a url", "message.link"),
        ("link", "https://cerbero.home/" + "x" * 2048, "message.link"),
        ("title", "${" + "x" * 300, "message.title"),
    ],
)
def test_a_field_that_breaks_its_rule_after_substitution_is_validation(field: str, value: str, path: str) -> None:
    with pytest.raises(HermesError) as raised:
        compose(templated(field), {"v": value}, VARIABLES)
    assert raised.value.code is ErrorCode.VALIDATION
    assert raised.value.details == [{"path": path, "message": raised.value.details[0]["message"]}]
    assert value not in raised.value.message or value == ""


@pytest.mark.parametrize(
    ("field", "value"),
    [("title", "x" * 256), ("severity", "critical"), ("summary", "x" * 2048), ("expected", "x" * 512)],
)
def test_a_field_at_its_limit_is_accepted(field: str, value: str) -> None:
    message = compose(templated(field), {"v": value}, VARIABLES)
    assert getattr(message, field) == value


def test_a_failed_substitution_says_in_which_field() -> None:
    texts = MessageTexts(title="Aviso", severity="info", summary="Falta ${data.missing}")
    with pytest.raises(HermesError) as raised:
        compose(texts, {}, VARIABLES)
    assert raised.value.code is ErrorCode.VALIDATION
    assert raised.value.details[0]["path"] == "message.summary"
    assert "${data.missing}" in raised.value.details[0]["message"]
