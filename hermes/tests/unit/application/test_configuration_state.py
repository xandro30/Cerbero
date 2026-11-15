"""The state of the configuration (T015): one snapshot replaced at once."""

import pytest

from hermes.application.configuration import ConfigurationState
from hermes.domain.configuration.model import Configuration, Variable
from hermes.domain.shared.errors import ErrorCode, HermesError


def snapshot(value: str) -> Configuration:
    return Configuration(instances=(), templates=(), variables=(Variable("v", value),), contacts=())


def test_without_a_snapshot_the_configuration_is_not_loaded() -> None:
    state = ConfigurationState()
    assert not state.loaded
    with pytest.raises(HermesError) as raised:
        state.current()
    assert raised.value.code is ErrorCode.UNAVAILABLE


def test_the_snapshot_is_replaced_at_once_and_an_old_reader_keeps_its_own() -> None:
    state = ConfigurationState(snapshot("before"))
    taken = state.current()
    state.replace(snapshot("after"))
    assert taken.variables[0].value == "before"
    assert state.current().variables[0].value == "after"
    assert state.loaded
