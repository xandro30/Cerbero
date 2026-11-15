"""The one-time code that verifies a channel (T073, research R12)."""

import hashlib
from datetime import UTC, datetime, timedelta

import pytest
from hypothesis import given
from hypothesis import strategies as st

from hermes.domain.verification.code import ALPHABET, VerificationCode, digest_of, expires_at, normalized


def test_a_code_has_8_characters_of_crockford_without_i_l_o_u() -> None:
    assert ALPHABET == "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
    code = VerificationCode.new()
    assert len(code.value) == 8
    assert set(code.value) <= set(ALPHABET)


def test_it_is_shown_as_xxxx_xxxx() -> None:
    assert VerificationCode("ABCD2345").shown == "ABCD-2345"


def test_codes_are_random() -> None:
    assert len({VerificationCode.new().value for _ in range(200)}) == 200


codes = st.text(alphabet=ALPHABET, min_size=8, max_size=8)


@given(code=codes, data=st.data())
def test_it_matches_without_case_spaces_or_hyphens(code: str, data: st.DataObject) -> None:
    variant = ""
    for char in code:
        variant += data.draw(st.sampled_from([char, char.lower()]))
        variant += data.draw(st.sampled_from(["", " ", "-", "  "]))
    assert normalized(variant) == code
    assert digest_of(variant) == VerificationCode(code).digest


@pytest.mark.parametrize("text", ["hola", "ABCD-234", "ABCD-23456", "ABCD-23IO", "ABCD 2L45", "", "ABCD_2345"])
def test_other_texts_are_not_codes(text: str) -> None:
    assert normalized(text) is None
    assert digest_of(text) is None


def test_only_its_sha256_is_kept() -> None:
    code = VerificationCode("ABCD2345")
    assert code.digest == hashlib.sha256(b"ABCD2345").digest()
    assert "ABCD2345" not in repr(code)


def test_it_expires_after_the_configured_minutes() -> None:
    now = datetime(2026, 11, 14, 10, 0, tzinfo=UTC)
    assert expires_at(now, 30) == now + timedelta(minutes=30)
    assert expires_at(now, 5) == now + timedelta(minutes=5)
    assert expires_at(now, 1440) == now + timedelta(days=1)
    for minutes in (4, 1441):
        with pytest.raises(ValueError, match="5 to 1440"):
            expires_at(now, minutes)
