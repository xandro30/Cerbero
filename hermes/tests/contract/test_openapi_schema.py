"""The published contracts (T005): OpenAPI 3.1 in JSON, every $ref resolved and the examples of contracts/api.md."""

import copy
from typing import Any

import pytest

from support.contract import BASE, contract, load_document, references

CODES = [
    "validation",
    "auth",
    "not_found",
    "conflict",
    "rate_limited",
    "provider_unavailable",
    "timeout",
    "unavailable",
    "internal",
]


def completed(data: dict[str, Any], **meta: Any) -> dict[str, Any]:
    return {
        "status": "completed",
        "data": data,
        "error": None,
        "meta": {"request_id": "req-1", "duration_ms": 3, **meta},
    }


def failed(code: str, **error: Any) -> dict[str, Any]:
    retryable = code in {"rate_limited", "provider_unavailable", "unavailable", "internal"}
    return {
        "status": "failed",
        "data": {},
        "error": {"code": code, "message": "a message", "retryable": retryable, **error},
        "meta": {"request_id": "req-1", "duration_ms": 3},
    }


SENT = completed(
    {
        "recipient": "contacto_demo",
        "instance": "family_bot",
        "provider_message_ref": "4521",
        "sent_at": "2026-11-14T10:00:00.000000Z",
        "truncated": False,
    },
    service="chat",
    provider="telegram",
    idempotency_key="case-1234-notify",
)

CAPABILITIES = completed(
    {
        "component": "hermes",
        "services": [
            {
                "category": "communication",
                "service": "chat",
                "contract": "v1",
                "providers": [
                    {
                        "provider": "telegram",
                        "version": "1.0.0",
                        "functions": [{"name": "send_message", "kind": "command", "mode": "sync", "timeout_s": 10}],
                        "instances": [{"name": "family_bot", "status": "healthy"}],
                    }
                ],
            }
        ],
    }
)

TESTED = completed(
    {"instance": "family_bot", "status": "healthy", "checked_at": "2026-11-14T10:00:00.000000Z"},
    provider="telegram",
)

REQUEST = {
    "recipients": ["contacto_demo"],
    "template": {"id": "device_unknown", "version": 1},
    "data": {"mac": "AA:BB:CC:DD:EE:FF", "ip": "192.168.1.57", "case_id": "case-1234"},
    "instance": "family_bot",
}


def test_both_documents_are_openapi_3_1_with_their_license_and_version() -> None:
    for name, version in (("common.v1.json", "1.1.0"), ("hermes.v1.json", "1.0.1")):
        document = load_document(name)
        assert document["openapi"] == "3.1.0", name
        assert document["info"]["version"] == version, name
        assert document["info"]["license"]["identifier"] == "AGPL-3.0-or-later", name


def test_every_reference_resolves() -> None:
    known = contract()
    for name, document in known.documents.items():
        found = list(references(document))
        assert found, name
        resolver = known.registry.resolver(base_uri=BASE + name)
        for reference in found:
            resolver.lookup(reference)


def test_hermes_only_has_the_three_operations() -> None:
    paths = load_document("hermes.v1.json")["paths"]
    operations = {
        (path, method): operation["operationId"]
        for path, item in paths.items()
        for method, operation in item.items()
        if isinstance(operation, dict) and "operationId" in operation
    }
    assert operations == {
        ("/chat/v1/send_message", "post"): "sendMessage",
        ("/v1/capabilities", "get"): "getCapabilities",
        ("/v1/instances/{name}/test", "post"): "testInstance",
    }


@pytest.mark.parametrize(
    ("operation_id", "status", "body"),
    [
        ("sendMessage", 200, SENT),
        ("sendMessage", 200, failed("provider_unavailable")),
        ("sendMessage", 200, failed("rate_limited", retryable=True, retry_after_s=30)),
        ("sendMessage", 200, failed("timeout", outcome_unknown=True)),
        ("sendMessage", 200, failed("conflict")),
        ("sendMessage", 400, failed("validation")),
        ("sendMessage", 401, failed("auth")),
        ("sendMessage", 404, failed("not_found")),
        ("sendMessage", 409, failed("conflict")),
        ("sendMessage", 429, failed("rate_limited", retry_after_s=2)),
        ("sendMessage", 500, failed("internal")),
        ("sendMessage", 503, failed("unavailable")),
        ("getCapabilities", 200, CAPABILITIES),
        ("getCapabilities", 401, failed("auth")),
        ("testInstance", 200, TESTED),
        ("testInstance", 200, failed("provider_unavailable")),
        ("testInstance", 404, failed("not_found")),
    ],
)
def test_each_example_of_the_contract_validates(operation_id: str, status: int, body: dict[str, Any]) -> None:
    contract().assert_valid(operation_id, status, body)


def test_a_response_with_a_field_more_or_less_does_not_validate() -> None:
    known = contract()
    extra = copy.deepcopy(SENT)
    extra["data"]["chat_id"] = "123456"
    assert known.check("sendMessage", 200, extra)
    missing = copy.deepcopy(SENT)
    del missing["data"]["provider_message_ref"]
    assert known.check("sendMessage", 200, missing)
    capabilities = copy.deepcopy(CAPABILITIES)
    capabilities["data"]["contacts"] = []
    assert known.check("getCapabilities", 200, capabilities)


def test_each_status_only_takes_its_own_code() -> None:
    known = contract()
    assert known.check("sendMessage", 400, failed("auth"))
    assert known.check("sendMessage", 200, failed("validation"))
    assert known.check("testInstance", 404, failed("internal"))


def test_the_send_message_request_follows_its_schema() -> None:
    known = contract()
    uri = known.request_uri("sendMessage")
    assert not known.errors(uri, REQUEST)
    assert not known.errors(uri, {key: value for key, value in REQUEST.items() if key != "instance"})
    for broken in (
        {**REQUEST, "recipients": []},
        {**REQUEST, "recipients": ["contacto_demo", "otro"]},
        {**REQUEST, "template": {"id": "device_unknown", "version": 0}},
        {**REQUEST, "recipients": ["Contacto"]},
        {**REQUEST, "extra": True},
    ):
        assert known.errors(uri, broken), broken


def test_the_common_meta_takes_the_optional_fields_of_hermes() -> None:
    known = contract()
    uri = known.common_uri("Completed")
    assert not known.errors(uri, SENT)
    assert known.errors(uri, completed({}, service="Chat!"))
    assert known.errors(uri, completed({}, idempotency_key=""))
    for code in CODES:
        assert not known.errors(known.common_uri("Failed"), failed(code)), code


def test_send_message_declares_its_function_contract() -> None:
    _, _, operation = contract().operation("sendMessage")
    assert operation["x-cerbero-function"] == {
        "kind": "command",
        "mode": "sync",
        "timeout": "10s",
        "side_effect": "Envía a ${recipient} el mensaje «${title}» por la instancia ${instance}",
        "discloses_data": "${provider} recibe el texto del mensaje y el chat de ${recipient}",
        "non_reversible": True,
    }
