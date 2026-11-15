"""The keys and the request of Hermes (T053), as the integration guide of Hécate asks."""

from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519

from hermes.infrastructure.hecate.keys import generate_keys


def private(pem: str) -> object:
    return serialization.load_pem_private_key(pem.encode("ascii"), password=None)


def test_the_tls_key_is_p256_and_the_call_key_ed25519() -> None:
    keys = generate_keys()
    tls = private(keys.tls_key_pem.reveal())
    call = private(keys.call_key_pem.reveal())
    assert isinstance(tls, ec.EllipticCurvePrivateKey)
    assert tls.curve.name == "secp256r1"
    assert isinstance(call, ed25519.Ed25519PrivateKey)
    assert keys.tls_key_pem != keys.call_key_pem


def test_the_request_is_pkcs10_without_a_name_with_the_tls_key() -> None:
    keys = generate_keys()
    assert keys.csr_pem.startswith("-----BEGIN CERTIFICATE REQUEST-----")
    request = x509.load_pem_x509_csr(keys.csr_pem.encode("ascii"))
    assert request.is_signature_valid
    assert request.subject == x509.Name([])
    assert len(request.extensions) == 0
    tls = private(keys.tls_key_pem.reveal())
    assert isinstance(tls, ec.EllipticCurvePrivateKey)
    assert request.public_key() == tls.public_key()


def test_the_call_key_goes_as_a_public_key_pem() -> None:
    keys = generate_keys()
    assert keys.call_public_pem.startswith("-----BEGIN PUBLIC KEY-----")
    public = serialization.load_pem_public_key(keys.call_public_pem.encode("ascii"))
    call = private(keys.call_key_pem.reveal())
    assert isinstance(public, ed25519.Ed25519PublicKey)
    assert isinstance(call, ed25519.Ed25519PrivateKey)
    assert public == call.public_key()


def test_each_time_the_keys_are_new() -> None:
    first, second = generate_keys(), generate_keys()
    assert first.tls_key_pem != second.tls_key_pem
    assert first.call_key_pem != second.call_key_pem
    assert first.csr_pem != second.csr_pem


def test_the_private_keys_never_show() -> None:
    keys = generate_keys()
    assert "PRIVATE KEY" not in repr(keys)
    assert "PRIVATE KEY" not in str(keys)
