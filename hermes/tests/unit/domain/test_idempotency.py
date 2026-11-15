"""The fingerprint of a request and its Idempotency-Key (T088, research R11).

The fingerprint covers the method, the path and the canonical JSON of the body, so the same request gives the same
fingerprint whatever the order of its keys or its spaces, and any change gives another.
"""

import json
from typing import Any

import pytest
from hypothesis import given
from hypothesis import strategies as st

from hermes.domain.idempotency.key import idempotency_key, request_fingerprint
from hermes.domain.shared.errors import ErrorCode, HermesError

PATH = "/chat/v1/send_message"

json_values = st.recursive(
    st.none() | st.booleans() | st.integers() | st.floats(allow_nan=False, allow_infinity=False) | st.text(max_size=20),
    lambda children: st.lists(children, max_size=4) | st.dictionaries(st.text(max_size=10), children, max_size=4),
    max_leaves=12,
)
bodies = st.dictionaries(st.text(max_size=10), json_values, max_size=5)


def reordered(value: Any) -> Any:
    """The same value with the keys of each object in the opposite order."""
    if isinstance(value, dict):
        return {key: reordered(value[key]) for key in reversed(list(value))}
    if isinstance(value, list):
        return [reordered(item) for item in value]
    return value


@given(body=bodies, indent=st.sampled_from([None, 0, 2, 4]))
def test_the_same_request_gives_the_same_fingerprint_with_other_order_or_spaces(
    body: dict[str, Any], indent: int | None
) -> None:
    compact = json.dumps(body, separators=(",", ":"))
    spaced = json.dumps(reordered(body), indent=indent, separators=(", ", " : "))
    first = request_fingerprint("POST", PATH, json.loads(compact))
    assert first == request_fingerprint("POST", PATH, json.loads(spaced))
    assert len(first) == 32


@given(body=bodies, other=bodies)
def test_another_body_gives_another_fingerprint(body: dict[str, Any], other: dict[str, Any]) -> None:
    same = json.dumps(body, sort_keys=True) == json.dumps(other, sort_keys=True)
    assert (request_fingerprint("POST", PATH, body) == request_fingerprint("POST", PATH, other)) is same


def test_another_method_or_path_gives_another_fingerprint() -> None:
    body = {"recipients": ["contacto_demo"], "template": {"id": "device_unknown"}, "data": {}}
    fingerprint = request_fingerprint("POST", PATH, body)
    assert request_fingerprint("PUT", PATH, body) != fingerprint
    assert request_fingerprint("POST", "/chat/v1/other", body) != fingerprint


def test_the_parts_cannot_be_mixed_up() -> None:
    assert request_fingerprint("POST", "/a", "b") != request_fingerprint("POST", "/ab", "")


@pytest.mark.parametrize("key", ["k", "case-1234-notify", " ~", "x" * 128])
def test_a_key_has_from_1_to_128_printable_ascii_characters(key: str) -> None:
    assert idempotency_key(key) == key


@pytest.mark.parametrize("key", [None, "", "x" * 129, "clé", "tab\tkey", "line\nkey"])
def test_any_other_key_is_validation_with_its_path(key: str | None) -> None:
    with pytest.raises(HermesError) as raised:
        idempotency_key(key)
    assert raised.value.code is ErrorCode.VALIDATION
    assert [detail["path"] for detail in raised.value.details] == ["Idempotency-Key"]
