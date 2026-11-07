import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from hecate.domain.sealing.errors import InvalidInput
from hecate.domain.sealing.share import ALPHABET, PREFIX, RecoveryShare

share_strategy = st.builds(
    RecoveryShare,
    index=st.integers(1, 3),
    share_set_id=st.binary(min_size=4, max_size=4),
    fragments=st.binary(min_size=32, max_size=32),
)


@given(share_strategy)
def test_text_roundtrip(share):
    text = share.encode()
    assert text.startswith(PREFIX + "-")
    assert all(len(group) <= 4 for group in text.split("-"))
    assert RecoveryShare.parse(text) == share


@settings(max_examples=200, deadline=None)
@given(share=share_strategy, data=st.data())
def test_any_changed_character_is_detected(share, data):
    text = share.encode()
    body_positions = [i for i, c in enumerate(text) if i >= len(PREFIX) + 1 and c != "-"]
    pos = data.draw(st.sampled_from(body_positions))
    replacement = data.draw(st.sampled_from([c for c in ALPHABET if c != text[pos]]))
    corrupted = text[:pos] + replacement + text[pos + 1 :]
    with pytest.raises(InvalidInput):
        RecoveryShare.parse(corrupted)


@given(share_strategy)
def test_lowercase_spaces_and_confusable_letters_are_accepted(share):
    text = share.encode()
    typed = text.lower().replace("-", " ").replace("0", "o").replace("1", "l")
    # the leading "hec1" must survive the same typing slips
    assert RecoveryShare.parse(typed) == share


def test_truncated_or_wrong_prefix_is_rejected():
    text = RecoveryShare(1, b"abcd", bytes(32)).encode()
    with pytest.raises(InvalidInput):
        RecoveryShare.parse(text[:-3])
    with pytest.raises(InvalidInput):
        RecoveryShare.parse("XYZ1-" + text[5:])
    with pytest.raises(InvalidInput):
        RecoveryShare.parse("")


def test_repr_does_not_show_fragments():
    share = RecoveryShare(2, b"abcd", b"\x01" * 32)
    assert "01" * 8 not in repr(share)
    assert share.fragments.hex() not in repr(share)
