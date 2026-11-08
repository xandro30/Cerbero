import pytest
from hypothesis import given
from hypothesis import strategies as st

from hecate.domain.sealing.errors import InvalidInput
from hecate.domain.secrets.path import SecretPath, SecretReference

SEGMENT = st.text(alphabet="abcdefghijklmnopqrstuvwxyz0123456789_-", min_size=1, max_size=64)
IDENTITY = st.from_regex(r"[a-z][a-z0-9-]{0,62}", fullmatch=True)


def test_parses_a_valid_path_and_exposes_its_owner():
    path = SecretPath.parse("secret://hermes/telegram/family_bot/bot_token")
    assert path.owner == "hermes"
    assert path.segments == ("telegram", "family_bot", "bot_token")
    assert str(path) == "secret://hermes/telegram/family_bot/bot_token"
    assert path.stream_name == "secret/hermes/telegram/family_bot/bot_token"


@pytest.mark.parametrize(
    "text",
    [
        "",
        "hermes/token",
        "secret:/hermes/token",
        "secret://",
        "secret://hermes",
        "secret://hermes/",
        "secret://hermes//token",
        "secret://hermes/token/",
        "secret://Hermes/token",
        "secret://1hermes/token",
        "secret://-hermes/token",
        "secret://her_mes/token",
        "secret://hermes/Token",
        "secret://hermes/to ken",
        "secret://hermes/to.ken",
        "secret://hermes/tok@en",
        "secret://hermes/../etc",
        "secret://" + "a" * 64 + "/token",
        "secret://hermes/" + "a" * 65,
        "secret://hermes/token\n",
    ],
)
def test_rejects_malformed_paths(text):
    with pytest.raises(InvalidInput):
        SecretPath.parse(text)


def test_rejects_non_text():
    with pytest.raises(InvalidInput):
        SecretPath.parse(None)  # type: ignore[arg-type]


@given(IDENTITY, st.lists(SEGMENT, min_size=1, max_size=5))
def test_every_well_formed_path_round_trips(identity, segments):
    text = f"secret://{identity}/" + "/".join(segments)
    path = SecretPath.parse(text)
    assert str(path) == text
    assert path.owner == identity


def test_reference_without_version():
    ref = SecretReference.parse("secret://hermes/token")
    assert ref.path == SecretPath.parse("secret://hermes/token")
    assert ref.version is None


def test_reference_with_version():
    ref = SecretReference.parse("secret://hermes/token@12")
    assert ref.path.owner == "hermes" and ref.version == 12


@pytest.mark.parametrize(
    "text",
    ["secret://hermes/token@", "secret://hermes/token@0", "secret://hermes/token@-1",
     "secret://hermes/token@x", "secret://hermes/token@1@2", "secret://hermes/token@01x", "@1"],
)
def test_rejects_malformed_references(text):
    with pytest.raises(InvalidInput):
        SecretReference.parse(text)
