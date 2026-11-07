import os

import nacl.exceptions
import nacl.pwhash.argon2id
from Crypto.Protocol.SecretSharing import Shamir
from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from hecate.application.ports import CryptoError

KEY_SIZE = 32
NONCE_SIZE = 12
BLOCK = 16
THRESHOLD = 2
SHARE_COUNT = 3


class SoftwareCryptoProvider:
    """CryptoProvider backed by audited libraries: cryptography, PyNaCl and PyCryptodome."""

    def random_bytes(self, n: int) -> bytes:
        return os.urandom(n)

    def derive_key(self, passphrase: str, salt: bytes, opslimit: int, memlimit: int) -> bytes:
        try:
            return nacl.pwhash.argon2id.kdf(
                KEY_SIZE, passphrase.encode(), salt, opslimit=opslimit, memlimit=memlimit
            )
        except (nacl.exceptions.CryptoError, nacl.exceptions.ValueError) as exc:
            raise CryptoError("key derivation failed") from exc

    def aead_encrypt(self, key: bytes, plaintext: bytes, aad: bytes) -> tuple[bytes, bytes]:
        nonce = os.urandom(NONCE_SIZE)
        return nonce, AESGCM(key).encrypt(nonce, plaintext, aad)

    def aead_decrypt(self, key: bytes, nonce: bytes, ciphertext: bytes, aad: bytes) -> bytes:
        try:
            return AESGCM(key).decrypt(nonce, ciphertext, aad)
        except (InvalidTag, ValueError) as exc:
            raise CryptoError("decryption failed") from exc

    def shamir_split(self, secret32: bytes) -> list[tuple[int, bytes]]:
        if len(secret32) != KEY_SIZE:
            raise CryptoError("secret must have 32 bytes")
        # PyCryptodome only splits 16-byte blocks: split each half with the same indices.
        halves = [secret32[:BLOCK], secret32[BLOCK:]]
        splits = [dict(Shamir.split(THRESHOLD, SHARE_COUNT, h, ssss=False)) for h in halves]
        return [(i, splits[0][i] + splits[1][i]) for i in sorted(splits[0])]

    def shamir_combine(self, shares: list[tuple[int, bytes]]) -> bytes:
        indices = [i for i, _ in shares]
        if len(set(indices)) < THRESHOLD or len(set(indices)) != len(indices):
            raise CryptoError("not enough distinct shares")
        if any(len(f) != KEY_SIZE for _, f in shares):
            raise CryptoError("invalid share length")
        first = Shamir.combine([(i, f[:BLOCK]) for i, f in shares], ssss=False)
        second = Shamir.combine([(i, f[BLOCK:]) for i, f in shares], ssss=False)
        return first + second
