"""The reader of the configuration file (research R7, contracts/config-file.md#reglas-de-lectura).

Safe YAML in UTF-8 of up to 1 MiB, without anchors, aliases or repeated keys, and with JSON values only. Dates stay
text, so nothing is converted without quotes. Each fault says where it is, as a path inside the file
(`instances[0].name`), never with the value.
"""

from __future__ import annotations

import math
from typing import Any

import yaml

from hermes.domain.configuration.model import ConfigurationError

MAX_BYTES = 1024 * 1024
ROOT = "."
TIMESTAMP = "tag:yaml.org,2002:timestamp"

Segment = str | int


def render(segments: list[Segment]) -> str:
    out = ""
    for segment in segments:
        out += f"[{segment}]" if isinstance(segment, int) else (f".{segment}" if out else str(segment))
    return out or ROOT


class _Loader(yaml.SafeLoader):
    """The safe loader of PyYAML, with the path of the node it composes; anchors, aliases and repeated keys stop it."""

    def __init__(self, stream: str) -> None:
        super().__init__(stream)
        self._path: list[Segment] = []

    def compose_node(self, parent: yaml.Node | None, index: Any) -> yaml.Node | None:
        segment: Segment | None = None
        if isinstance(index, yaml.ScalarNode):
            segment = str(index.value)
        elif isinstance(index, int):
            segment = index
        if segment is not None:
            self._path.append(segment)
        try:
            if self.check_event(yaml.AliasEvent):
                raise ConfigurationError(render(self._path), "aliases are not allowed")
            node = super().compose_node(parent, index)
            if self.anchors:
                raise ConfigurationError(render(self._path), "anchors are not allowed")
            return node
        finally:
            if segment is not None:
                self._path.pop()

    def compose_mapping_node(self, anchor: Any) -> yaml.MappingNode:
        node = super().compose_mapping_node(anchor)
        seen: set[str] = set()
        for key, _ in node.value:
            if isinstance(key, yaml.ScalarNode):
                if key.value in seen:
                    raise ConfigurationError(render([*self._path, str(key.value)]), "the key repeats")
                seen.add(key.value)
        return node


_Loader.yaml_implicit_resolvers = {
    first: [(tag, pattern) for tag, pattern in resolvers if tag != TIMESTAMP]
    for first, resolvers in yaml.SafeLoader.yaml_implicit_resolvers.items()
}


def _json_only(value: Any, path: list[Segment]) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise ConfigurationError(render([*path, str(key)]), "the keys must be text")
            _json_only(item, [*path, key])
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _json_only(item, [*path, index])
    elif isinstance(value, float) and not math.isfinite(value):
        raise ConfigurationError(render(path), "numbers must be finite")
    elif not (value is None or isinstance(value, str | bool | int | float)):
        raise ConfigurationError(render(path), "only text, numbers, booleans, null, lists and maps are allowed")


def load_yaml(raw: bytes) -> Any:
    if len(raw) > MAX_BYTES:
        raise ConfigurationError(ROOT, "the file must be up to 1 MiB")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise ConfigurationError(ROOT, "the file must be UTF-8 text") from None
    loader = _Loader(text)
    try:
        node = loader.get_single_node()
        data = loader.construct_document(node) if node is not None else None
    except yaml.MarkedYAMLError as error:
        mark = error.problem_mark or error.context_mark
        where = f"line {mark.line + 1}" if mark is not None else ROOT
        raise ConfigurationError(where, "the file is not valid YAML") from None
    except yaml.YAMLError:
        raise ConfigurationError(ROOT, "the file is not valid YAML") from None
    finally:
        loader.dispose()
    _json_only(data, [])
    return data
