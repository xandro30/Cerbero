from datetime import UTC, datetime, timedelta

import pytest
from hypothesis import given
from hypothesis import strategies as st

from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from hecate.domain.secrets.path import SecretPath
from hecate.domain.secrets.secret import (
    Secret,
    VersionAdded,
    VersionDeprecated,
    VersionDestroyed,
    VersionPromoted,
    VersionRestored,
    VersionState,
)
from hecate.domain.secrets.value_type import ValueType

T0 = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)
GRACE = timedelta(hours=24)
PATH = SecretPath.parse("secret://hermes/token")


def minutes(n: int) -> datetime:
    return T0 + timedelta(minutes=n)


class Harness:
    """Builds a secret by applying the events its decision methods return, like the service does."""

    def __init__(self, **create_kwargs) -> None:
        events = Secret.create(PATH, ValueType.STRING, T0, "aaaa-aaaa-aaaa-0001", 1, **create_kwargs)
        self.history = list(events)
        self.secret = Secret.from_events(self.history)

    def do(self, events) -> list:
        self.history.extend(events)
        for event in events:
            self.secret.apply(event)
        return events

    def add(self, at: datetime, key_version: int = 1) -> list:
        n = max(self.secret.versions) + 1
        return self.do(self.secret.add_version(at, f"aaaa-aaaa-aaaa-{n:04d}", key_version))

    def state(self, n: int, now: datetime = T0) -> VersionState:
        return self.secret.state_of(n, now, GRACE)


def test_create_gives_version_one_active_and_the_metadata():
    expires = T0 + timedelta(days=30)
    h = Harness(max_versions=5, rotate_every="90d", expires_at=expires)
    s = h.secret
    assert (s.path, s.owner, s.value_type, s.max_versions) == (PATH, "hermes", ValueType.STRING, 5)
    assert (s.rotate_every, s.expires_at) == ("90d", expires)
    assert s.stream_version == 2 and s.active_version() == 1
    assert h.state(1) is VersionState.ACTIVE
    assert s.versions[1].fingerprint == "aaaa-aaaa-aaaa-0001" and s.versions[1].key_version == 1
    assert s.versions[1].created_at == T0


def test_defaults_to_ten_versions_without_rotation_or_expiry():
    s = Harness().secret
    assert (s.max_versions, s.rotate_every, s.expires_at) == (10, None, None)


def test_rebuilds_the_same_state_from_its_events():
    h = Harness()
    h.add(minutes(1))
    h.do(h.secret.revoke(1, minutes(2), GRACE))
    h.add(minutes(3))
    rebuilt = Secret.from_events(h.history)
    assert rebuilt.stream_version == h.secret.stream_version == len(h.history)
    assert rebuilt.versions == h.secret.versions
    assert rebuilt.active_version() == 3


def test_from_events_needs_a_creation_event_first():
    h = Harness()
    with pytest.raises(ValueError):
        Secret.from_events([])
    with pytest.raises(ValueError):
        Secret.from_events(h.history[1:])


def test_adding_a_version_deprecates_the_previous_active_one():
    h = Harness()
    events = h.add(minutes(10))
    assert [type(e) for e in events] == [VersionDeprecated, VersionAdded]
    assert h.secret.active_version() == 2
    assert h.state(1) is VersionState.DEPRECATED and h.state(2) is VersionState.ACTIVE
    assert h.secret.versions[1].deprecated_at == minutes(10)


def test_adding_a_version_with_nothing_active_deprecates_nothing():
    h = Harness()
    h.do(h.secret.revoke(1, minutes(1), GRACE))
    events = h.add(minutes(2))
    assert [type(e) for e in events] == [VersionAdded]
    assert h.state(1) is VersionState.REVOKED and h.state(2) is VersionState.ACTIVE


def test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change():
    h = Harness(max_versions=2)
    h.add(minutes(1))
    events = h.add(minutes(2))
    assert isinstance(events[-1], VersionDestroyed)
    assert (events[-1].version, events[-1].reason) == (1, "max_versions")
    assert h.state(1) is VersionState.DESTROYED
    assert [h.state(n) for n in (2, 3)] == [VersionState.DEPRECATED, VersionState.ACTIVE]
    events = h.add(minutes(3))
    assert events[-1].version == 2
    assert sum(v.state is not VersionState.DESTROYED for v in h.secret.versions.values()) == 2


@pytest.mark.parametrize("max_versions", [1, 0, -1, 101, True, "2", None])
def test_a_maximum_outside_2_to_100_is_rejected(max_versions):
    with pytest.raises(InvalidInput):
        Secret.create(PATH, ValueType.STRING, T0, "aaaa-aaaa-aaaa-0001", 1, max_versions=max_versions)


def test_with_the_minimum_of_two_the_previous_version_survives_a_rotation_in_its_grace():
    h = Harness(max_versions=2)
    events = h.add(minutes(1))
    assert not any(isinstance(e, VersionDestroyed) for e in events)
    assert [h.state(1, minutes(2)), h.state(2, minutes(2))] == [VersionState.DEPRECATED, VersionState.ACTIVE]


def test_a_maximum_of_two_destroys_the_oldest_with_a_third_version():
    h = Harness(max_versions=2)
    h.add(minutes(1))
    h.add(minutes(2))
    assert [h.state(n, minutes(3)) for n in (1, 2, 3)] == [
        VersionState.DESTROYED, VersionState.DEPRECATED, VersionState.ACTIVE]


ALLOWED = {
    # (from, action) -> to ; everything else is refused
    ("active", "revoke"): "revoked",
    ("deprecated", "revoke"): "revoked",
    ("deprecated", "promote"): "active",
    ("revoked", "restore"): "deprecated",
    ("revoked", "destroy"): "destroyed",
}


