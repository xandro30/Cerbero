"""Checks responses and requests against the published contracts: hermes.v1.json and common.v1.json."""

from __future__ import annotations

import json
from collections.abc import Iterator
from functools import cache
from importlib import resources
from typing import Any
from urllib.parse import quote

from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012

BASE = "https://contracts.cerbero.home/openapi/"
DOCUMENTS = ("common.v1.json", "hermes.v1.json")


def load_document(name: str) -> dict[str, Any]:
    text = (resources.files("cerbero_contracts") / "openapi" / name).read_text(encoding="utf-8")
    document: dict[str, Any] = json.loads(text)
    return document


def references(value: Any) -> Iterator[str]:
    """Every `$ref` string inside a document."""
    if isinstance(value, dict):
        for key, member in value.items():
            if key == "$ref" and isinstance(member, str):
                yield member
            else:
                yield from references(member)
    elif isinstance(value, list):
        for member in value:
            yield from references(member)


def _pointer(*segments: str) -> str:
    escaped = (segment.replace("~", "~0").replace("/", "~1") for segment in segments)
    return "/" + "/".join(quote(segment, safe="~") for segment in escaped)


class Contract:
    def __init__(self) -> None:
        self.documents = {name: load_document(name) for name in DOCUMENTS}
        self.registry: Registry[Any] = Registry().with_resources(
            (BASE + name, Resource.from_contents(document, default_specification=DRAFT202012))
            for name, document in self.documents.items()
        )

    def operation(self, operation_id: str) -> tuple[str, str, dict[str, Any]]:
        for path, item in self.documents["hermes.v1.json"]["paths"].items():
            for method, operation in item.items():
                if isinstance(operation, dict) and operation.get("operationId") == operation_id:
                    return path, method, operation
        raise KeyError(operation_id)

    def operation_of(self, method: str, path: str) -> str | None:
        """The operation that a request with this method and this concrete path calls, if the contract has it."""
        given = path.split("/")
        for template, item in self.documents["hermes.v1.json"]["paths"].items():
            wanted = template.split("/")
            operation = item.get(method.lower())
            if not isinstance(operation, dict) or len(wanted) != len(given):
                continue
            if all(w == g or (w.startswith("{") and w.endswith("}") and g) for w, g in zip(wanted, given, strict=True)):
                return str(operation["operationId"])
        return None

    def response_uri(self, operation_id: str, status: int) -> str:
        path, method, operation = self.operation(operation_id)
        if str(status) not in operation["responses"]:
            raise KeyError(f"{operation_id} has no response {status}")
        pointer = _pointer("paths", path, method, "responses", str(status), "content", "application/json", "schema")
        return f"{BASE}hermes.v1.json#{pointer}"

    def request_uri(self, operation_id: str) -> str:
        path, method, _ = self.operation(operation_id)
        pointer = _pointer("paths", path, method, "requestBody", "content", "application/json", "schema")
        return f"{BASE}hermes.v1.json#{pointer}"

    @staticmethod
    def common_uri(name: str) -> str:
        return f"{BASE}common.v1.json#/components/schemas/{name}"

    @staticmethod
    def hermes_uri(name: str) -> str:
        return f"{BASE}hermes.v1.json#/components/schemas/{name}"

    def errors(self, uri: str, value: Any) -> list[str]:
        validator = Draft202012Validator({"$ref": uri}, registry=self.registry)
        return [error.message for error in validator.iter_errors(value)]

    def check(self, operation_id: str, status: int, body: Any) -> list[str]:
        return self.errors(self.response_uri(operation_id, status), body)

    def assert_valid(self, operation_id: str, status: int, body: Any) -> None:
        problems = self.check(operation_id, status, body)
        assert not problems, f"{operation_id} {status}: {problems}"


@cache
def contract() -> Contract:
    return Contract()
