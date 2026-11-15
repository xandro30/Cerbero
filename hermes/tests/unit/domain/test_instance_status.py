"""The state of an instance (T098, data-model.md#estado-de-una-instancia, FR-013).

`unavailable` if its last test failed, if the provider rejected its credentials (until a test works) or if its last
three calls failed; `degraded` if any of its last ten failed; `healthy` otherwise.
"""

import pytest

from hermes.domain.instances.status import InstanceRecord, InstanceStatus


def with_calls(*calls: bool) -> InstanceRecord:
    record = InstanceRecord()
    for worked in calls:
        record = record.with_call(worked)
    return record


def test_an_instance_without_calls_or_tests_is_healthy() -> None:
    assert InstanceRecord().status is InstanceStatus.HEALTHY


def test_calls_that_work_keep_it_healthy() -> None:
    assert with_calls(True, True, True).status is InstanceStatus.HEALTHY


@pytest.mark.parametrize("calls", [(False,), (True, False), (False, True), (False, False, True)])
def test_any_failure_among_the_last_ten_calls_degrades_it(calls: tuple[bool, ...]) -> None:
    assert with_calls(*calls).status is InstanceStatus.DEGRADED


def test_a_failure_older_than_the_last_ten_calls_no_longer_counts() -> None:
    assert with_calls(False, *([True] * 9)).status is InstanceStatus.DEGRADED
    assert with_calls(False, *([True] * 10)).status is InstanceStatus.HEALTHY


def test_three_failed_calls_in_a_row_make_it_unavailable() -> None:
    assert with_calls(True, False, False).status is InstanceStatus.DEGRADED
    assert with_calls(True, False, False, False).status is InstanceStatus.UNAVAILABLE
    assert with_calls(True, False, False, False, True).status is InstanceStatus.DEGRADED


def test_a_failed_test_makes_it_unavailable_until_a_test_works() -> None:
    record = InstanceRecord().with_test(worked=False)
    assert record.status is InstanceStatus.UNAVAILABLE
    assert record.with_call(True).status is InstanceStatus.UNAVAILABLE
    assert record.with_test(worked=True).status is InstanceStatus.HEALTHY


def test_rejected_credentials_make_it_unavailable_until_a_test_works() -> None:
    record = with_calls(True).with_call(False, credentials_rejected=True)
    assert record.status is InstanceStatus.UNAVAILABLE
    assert record.with_call(True).with_call(True).status is InstanceStatus.UNAVAILABLE
    assert record.with_test(worked=False).status is InstanceStatus.UNAVAILABLE
    healed = record.with_test(worked=True)
    assert healed.status is InstanceStatus.DEGRADED
    assert healed.credentials_rejected is False


def test_a_test_that_rejects_the_credentials_marks_them_too() -> None:
    record = InstanceRecord().with_test(worked=False, credentials_rejected=True)
    assert record.credentials_rejected is True
    assert record.with_test(worked=True).status is InstanceStatus.HEALTHY


def test_a_test_that_works_does_not_erase_the_failed_calls() -> None:
    record = with_calls(False, False, False).with_test(worked=True)
    assert record.status is InstanceStatus.UNAVAILABLE
    assert record.with_call(True).status is InstanceStatus.DEGRADED
