"""The test helpers do what the suites expect of them (T006)."""

from datetime import timedelta

import jwt
import pytest
from cryptography import x509
from cryptography.x509.verification import PolicyBuilder, Store, VerificationError

from support.clock import FakeClock
from support.pki import TestCa, spiffe, thumbprint


def test_the_fake_clock_only_moves_forward_by_hand() -> None:
    clock = FakeClock()
    start = clock.now()
    clock.advance(300)
    clock.advance(timedelta(minutes=1))
    assert clock.now() - start == timedelta(seconds=360)
    with pytest.raises(ValueError, match="does not go back"):
        clock.advance(-1)


def test_a_leaf_of_the_test_ca_verifies_as_a_client_with_its_spiffe_id() -> None:
    ca = TestCa()
    leaf = ca.issue("hades")
    verifier = PolicyBuilder().store(Store([ca.root])).time(ca.now).build_client_verifier()
    verified = verifier.verify(leaf.certificate, [ca.intermediate])
    assert verified.subjects == [x509.UniformResourceIdentifier(spiffe("hades"))]


def test_a_leaf_of_another_ca_does_not_verify() -> None:
    ca, other = TestCa(), TestCa()
    verifier = PolicyBuilder().store(Store([ca.root])).time(ca.now).build_client_verifier()
    with pytest.raises(VerificationError):
        verifier.verify(other.issue("hades").certificate, [other.intermediate])


def test_the_crl_is_signed_by_the_intermediate_and_lists_the_revoked_serial() -> None:
    ca = TestCa()
    leaf = ca.issue("hades")
    crl = ca.crl([leaf.serial])
    assert crl.is_signature_valid(ca.intermediate.public_key())
    assert crl.get_revoked_certificate_by_serial_number(leaf.serial) is not None
    assert crl.next_update_utc - crl.last_update_utc == timedelta(seconds=300)


def test_a_short_token_verifies_with_its_published_key() -> None:
    ca = TestCa()
    key = ca.call_key("hades")
    token = key.token(issuer=spiffe("hades"), audience=spiffe("hermes"), issued_at=ca.now)
    assert jwt.get_unverified_header(token)["kid"] == key.kid
    claims = jwt.decode(
        token,
        jwt.PyJWK(key.jwk(), algorithm="EdDSA"),
        algorithms=["EdDSA"],
        audience=spiffe("hermes"),
        options={"verify_exp": False, "verify_iat": False},
    )
    assert claims["iss"] == spiffe("hades")
    assert claims["exp"] - claims["iat"] == 300


def test_the_kid_of_a_call_key_is_its_rfc_7638_thumbprint() -> None:
    assert thumbprint("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo") == "kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k"
    key = TestCa().call_key("hades")
    assert key.kid == thumbprint(key.jwk()["x"])
    assert key.identity == spiffe("hades")
