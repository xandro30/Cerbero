import pytest

from hecate.domain.sealing.errors import InvalidInput
from hecate.domain.secrets.value_type import MAX_VALUE_BYTES, ValueType

PEM = (
    "-----BEGIN CERTIFICATE-----\n"
    "MIIBszCCAVmgAwIBAgIUAAAAAAAAAAAAAAAAAAAAAAAwCgYIKoZIzj0EAwIw\n"
    "-----END CERTIFICATE-----\n"
)


def test_parse_known_and_unknown_types():
    assert ValueType.parse("string") is ValueType.STRING
    assert ValueType.parse("json") is ValueType.JSON
    assert ValueType.parse("pem") is ValueType.PEM
    for bad in ("binary", "", "STRING", None, 3):
        with pytest.raises(InvalidInput):
            ValueType.parse(bad)  # type: ignore[arg-type]


def test_string_accepts_utf8_text_and_returns_its_bytes():
    assert ValueType.STRING.validate("pässwörd €") == "pässwörd €".encode()


def test_string_rejects_empty_and_non_text():
    with pytest.raises(InvalidInput):
        ValueType.STRING.validate("")
    with pytest.raises(InvalidInput):
        ValueType.STRING.validate(b"bytes")  # type: ignore[arg-type]
    with pytest.raises(InvalidInput):
        ValueType.STRING.validate("lone surrogate \ud800")


def test_json_accepts_valid_and_rejects_invalid():
    assert ValueType.JSON.validate('{"a": [1, 2]}') == b'{"a": [1, 2]}'
    for bad in ("{not json", "", "{'a': 1}"):
        with pytest.raises(InvalidInput):
            ValueType.JSON.validate(bad)


def test_pem_accepts_one_or_more_blocks():
    assert ValueType.PEM.validate(PEM) == PEM.encode()
    assert ValueType.PEM.validate(PEM + PEM)


@pytest.mark.parametrize(
    "text",
    [
        "not a pem",
        "-----BEGIN CERTIFICATE-----\n!!!not base64!!!\n-----END CERTIFICATE-----\n",
        "-----BEGIN CERTIFICATE-----\nMIIB\n-----END PRIVATE KEY-----\n",
        "-----BEGIN CERTIFICATE-----\nMIIB\n",
        "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n",
        "junk\n" + PEM,
        PEM + "junk\n",
    ],
)
def test_pem_rejects_malformed(text):
    with pytest.raises(InvalidInput):
        ValueType.PEM.validate(text)


@pytest.mark.parametrize("value_type,builder", [
    (ValueType.STRING, lambda n: "a" * n),
    (ValueType.JSON, lambda n: '"' + "a" * (n - 2) + '"'),
])
def test_size_limit_is_64_kib_for_every_type(value_type, builder):
    assert MAX_VALUE_BYTES == 64 * 1024
    assert value_type.validate(builder(MAX_VALUE_BYTES))
    with pytest.raises(InvalidInput):
        value_type.validate(builder(MAX_VALUE_BYTES + 1))


def test_size_limit_counts_bytes_not_characters():
    with pytest.raises(InvalidInput):
        ValueType.STRING.validate("€" * (MAX_VALUE_BYTES // 3 + 1))


def test_pem_over_the_limit_is_rejected():
    body = "A" * 64 + "\n"
    big = "-----BEGIN CERTIFICATE-----\n" + body * 1100 + "-----END CERTIFICATE-----\n"
    assert len(big) > MAX_VALUE_BYTES
    with pytest.raises(InvalidInput):
        ValueType.PEM.validate(big)
