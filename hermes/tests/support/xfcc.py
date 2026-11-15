"""`X-Forwarded-Client-Cert` as Envoy writes it with `SANITIZE_SET` (contracts/proxy.md#la-cabecera-que-lee-hermes)."""

from __future__ import annotations

import hashlib
from collections.abc import Sequence
from urllib.parse import quote

from cryptography import x509
from cryptography.hazmat.primitives import serialization

from support.pki import spiffe

HERMES = spiffe("hermes")


def pem(certificate: x509.Certificate) -> str:
    return certificate.public_bytes(serialization.Encoding.PEM).decode("ascii")


def certificate_hash(certificate: x509.Certificate) -> str:
    return hashlib.sha256(certificate.public_bytes(serialization.Encoding.DER)).hexdigest()


def san_uri(certificate: x509.Certificate) -> str:
    names = certificate.extensions.get_extension_for_class(x509.SubjectAlternativeName).value
    return str(names.get_values_for_type(x509.UniformResourceIdentifier)[0])


def xfcc(
    certificate: x509.Certificate,
    intermediates: Sequence[x509.Certificate],
    *,
    uri: str | None = None,
    by: str | None = HERMES,
) -> str:
    """One element with `By`, `Hash`, `Cert`, `Chain` (the certificate and its intermediates) and `URI`."""
    chain = "".join(pem(item) for item in (certificate, *intermediates))
    fields = [f"By={by}"] if by is not None else []
    fields += [
        f"Hash={certificate_hash(certificate)}",
        f'Cert="{quote(pem(certificate), safe="")}"',
        f'Chain="{quote(chain, safe="")}"',
        f"URI={uri if uri is not None else san_uri(certificate)}",
    ]
    return ";".join(fields)
