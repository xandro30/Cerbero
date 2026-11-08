import json

from hecate.application.ports import CryptoProvider, KeyMaterial, SecretMaterial

KEY_SIZE = 32
FINGERPRINT_BYTES = 8
_FINGERPRINT_INFO = b"hecate/fingerprint"


def _aad(**fields: object) -> bytes:
    return json.dumps(fields, sort_keys=True, separators=(",", ":")).encode()


class Envelope:
    """Layered encryption: root -> scope key (one per identity) -> version key -> value.

    Every layer is AES-256-GCM with associated data that ties it to its purpose, path,
    version and identity, so moving encrypted data elsewhere makes it undecryptable (FR-004).
    """

    def __init__(self, crypto: CryptoProvider) -> None:
        self._crypto = crypto

    # -- scope key (wrapped by the root) --------------------------------------------------------

    @staticmethod
    def _scope_aad(identity: str, key_version: int) -> bytes:
        return _aad(purpose="scope_key", identity=identity, key_version=key_version)

    def new_scope_key(self, root: bytes, identity: str, key_version: int) -> tuple[bytes, KeyMaterial]:
        key = self._crypto.random_bytes(KEY_SIZE)
        nonce, ciphertext = self._crypto.aead_encrypt(root, key, self._scope_aad(identity, key_version))
        return key, KeyMaterial(nonce, ciphertext)

    def open_scope_key(self, root: bytes, identity: str, key_version: int, material: KeyMaterial) -> bytes:
        return self._crypto.aead_decrypt(
            root, material.nonce, material.ciphertext, self._scope_aad(identity, key_version)
        )

    # -- values (version key wrapped by the scope key) ------------------------------------------

    @staticmethod
    def _dek_aad(path: str, version: int, identity: str, key_version: int) -> bytes:
        return _aad(purpose="dek", path=path, version=version, scope=identity, key_version=key_version)

    @staticmethod
    def _value_aad(path: str, version: int, identity: str) -> bytes:
        return _aad(purpose="secret", path=path, version=version, scope=identity)

    def seal_value(
        self, scope_key: bytes, path: str, version: int, identity: str, key_version: int, value: bytes
    ) -> SecretMaterial:
        dek = self._crypto.random_bytes(KEY_SIZE)
        value_nonce, value_ciphertext = self._crypto.aead_encrypt(
            dek, value, self._value_aad(path, version, identity)
        )
        dek_nonce, dek_ciphertext = self._crypto.aead_encrypt(
            scope_key, dek, self._dek_aad(path, version, identity, key_version)
        )
        return SecretMaterial(key_version, value_nonce, value_ciphertext, dek_nonce, dek_ciphertext)

    def open_value(
        self, scope_key: bytes, path: str, version: int, identity: str, material: SecretMaterial
    ) -> bytes:
        dek = self._crypto.aead_decrypt(
            scope_key, material.dek_nonce, material.dek_ciphertext,
            self._dek_aad(path, version, identity, material.key_version),
        )
        return self._crypto.aead_decrypt(
            dek, material.value_nonce, material.value_ciphertext, self._value_aad(path, version, identity)
        )

    # -- fingerprint ----------------------------------------------------------------------------

    def fingerprint(self, scope_key: bytes, value: bytes) -> str:
        """Keyed fingerprint: only Hécate can compute it, so it does not help guess weak values."""
        key = self._crypto.hkdf_sha256(scope_key, _FINGERPRINT_INFO, KEY_SIZE)
        digest = self._crypto.hmac_sha256(key, value)[:FINGERPRINT_BYTES].hex()
        return "-".join(digest[i:i + 4] for i in range(0, len(digest), 4))