def _put_version_in_state(state: str) -> Harness:
    """Version 1 ends up in `state`; version 2 stays active when needed."""
    h = Harness()
    if state == "active":
        return h
    h.add(minutes(1))
    if state == "deprecated":
        return h
    h.do(h.secret.revoke(1, minutes(2), GRACE))
    if state == "revoked":
        return h
    h.do(h.secret.destroy(1, minutes(3), GRACE))
    return h


@pytest.mark.parametrize("state", ["active", "deprecated", "revoked", "destroyed"])
@pytest.mark.parametrize("action", ["revoke", "restore", "promote", "destroy"])
def test_only_the_documented_transitions_are_allowed(state, action):
    h = _put_version_in_state(state)
    now = minutes(5)
    assert h.state(1, now).value == state
    method = getattr(h.secret, action)
    if (state, action) in ALLOWED:
        h.do(method(1, now, GRACE))
        assert h.state(1, now).value == ALLOWED[(state, action)]
    else:
        before = h.secret.stream_version
        with pytest.raises(StateConflict):
            method(1, now, GRACE)
        assert h.secret.stream_version == before


def test_unknown_versions_are_not_found():
    h = Harness()
    for action in ("revoke", "restore", "promote", "destroy"):
        with pytest.raises(NotFound):
            getattr(h.secret, action)(9, T0, GRACE)
    with pytest.raises(NotFound):
        h.secret.state_of(9, T0, GRACE)


def test_promote_swaps_which_version_is_active():
    h = Harness()
    h.add(minutes(1))
    events = h.do(h.secret.promote(1, minutes(2), GRACE))
    assert [type(e) for e in events] == [VersionDeprecated, VersionPromoted]
    assert h.secret.active_version() == 1
    assert h.state(2, minutes(2)) is VersionState.DEPRECATED
    assert sum(h.state(n, minutes(2)) is VersionState.ACTIVE for n in h.secret.versions) == 1


def test_promote_when_nothing_is_active_only_promotes():
    h = Harness()
    h.add(minutes(1))
    h.do(h.secret.revoke(2, minutes(2), GRACE))
    events = h.do(h.secret.promote(1, minutes(3), GRACE))
    assert [type(e) for e in events] == [VersionPromoted]
    assert h.secret.active_version() == 1


def test_revoking_the_active_version_leaves_no_active_one():
    h = Harness()
    h.do(h.secret.revoke(1, minutes(1), GRACE))
    assert h.secret.active_version() is None


def test_restore_makes_it_deprecated_with_a_new_grace():
    h = Harness()
    h.add(minutes(1))
    h.do(h.secret.revoke(1, minutes(2), GRACE))
    events = h.do(h.secret.restore(1, minutes(30), GRACE))
    assert isinstance(events[0], VersionRestored)
    assert h.secret.versions[1].deprecated_at == minutes(30)
    assert h.state(1, minutes(30) + GRACE - timedelta(seconds=1)) is VersionState.DEPRECATED
    assert h.state(1, minutes(30) + GRACE) is VersionState.REVOKED


def test_a_version_past_its_grace_can_be_restored_but_not_revoked_or_promoted():
    h = Harness()
    h.add(minutes(1))
    late = minutes(1) + GRACE + timedelta(hours=1)
    assert h.state(1, late) is VersionState.REVOKED
    for action in ("revoke", "promote"):
        with pytest.raises(StateConflict):
            getattr(h.secret, action)(1, late, GRACE)
    h.do(h.secret.restore(1, late, GRACE))
    assert h.state(1, late) is VersionState.DEPRECATED


def test_a_version_past_its_grace_can_be_destroyed():
    h = Harness()
    h.add(minutes(1))
    late = minutes(1) + GRACE
    h.do(h.secret.destroy(1, late, GRACE))
    assert h.state(1, late) is VersionState.DESTROYED


def test_only_one_version_is_ever_active():
    h = Harness()
    for i in range(1, 6):
        h.add(minutes(i))
        if i % 2:
            h.do(h.secret.promote(1, minutes(i), GRACE))
        active = [n for n in h.secret.versions if h.state(n, minutes(i)) is VersionState.ACTIVE]
        assert len(active) <= 1


@given(st.integers(min_value=-7200, max_value=7200))
def test_effective_state_flips_exactly_at_the_end_of_the_grace(offset_seconds):
    h = Harness()
    h.add(T0)
    now = T0 + GRACE + timedelta(seconds=offset_seconds)
    expected = VersionState.REVOKED if offset_seconds >= 0 else VersionState.DEPRECATED
    assert h.state(1, now) is expected
    assert h.state(2, now) is VersionState.ACTIVE  # the active one never expires


@given(st.integers(min_value=1, max_value=72), st.integers(min_value=1, max_value=72))
def test_the_grace_is_the_one_received(grace_hours, elapsed_hours):
    h = Harness()
    h.add(T0)
    state = h.secret.state_of(1, T0 + timedelta(hours=elapsed_hours), timedelta(hours=grace_hours))
    assert (state is VersionState.REVOKED) == (elapsed_hours >= grace_hours)


def test_stream_version_grows_with_every_event():
    h = Harness()
    assert h.secret.stream_version == 2
    h.add(minutes(1))  # deprecate + add
    assert h.secret.stream_version == 4
    h.do(h.secret.revoke(1, minutes(2), GRACE))
    assert h.secret.stream_version == 5


def test_events_never_carry_a_value_or_a_key():
    h = Harness()
    h.add(minutes(1))
    for event in h.history:
        assert not any(k in vars(event) for k in ("value", "key", "secret", "material"))
