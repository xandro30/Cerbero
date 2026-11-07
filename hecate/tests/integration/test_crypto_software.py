import os

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from hecate.application.ports import CryptoError
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider

crypto = SoftwareCryptoProvider()
MIN_OPS, MIN_MEM = 1, 8192


def test_aead_roundtrip_with_associated_data():
    key = os.urandom(32)
    nonce, ct = crypto.aead_encrypt(key, b"secret", b"aad")
    assert len(nonce) == 12
    assert crypto.aead_decrypt(key, nonce, ct, b"aad") == b"secret"


@settings(max_examples=60, deadline=None)
@given(
    plaintext=st.binary(min_size=1, max_size=64),
    aad=st.binary(max_size=32),
    which=st.sampled_from(["nonce", "ciphertext", "aad"]),
    data=st.data(),
)
def test_aead_any_altered_byte_fails(plaintext, aad, which, data):
    key = os.urandom(32)
    nonce, ct = crypto.aead_encrypt(key, plaintext, aad)
    target = {"nonce": nonce, "ciphertext": ct, "aad": aad}[which]
    if not target:
        return
    pos = data.draw(st.integers(0, len(target) - 1))
    flipped = bytearray(target)
    flipped[pos] ^= 0x01
    args = {"nonce": nonce, "ciphertext": ct, "aad": aad}
    args[which] = bytes(flipped)
    with pytest.raises(CryptoError):
        crypto.aead_decrypt(key, args["nonce"], args["ciphertext"], args["aad"])


def test_aead_wrong_key_fails():
    nonce, ct = crypto.aead_encrypt(os.urandom(32), b"x", b"")
    with pytest.raises(CryptoError):
        crypto.aead_decrypt(os.urandom(32), nonce, ct, b"")


def test_derive_key_is_deterministic_and_depends_on_inputs():
    salt = os.urandom(16)
    k1 = crypto.derive_key("a long passphrase", salt, MIN_OPS, MIN_MEM)
    assert len(k1) == 32
    assert k1 == crypto.derive_key("a long passphrase", salt, MIN_OPS, MIN_MEM)
    assert k1 != crypto.derive_key("another passphrase", salt, MIN_OPS, MIN_MEM)
    assert k1 != crypto.derive_key("a long passphrase", os.urandom(16), MIN_OPS, MIN_MEM)


@settings(max_examples=40, deadline=None)
@given(secret=st.binary(min_size=32, max_size=32))
def test_shamir_any_pair_recovers_and_one_never(secret):
    shares = crypto.shamir_split(secret)
    assert [i for i, _ in shares] == [1, 2, 3]
    assert all(len(f) == 32 for _, f in shares)
    for a in range(3):
        for b in range(3):
            if a != b:
                assert crypto.shamir_combine([shares[a], shares[b]]) == secret
    for single in shares:
        with pytest.raises(CryptoError):
            crypto.shamir_combine([single])


def test_shamir_pair_from_different_sets_does_not_recover():
    secret = os.urandom(32)
    one = crypto.shamir_split(secret)
    two = crypto.shamir_split(secret)
    assert crypto.shamir_combine([one[0], two[1]]) != secret


def test_random_bytes_length():
    assert len(crypto.random_bytes(16)) == 16
