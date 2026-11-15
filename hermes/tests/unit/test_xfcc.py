"""The reader of `X-Forwarded-Client-Cert` (T043, contracts/proxy.md#la-cabecera-que-lee-hermes)."""

import contextlib
from urllib.parse import quote

import pytest
from hypothesis import given
from hypothesis import strategies as st

from hermes.infrastructure.api.xfcc import XfccError, parse_xfcc
from support.pki import TestCa, spiffe
from support.xfcc import HERMES, certificate_hash, pem, xfcc

CA = TestCa()
LEAF = CA.issue("hades")


def test_one_element_with_hash_cert_chain_and_uri_gives_the_presented_certificate() -> None:
    presented = parse_xfcc(xfcc(LEAF.certificate, [CA.intermediate]))
    assert presented.certificate == LEAF.certificate
    assert presented.chain == (LEAF.certificate, CA.intermediate)
    assert presented.uri == spiffe("hades")
    assert presented.by == HERMES


def test_by_is_optional() -> None:
    assert parse_xfcc(xfcc(LEAF.certificate, [CA.intermediate], by=None)).by is None


def test_keys_are_case_insensitive_and_values_may_go_without_quotes() -> None:
    header = ";".join(
        [
            f"hash={certificate_hash(LEAF.certificate).upper()}",
            f"CERT={quote(pem(LEAF.certificate), safe='')}",
            f'chain="{quote(pem(LEAF.certificate) + pem(CA.intermediate), safe="")}"',
            f'Uri="{spiffe("hades")}"',
            f'BY="{HERMES}"',
        ]
    )
    presented = parse_xfcc(header)
    assert presented.certificate == LEAF.certificate
    assert presented.uri == spiffe("hades")
    assert presented.by == HERMES


def test_fields_that_hermes_does_not_read_are_ignored() -> None:
    header = xfcc(LEAF.certificate, [CA.intermediate]) + ';Subject="";DNS=hades.local'
    assert parse_xfcc(header).uri == spiffe("hades")


def without(field: str) -> str:
    parts = xfcc(LEAF.certificate, [CA.intermediate]).split(";")
    return ";".join(part for part in parts if not part.startswith(f"{field}="))


@pytest.mark.parametrize("field", ["Hash", "Cert", "Chain", "URI"])
def test_a_missing_field_is_an_error(field: str) -> None:
    with pytest.raises(XfccError):
        parse_xfcc(without(field))


@pytest.mark.parametrize(
    "header",
    [
        "",
        xfcc(LEAF.certificate, [CA.intermediate]) + "," + xfcc(LEAF.certificate, [CA.intermediate]),
        xfcc(LEAF.certificate, [CA.intermediate]) + f";URI={spiffe('otro')}",
        xfcc(LEAF.certificate, [CA.intermediate]) + f";Hash={certificate_hash(LEAF.certificate)}",
        xfcc(LEAF.certificate, [CA.intermediate]) + f";By={HERMES}",
        ";".join(
            part if not part.startswith("Cert=") else 'Cert="not%20a%20certificate"'
            for part in xfcc(LEAF.certificate, [CA.intermediate]).split(";")
        ),
        xfcc(LEAF.certificate, [CA.intermediate]).replace("Chain=", "Chain=x", 1),
        xfcc(LEAF.certificate, [CA.intermediate]).replace("Hash=", "Hash=00", 1),
        xfcc(LEAF.certificate, [CA.intermediate]) + ';Subject="not closed',
        "Hash",
        ";;;",
    ],
)
def test_a_header_that_does_not_follow_the_format_is_an_error(header: str) -> None:
    with pytest.raises(XfccError):
        parse_xfcc(header)


def test_the_hash_must_be_the_one_of_the_certificate() -> None:
    other = CA.issue("hades")
    header = xfcc(LEAF.certificate, [CA.intermediate]).replace(
        certificate_hash(LEAF.certificate), certificate_hash(other.certificate)
    )
    with pytest.raises(XfccError, match="Hash"):
        parse_xfcc(header)


def test_the_error_never_carries_the_certificate() -> None:
    header = xfcc(LEAF.certificate, [CA.intermediate]).replace("Hash=", "Hash=00", 1)
    with pytest.raises(XfccError) as raised:
        parse_xfcc(header)
    assert "BEGIN" not in str(raised.value)
    assert "MII" not in str(raised.value)


@given(st.text(max_size=400))
def test_an_arbitrary_header_never_breaks_the_reader(header: str) -> None:
    with contextlib.suppress(XfccError):
        parse_xfcc(header)


@given(st.text(alphabet=st.sampled_from(list('=;,"\\%abcHashCertChainURI')), max_size=200))
def test_a_header_made_of_the_separators_never_breaks_the_reader(header: str) -> None:
    with contextlib.suppress(XfccError):
        parse_xfcc(header)
