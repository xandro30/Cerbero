"""The keys of Hermes and its certificate request (research R4), generated on its machine; they never leave it.

A P-256 key for TLS, a different Ed25519 key to sign the calls, a PKCS#10 request without a name (the token of Hécate
decides the identity) and the public call key in PEM `PUBLIC KEY`.
"""

from __future__ import annotations

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519

from hermes.application.ports import GeneratedKeys
from hermes.domain.shared.secret import SecretValue


def _private_pem(key: ec.EllipticCurvePrivateKey | ed25519.Ed25519PrivateKey) -> SecretValue:
    raw = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())
    return SecretValue(raw.decode("ascii"))


def generate_keys() -> GeneratedKeys:
    tls = ec.generate_private_key(ec.SECP256R1())
    call = ed25519.Ed25519PrivateKey.generate()
    request = x509.CertificateSigningRequestBuilder().subject_name(x509.Name([])).sign(tls, hashes.SHA256())
    public = call.public_key().public_bytes(serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo)
    return GeneratedKeys(
        tls_key_pem=_private_pem(tls),
        call_key_pem=_private_pem(call),
        csr_pem=request.public_bytes(serialization.Encoding.PEM).decode("ascii"),
        call_public_pem=public.decode("ascii"),
    )


def load_call_key(pem: SecretValue) -> ed25519.Ed25519PrivateKey:
    key = serialization.load_pem_private_key(pem.reveal().encode("ascii"), password=None)
    if not isinstance(key, ed25519.Ed25519PrivateKey):
        raise ValueError("the call key of Hermes must be an Ed25519 key")
    return key
