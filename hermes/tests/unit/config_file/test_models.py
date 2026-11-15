"""The models of the configuration file (T071, contracts/config-file.md#campos): strict, with the path of each fault."""

from typing import Any

import pytest
import yaml

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.config_file.models import SECRET_RULE, parse_configuration, read_configuration
from support.config_files import VALID_YAML
from support.configuration import configuration


def data() -> dict[str, Any]:
    loaded: dict[str, Any] = yaml.safe_load(VALID_YAML)
    return loaded


def faults(document: Any) -> list[dict[str, str]]:
    with pytest.raises(HermesError) as raised:
        parse_configuration(document)
    assert raised.value.code is ErrorCode.VALIDATION
    return raised.value.details


def paths(document: Any) -> list[str]:
    return [fault["path"] for fault in faults(document)]


def test_the_file_of_the_tests_is_the_configuration_of_the_tests() -> None:
    assert read_configuration(VALID_YAML) == configuration()


def test_an_unknown_field_is_refused_with_its_path() -> None:
    document = data()
    document["instances"][0]["colour"] = "red"
    document["owner"] = "admin"
    assert sorted(paths(document)) == ["instances[0].colour", "owner"]


@pytest.mark.parametrize(
    ("change", "path"),
    [
        (
            lambda d: d["templates"][0]["versions"][0]["message"].update(title=123),
            "templates[0].versions[0].message.title",
        ),
        (
            lambda d: d["templates"][0]["versions"][0]["message"].update(title=True),
            "templates[0].versions[0].message.title",
        ),
        (lambda d: d.update(version="1"), "version"),
        (lambda d: d["templates"][0]["versions"][0].update(version="1"), "templates[0].versions[0].version"),
        (
            lambda d: d["instances"][0].update(limits={"requests_per_minute": "20"}),
            "instances[0].limits.requests_per_minute",
        ),
        (lambda d: d["variables"].update(console_url=8080), "variables.console_url"),
    ],
)
def test_there_are_no_implicit_conversions(change: Any, path: str) -> None:
    document = data()
    change(document)
    assert paths(document) == [path]


@pytest.mark.parametrize(
    ("change", "path"),
    [
        (lambda d: d["instances"][0].update(name="Family-Bot"), "instances[0].name"),
        (lambda d: d["contacts"][0].update(name="contacto demo"), "contacts[0].name"),
        (lambda d: d["templates"][0].update(id="9device"), "templates[0].id"),
        (lambda d: d["variables"].update({"Console": "x"}), "variables.Console"),
    ],
)
def test_names_follow_the_rule(change: Any, path: str) -> None:
    document = data()
    change(document)
    assert paths(document) == [path]


@pytest.mark.parametrize("version", [2, 0, None])
def test_the_version_of_the_format_is_1(version: Any) -> None:
    document = data()
    document["version"] = version
    assert paths(document) == ["version"]


def test_a_secret_field_takes_only_a_secret_reference_and_never_shows_the_value() -> None:
    document = data()
    document["instances"][0]["secrets"]["bot_token"] = "123456:the-real-token"
    [fault] = faults(document)
    assert fault == {"path": "instances[0].secrets.bot_token", "message": SECRET_RULE}
    assert "123456:the-real-token" not in fault["message"]


def test_a_repeated_name_is_refused_where_it_repeats() -> None:
    document = data()
    document["instances"].append(dict(document["instances"][1]))
    document["contacts"].append(
        {"name": "contacto_demo", "channels": [{"service": "chat", "provider": "fake", "instance": "test_fake"}]}
    )
    assert sorted(paths(document)) == ["contacts[3].name", "instances[2].name"]


@pytest.mark.parametrize(
    ("change", "path"),
    [
        (lambda d: d["templates"][0].update(versions=[]), "templates[0].versions"),
        (lambda d: d["contacts"][0].update(channels=[]), "contacts[0].channels"),
        (lambda d: d["variables"].update(console_url="x" * 4097), "variables.console_url"),
        (
            lambda d: d["instances"][0].update(limits={"requests_per_minute": 601}),
            "instances[0].limits.requests_per_minute",
        ),
        (
            lambda d: d["contacts"][0]["channels"][0].update(service="email"),
            "contacts[0].channels[0].service",
        ),
        (
            lambda d: d["templates"][0]["versions"][1].update(version=1),
            "templates[0].versions[1].version",
        ),
        (
            lambda d: d["templates"][0]["versions"][0]["message"].update(severity="urgent"),
            "templates[0].versions[0].message.severity",
        ),
        (
            lambda d: d["templates"][0]["versions"][0]["message"].update(link="http://cerbero.home"),
            "templates[0].versions[0].message.link",
        ),
    ],
)
def test_each_rule_of_the_fields_says_where_it_fails(change: Any, path: str) -> None:
    document = data()
    change(document)
    assert paths(document) == [path]


def test_every_fault_comes_together_and_the_message_counts_them() -> None:
    document = data()
    document["version"] = 3
    document["instances"][0]["name"] = "Bad"
    with pytest.raises(HermesError) as raised:
        parse_configuration(document)
    assert len(raised.value.details) == 2
    assert "2" in raised.value.message


def test_a_document_that_is_not_a_map_is_refused() -> None:
    assert paths(["version", 1]) == ["."]
