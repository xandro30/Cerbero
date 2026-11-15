"""The reader of `X-Forwarded-Client-Cert`, in the text format of Envoy (contracts/proxy.md#la-cabecera-que-lee-hermes).

Elements go separated by `,` and fields by `;`; a value with one of `,;="` goes between quotes, with `\\"` inside.
With `SANITIZE_SET` there is one element with `Hash`, `Cert`, `Chain` and `URI`, and `By` when Envoy adds it.
"""

from __future__ import annotations

import hashlib
from urllib.parse import unquote

from cryptography import x509
from cryptography.hazmat.primitives import serialization

from hermes.application.ports import PresentedCertificate

REQUIRED = ("hash", "cert", "chain", "uri")


class XfccError(ValueError):
    """A header that does not follow the format: the call answers `auth`. Its message never carries the header."""


def _split(text: str, separator: str) -> list[str]:
    parts: list[str] = []
    current: list[str] = []
    quoted = escaped = False
    for char in text:
        if escaped:
            escaped = False
        elif quoted and char == "\\":
            escaped = True
        elif char == '"':
            quoted = not quoted
        elif char == separator and not quoted:
            parts.append("".join(current))
            current = []
            continue
        current.append(char)
    if quoted:
        raise XfccError("a quoted value is not closed")
    parts.append("".join(current))
    return parts


def _value(raw: str) -> str:
    if len(raw) >= 2 and raw[0] == raw[-1] == '"':
        return raw[1:-1].replace('\\"', '"')
    if '"' in raw:
        raise XfccError("a value has a quote out of place")
    return raw


def _fields(element: str) -> dict[str, list[str]]:
    fields: dict[str, list[str]] = {}
    for pair in _split(element, ";"):
        key, separator, raw = pair.partition("=")
        if not separator or not key.strip():
            raise XfccError("a field has no key or no value")
        fields.setdefault(key.strip().lower(), []).append(_value(raw.strip()))
    return fields


def _once(fields: dict[str, list[str]], name: str) -> str:
    values = fields.get(name, [])
    if len(values) != 1:
        raise XfccError(f"{name} must appear once")
    return values[0]


def parse_xfcc(header: str) -> PresentedCertificate:
    elements = _split(header, ",")
    if len(elements) != 1:
        raise XfccError("the header must have exactly one element")
    fields = _fields(elements[0])
    hash_hex, cert, chain, uri = (_once(fields, name) for name in REQUIRED)
    by = fields.get("by", [])
    if len(by) > 1:
        raise XfccError("by must appear at most once")
    try:
        certificate = x509.load_pem_x509_certificate(unquote(cert).encode("ascii"))
        chain_certificates = tuple(x509.load_pem_x509_certificates(unquote(chain).encode("ascii")))
    except ValueError, TypeError:
        raise XfccError("cert and chain must be certificates in URL-encoded PEM") from None
    digest = hashlib.sha256(certificate.public_bytes(serialization.Encoding.DER)).hexdigest()
    if digest != hash_hex.lower():
        raise XfccError("Hash is not the one of Cert")
    if not uri:
        raise XfccError("uri must not be empty")
    return PresentedCertificate(certificate=certificate, chain=chain_certificates, uri=uri, by=by[0] if by else None)
