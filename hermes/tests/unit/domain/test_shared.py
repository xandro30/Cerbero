"""The common domain (T008): names, secret references, SPIFFE IDs, error codes and secret values."""

import pickle

import pytest
from hypothesis import assume, given
from hypothesis import strategies as st

from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.names import is_name, require_name
from hermes.domain.shared.references import SecretReference, SpiffeId
from hermes.domain.shared.secret import SecretValue


@pytest.mark.parametrize("value", ["a", "family_bot", "contacto_demo", "a1_2", "x" + "y" * 62])
def test_a_name_follows_the_pattern(value: str) -> None:
    assert is_name(value)
    assert require_name(value, "instance") == value


@pytest.mark.parametrize("value", ["", "A", "1abc", "_a", "a-b", "a b", "á", "x" * 64, "a\n", "family.bot"])
def test_anything_else_is_not_a_name(value: str) -> None:
    assert not is_name(value)
    with pytest.raises(HermesError) as raised:
        require_name(value, "instance")
    assert raised.value.code is ErrorCode.VALIDATION


def test_a_secret_reference_reads_with_the_rules_of_hecate() -> None:
    reference = SecretReference.parse("secret://hermes/telegram/family_bot/bot_token")
    assert reference.identity == "hermes"
    assert reference.path == ("telegram", "family_bot", "bot_token")
    assert reference.version is None
    assert str(reference) == "secret://hermes/telegram/family_bot/bot_token"
    pinned = SecretReference.parse("secret://hermes/postgres/password@3")
    assert pinned.version == 3
    assert str(pinned) == "secret://hermes/postgres/password@3"
    assert SecretReference.parse(str(pinned)) == pinned


@pytest.mark.parametrize(
    "value",
    [
        "123456:ABC-plain-token",
        "hermes/telegram/bot_token",
        "https://hermes/telegram",
        "secret://Hermes/telegram",
        "secret://-hermes/telegram",
        "secret://hermes",
        "secret://hermes/",
        "secret://hermes//bot_token",
        "secret://hermes/bot token",
        "secret://hermes/" + "x" * 65,
        "secret://" + "h" * 64 + "/x",
        "secret://hermes/x@0",
        "secret://hermes/x@01",
        "secret://hermes/x@1234567890",
        "secret://hermes/x@",
    ],
)
def test_anything_else_is_not_a_secret_reference(value: str) -> None:
    with pytest.raises(HermesError) as raised:
        SecretReference.parse(value)
    assert raised.value.code is ErrorCode.VALIDATION
    assert value not in raised.value.message


def test_a_spiffe_id_gives_its_trust_domain_and_identity() -> None:
    spiffe = SpiffeId.parse("spiffe://cerbero.home/hades")
    assert spiffe.trust_domain == "cerbero.home"
    assert spiffe.identity == "hades"
    assert str(spiffe) == "spiffe://cerbero.home/hades"
    assert SpiffeId.parse("spiffe://cerbero_test.home-lab/test-caller").identity == "test-caller"


@pytest.mark.parametrize(
    "value",
    [
        "spiffe://cerbero.home",
        "spiffe://cerbero.home/",
        "spiffe:///hades",
        "spiffe://Cerbero.home/hades",
        "spiffe://cerbero.home/Hades",
        "spiffe://cerbero.home/hades/child",
        "spiffe://cerbero.home/1hades",
        "https://cerbero.home/hades",
        "spiffe://" + "a" * 256 + "/hades",
    ],
)
def test_anything_else_is_not_a_spiffe_id(value: str) -> None:
    with pytest.raises(HermesError) as raised:
        SpiffeId.parse(value)
    assert raised.value.code is ErrorCode.VALIDATION


def test_the_error_codes_are_the_nine_common_with_their_default_retryable() -> None:
    assert [code.value for code in ErrorCode] == [
        "validation",
        "auth",
        "not_found",
        "conflict",
        "rate_limited",
        "provider_unavailable",
        "timeout",
        "unavailable",
        "internal",
    ]
    assert {code.value for code in ErrorCode if code.retryable} == {
        "rate_limited",
        "provider_unavailable",
        "unavailable",
        "internal",
    }


def test_a_hermes_error_takes_the_default_retryable_of_its_code_unless_told() -> None:
    assert HermesError(ErrorCode.INTERNAL, "unexpected").retryable
    assert not HermesError(ErrorCode.CONFLICT, "not verified").retryable
    unknown = HermesError(ErrorCode.TIMEOUT, "no answer", retryable=False, outcome_unknown=True, from_provider=True)
    assert unknown.outcome_unknown is True
    assert unknown.from_provider is True
    limited = HermesError(ErrorCode.RATE_LIMITED, "slow down", retry_after_s=7)
    assert limited.retryable
    assert limited.retry_after_s == 7


def test_a_secret_value_never_shows_itself() -> None:
    secret = SecretValue("123456:ABC-token")
    for text in (repr(secret), str(secret), f"{secret}", f"{secret!r}", "%s" % (secret,), format(secret, ">30")):  # noqa: UP031
        assert "123456" not in text
    assert secret.reveal() == "123456:ABC-token"
    assert secret == SecretValue("123456:ABC-token")
    assert secret != SecretValue("other")
    with pytest.raises(TypeError):
        pickle.dumps(secret)


@given(st.text(min_size=1))
def test_no_secret_value_shows_up_in_its_representation(value: str) -> None:
    shown = repr(SecretValue(value))
    assume(value not in "SecretValue(***)")
    assert value not in shown
