"""The reader of the configuration file (T070, contracts/config-file.md#reglas-de-lectura): safe YAML, with paths."""

import pytest

from hermes.domain.configuration.model import ConfigurationError
from hermes.infrastructure.config_file.loader import MAX_BYTES, load_yaml
from support.config_files import VALID_YAML


def refused(raw: bytes) -> ConfigurationError:
    with pytest.raises(ConfigurationError) as raised:
        load_yaml(raw)
    return raised.value


def test_a_valid_file_is_read() -> None:
    data = load_yaml(VALID_YAML.encode())
    assert data["version"] == 1
    assert data["contacts"][0]["name"] == "contacto_demo"


@pytest.mark.parametrize(
    ("raw", "path"),
    [
        (b"version: 1\ninstances:\n  - name: a\n    name: b\n", "instances[0].name"),
        (b"version: 1\nversion: 2\n", "version"),
        (b"version: 1\nvariables:\n  a: x\n  a: y\n", "variables.a"),
    ],
)
def test_a_repeated_key_is_refused_with_its_path(raw: bytes, path: str) -> None:
    error = refused(raw)
    assert error.path == path
    assert "repeat" in error.message


@pytest.mark.parametrize(
    ("raw", "path"),
    [
        (b"version: 1\nvariables: &shared\n  a: b\n", "variables"),
        (b"version: 1\nvariables:\n  a: &value hola\n  b: *value\n", "variables.a"),
        (b"base: &base { x: 1 }\nother:\n  <<: *base\n", "base"),
        (b"version: 1\nlist:\n  - &item one\n", "list[0]"),
    ],
)
def test_anchors_and_aliases_are_refused_with_their_path(raw: bytes, path: str) -> None:
    assert refused(raw).path == path


def test_a_file_of_more_than_1_mib_is_refused() -> None:
    error = refused(b"version: 1\n" + b"#" * MAX_BYTES)
    assert MAX_BYTES == 1024 * 1024
    assert error.path == "."
    assert "1 MiB" in error.message


def test_text_that_is_not_utf8_is_refused() -> None:
    error = refused(b"version: 1\nvariables:\n  a: \xff\xfe\n")
    assert error.path == "."
    assert "UTF-8" in error.message


def test_a_date_stays_text() -> None:
    assert load_yaml(b"version: 1\nvariables:\n  day: 2026-11-14\n")["variables"]["day"] == "2026-11-14"


@pytest.mark.parametrize(
    ("raw", "path"),
    [
        (b"version: 1\nitems: !!set { a: null }\n", "items"),
        (b"version: 1\n1: a\n", "1"),
        (b"version: 1\nratio: .nan\n", "ratio"),
        (b"version: 1\nblob: !!binary aGVsbG8=\n", "blob"),
    ],
)
def test_only_json_values_are_read(raw: bytes, path: str) -> None:
    assert refused(raw).path == path


def test_a_file_that_is_not_yaml_says_where() -> None:
    error = refused(b"version: [1\nother: 2\n")
    assert error.path.startswith("line ")
    assert "YAML" in error.message


def test_an_empty_file_is_an_empty_document() -> None:
    assert load_yaml(b"") is None
