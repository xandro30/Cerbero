import dataclasses

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from hecate.application.ports import CryptoError
from hecate.application.secrets.envelope import Envelope
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider

ROOT = bytes(range(32))
PATH = "secret://hermes/token"


@pytest.fixture(scope="module")
def envelope():
    return Envelope(SoftwareCryptoProvider())


@pytest.fixture(scope="module")
def scope_key(envelope):
    key, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    return key


def flip(data: bytes, index: int) -> bytes:
    i = index % len(data)
    return data[:i] + bytes([data[i] ^ 0x01]) + data[i + 1:]


def test_scope_key_is_created_and_opened_with_the_root(envelope):
    key, wrapped = envelope.new_scope_key(ROOT, "hermes", 1)
    assert len(key) == 32 and key != ROOT
    assert envelope.open_scope_key(ROOT, "hermes", 1, wrapped) == key
    assert key not in wrapped.ciphertext


def test_scope_key_does_not_open_with_another_root_identity_or_version(envelope):
    _, wrapped = envelope.new_scope_key(ROOT, "hermes", 1)
    with pytest.raises(CryptoError):
        envelope.open_scope_key(bytes(32), "hermes", 1, wrapped)
    with pytest.raises(CryptoError):
        envelope.open_scope_key(ROOT, "echidna", 1, wrapped)
    with pytest.raises(CryptoError):
        envelope.open_scope_key(ROOT, "hermes", 2, wrapped)


def test_each_scope_key_is_different(envelope):
    first, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    second, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    assert first != second


def test_seals_and_opens_a_value_through_the_version_key(envelope, scope_key):
    material = envelope.seal_value(scope_key, PATH, 1, "hermes", 1, b"s3cret-value")
    assert b"s3cret-value" not in material.value_ciphertext
    assert material.key_version == 1
    assert envelope.open_value(scope_key, PATH, 1, "hermes", material) == b"s3cret-value"


def test_every_version_gets_its_own_key(envelope, scope_key):
    first = envelope.seal_value(scope_key, PATH, 1, "hermes", 1, b"same")
    second = envelope.seal_value(scope_key, PATH, 1, "hermes", 1, b"same")
    assert first.dek_ciphertext != second.dek_ciphertext
    assert first.value_ciphertext != second.value_ciphertext


@given(st.binary(min_size=1, max_size=200), st.integers(min_value=0, max_value=10_000))
@settings(max_examples=60, deadline=None)
def test_altering_any_byte_prevents_decryption(value, index):
    envelope = Envelope(SoftwareCryptoProvider())
    key, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    material = envelope.seal_value(key, PATH, 1, "hermes", 1, value)
    assert envelope.open_value(key, PATH, 1, "hermes", material) == value
    for field in ("value_nonce", "value_ciphertext", "dek_nonce", "dek_ciphertext"):
        altered = dataclasses.replace(material, **{field: flip(getattr(material, field), index)})
        with pytest.raises(CryptoError):
            envelope.open_value(key, PATH, 1, "hermes", altered)


@given(
    st.sampled_from([
        ("secret://hermes/other", 1, "hermes", 1),  # other path
        (PATH, 2, "hermes", 1),                     # other version
        (PATH, 1, "echidna", 1),                    # other identity
        (PATH, 1, "hermes", 2),                     # other scope key version
    ])
)
def test_moving_the_encrypted_data_elsewhere_prevents_decryption(where):
    envelope = Envelope(SoftwareCryptoProvider())
    key, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    material = envelope.seal_value(key, PATH, 1, "hermes", 1, b"value")
    path, version, identity, key_version = where
    moved = dataclasses.replace(material, key_version=key_version)
    with pytest.raises(CryptoError):
        envelope.open_value(key, path, version, identity, moved)


def test_another_scope_key_cannot_open_the_value(envelope, scope_key):
    material = envelope.seal_value(scope_key, PATH, 1, "hermes", 1, b"value")
    other, _ = envelope.new_scope_key(ROOT, "hermes", 1)
    with pytest.raises(CryptoError):
        envelope.open_value(other, PATH, 1, "hermes", material)


def test_fingerprint_is_deterministic_for_the_same_identity_and_value(envelope, scope_key):
    assert envelope.fingerprint(scope_key, b"value") == envelope.fingerprint(scope_key, b"value")
    assert envelope.fingerprint(scope_key, b"value") != envelope.fingerprint(scope_key, b"value2")


def test_fingerprint_differs_between_identities_and_has_the_documented_shape(envelope, scope_key):
    other, _ = envelope.new_scope_key(ROOT, "echidna", 1)
    fp = envelope.fingerprint(scope_key, b"value")
    assert fp != envelope.fingerprint(other, b"value")
    groups = fp.split("-")
    assert len(groups) == 4 and all(len(g) == 4 and int(g, 16) >= 0 for g in groups)
    assert fp == fp.lower()


@given(st.binary(min_size=4, max_size=64))
def test_fingerprint_does_not_contain_the_value(value):
    envelope = Envelope(SoftwareCryptoProvider())
    key = bytes(32)
    fp = envelope.fingerprint(key, value).replace("-", "")
    assert value.hex() not in fp
