import pytest

from hecate.domain.sealing.errors import InvalidInput
from hecate.domain.sealing.passphrase import Passphrase


def test_accepts_twelve_or_more_characters():
    assert Passphrase("exactly12chr").value == "exactly12chr"


@pytest.mark.parametrize("value", ["", "short", "elevenchars"])
def test_rejects_fewer_than_twelve_characters(value):
    with pytest.raises(InvalidInput):
        Passphrase(value)


def test_rejects_non_text():
    with pytest.raises(InvalidInput):
        Passphrase(None)  # type: ignore[arg-type]


def test_repr_and_str_do_not_show_the_value():
    p = Passphrase("super secret phrase")
    assert "secret" not in repr(p)
    assert "secret" not in str(p)
    assert "secret" not in f"{p!r} {p}"
