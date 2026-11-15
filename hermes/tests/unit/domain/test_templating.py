"""Substitution in the texts of a template (T026): `${data.<path>}` and `${vars.<name>}`, and nothing else."""

from typing import Any

import pytest
from hypothesis import given
from hypothesis import strategies as st

from hermes.domain.messages.templating import TemplateError, substitute
from hermes.domain.shared.errors import ErrorCode

DATA: dict[str, Any] = {
    "mac": "AA:BB:CC:DD:EE:FF",
    "device": {"ip": "192.168.1.57", "ports": [22, 80], "seen": {"first": "ayer"}},
    "count": 3,
    "blocked": True,
    "nothing": None,
    "ratio": 0.5,
    "with-dash_and_9": "ok",
}
VARIABLES = {"console_url": "https://cerbero.home"}


def test_a_data_path_goes_into_the_data_by_dots() -> None:
    assert (
        substitute("MAC ${data.mac}, IP ${data.device.ip}", DATA, VARIABLES) == "MAC AA:BB:CC:DD:EE:FF, IP 192.168.1.57"
    )
    assert substitute("${data.device.seen.first}", DATA, VARIABLES) == "ayer"
    assert substitute("${data.with-dash_and_9}", DATA, VARIABLES) == "ok"


def test_a_variable_comes_from_the_configuration() -> None:
    assert substitute("${vars.console_url}/cases/1", DATA, VARIABLES) == "https://cerbero.home/cases/1"


def test_a_value_that_is_not_text_is_written_as_its_json() -> None:
    assert (
        substitute("${data.count} ${data.blocked} ${data.nothing} ${data.ratio}", DATA, VARIABLES) == "3 true null 0.5"
    )
    assert substitute("${data.device.ports}", DATA, VARIABLES) == "[22, 80]"
    assert substitute("${data.device.seen}", DATA, VARIABLES) == '{"first": "ayer"}'


@pytest.mark.parametrize(
    "text",
    ["${data.missing}", "${data.device.missing}", "${data.mac.inner}", "${data.device.ports.0}", "${vars.missing}"],
)
def test_a_missing_value_is_an_error(text: str) -> None:
    with pytest.raises(TemplateError) as raised:
        substitute(text, DATA, VARIABLES)
    assert raised.value.code is ErrorCode.VALIDATION
    assert text in raised.value.message


@pytest.mark.parametrize(
    "text",
    [
        "${mac}",
        "${data}",
        "${data.}",
        "${data..mac}",
        "${ data.mac }",
        "${vars.console_url.x}",
        "${vars.Console}",
        "${input.url}",
        "${data.mac|upper}",
        "${}",
        "${data.mac",
        "abc ${",
        "fine ${data.mac} then ${broken",
    ],
)
def test_any_other_form_of_a_placeholder_is_an_error(text: str) -> None:
    with pytest.raises(TemplateError) as raised:
        substitute(text, DATA, VARIABLES)
    assert raised.value.code is ErrorCode.VALIDATION


def test_a_value_is_written_as_it_is_and_never_substituted_again() -> None:
    assert substitute("${data.text}", {"text": "${vars.console_url}"}, VARIABLES) == "${vars.console_url}"


def test_the_error_names_the_placeholder_but_never_the_data() -> None:
    with pytest.raises(TemplateError) as raised:
        substitute("${data.reason}", {"other": "valor-privado"}, VARIABLES)
    assert "${data.reason}" in raised.value.message
    assert "valor-privado" not in raised.value.message


@given(st.text().filter(lambda text: "${" not in text))
def test_a_text_without_placeholders_does_not_change(text: str) -> None:
    assert substitute(text, DATA, VARIABLES) == text
