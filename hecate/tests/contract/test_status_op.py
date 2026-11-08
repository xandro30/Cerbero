import json
from datetime import timedelta

import pytest

from hecate.infrastructure.control.server import RequestHandler
from tests.fakes import GOOD_PASSPHRASE, FakeClock, initialize, make_service

STATUS_FIELDS = {
    "state", "initialized_at", "unsealed_via", "recommend_passphrase_change",
    "consecutive_failures", "retry_after_s",
}


def call(handler, op, args=None):
    return handler.handle(json.dumps({"request_id": "r", "op": op, "args": args or {}}).encode())


def test_status_when_uninitialized():
    reply = call(RequestHandler(make_service()), "status")
    assert reply["status"] == "completed" and reply["error"] is None
    assert set(reply["data"]) == STATUS_FIELDS
    assert reply["data"] | {} == {
        "state": "uninitialized", "initialized_at": None, "unsealed_via": None,
        "recommend_passphrase_change": False, "consecutive_failures": 0, "retry_after_s": 0,
    }


def test_status_when_sealed_unsealed_and_after_recovery():
    service = make_service()
    shares = initialize(service)
    handler = RequestHandler(service)

    sealed = call(handler, "status")["data"]
    assert set(sealed) == STATUS_FIELDS
    assert sealed["state"] == "sealed" and sealed["initialized_at"] and sealed["unsealed_via"] is None

    call(handler, "unseal", {"passphrase": GOOD_PASSPHRASE})
    unsealed = call(handler, "status")["data"]
    assert (unsealed["state"], unsealed["unsealed_via"], unsealed["recommend_passphrase_change"]) == (
        "unsealed", "passphrase", False)

    call(handler, "seal")
    call(handler, "unseal", {"shares": shares[:2]})
    recovered = call(handler, "status")["data"]
    assert (recovered["unsealed_via"], recovered["recommend_passphrase_change"]) == ("recovery", True)


def test_status_reports_failures_and_the_pending_wait():
    clock = FakeClock()
    service = make_service(clock=clock)
    initialize(service)
    handler = RequestHandler(service)
    for _ in range(3):
        call(handler, "unseal", {"passphrase": "wrong wrong wrong"})
    data = call(handler, "status")["data"]
    assert (data["consecutive_failures"], data["retry_after_s"]) == (3, 60)
    clock.advance(timedelta(seconds=30))
    assert call(handler, "status")["data"]["retry_after_s"] == 30


@pytest.mark.parametrize("op", ["status", "seal"])
def test_internal_error_hides_the_cause_and_is_retryable(op):
    service = make_service()
    initialize(service)

    def boom(*_a, **_k):
        raise RuntimeError("secret detail: /var/lib/hecate/hecate.db is locked")

    service.status = boom
    service.seal = boom
    reply = call(RequestHandler(service), op)

    assert reply["status"] == "failed"
    assert reply["error"] == {"code": "internal", "message": "internal error", "retryable": True}
    assert reply["data"] == {}
    assert "secret detail" not in json.dumps(reply)
    assert reply["meta"]["request_id"] == "r"


# -- expiry counts (FR-022) ----------------------------------------------------------------------------


def _secrets_env(unsealed: bool = True):
    from datetime import UTC, datetime

    from tests.fakes import START, SecretsRig

    rig = SecretsRig(unsealed=unsealed)
    handler = RequestHandler(rig.sealing, rig.service)
    assert START == datetime(2026, 11, 7, 12, 0, tzinfo=UTC)
    return rig, handler


def _create_expiring(rig, name: str, delta: timedelta | None):
    expires = (rig.clock.now() + delta).isoformat() if delta is not None else None
    rig.service.create(f"secret://hermes/{name}", f"value-of-{name}", "string", expires_at=expires)


def test_status_counts_expired_and_expiring_soon_secrets_without_paths():
    rig, handler = _secrets_env()
    _create_expiring(rig, "expired", -timedelta(days=1))
    _create_expiring(rig, "soon", timedelta(days=3))
    _create_expiring(rig, "later", timedelta(days=30))
    _create_expiring(rig, "never", None)
    reply = call(handler, "status")
    assert reply["status"] == "completed"
    assert set(reply["data"]) == STATUS_FIELDS | {"secrets_expired", "secrets_expiring_soon"}
    assert (reply["data"]["secrets_expired"], reply["data"]["secrets_expiring_soon"]) == (1, 1)
    assert "secret://" not in json.dumps(reply) and "expired" not in json.dumps(reply["data"]).replace(
        "secrets_expired", "")


def test_the_limits_are_inclusive_on_the_side_of_the_nearest_date():
    rig, handler = _secrets_env()
    _create_expiring(rig, "now", timedelta(0))
    _create_expiring(rig, "exactly-seven-days", timedelta(days=7))
    _create_expiring(rig, "just-beyond", timedelta(days=7, seconds=1))
    data = call(handler, "status")["data"]
    assert (data["secrets_expired"], data["secrets_expiring_soon"]) == (1, 1)


def test_the_counts_move_with_the_clock():
    rig, handler = _secrets_env()
    _create_expiring(rig, "soon", timedelta(days=3))
    assert call(handler, "status")["data"]["secrets_expiring_soon"] == 1
    rig.clock.advance(timedelta(days=4))
    data = call(handler, "status")["data"]
    assert (data["secrets_expired"], data["secrets_expiring_soon"]) == (1, 0)


def test_the_counts_are_available_while_sealed():
    rig, handler = _secrets_env()
    _create_expiring(rig, "expired", -timedelta(days=1))
    _create_expiring(rig, "soon", timedelta(days=2))
    rig.sealing.seal()
    reply = call(handler, "status")
    assert reply["status"] == "completed" and reply["data"]["state"] == "sealed"
    assert (reply["data"]["secrets_expired"], reply["data"]["secrets_expiring_soon"]) == (1, 1)


def test_expiry_counts_can_be_asked_for_directly_and_leave_no_row_in_the_access_log():
    rig, _ = _secrets_env()
    _create_expiring(rig, "expired", -timedelta(days=1))
    rows = len(rig.access_log.items)
    assert rig.service.expiry_counts(rig.clock.now()) == {"secrets_expired": 1, "secrets_expiring_soon": 0}
    assert len(rig.access_log.items) == rows


def test_a_secret_with_every_version_destroyed_is_no_longer_counted():
    rig, handler = _secrets_env()
    _create_expiring(rig, "expired", -timedelta(days=1))
    path = "secret://hermes/expired"
    rig.service.revoke(path, 1, 2)
    rig.service.destroy(path, 1, 3, {"passphrase": GOOD_PASSPHRASE})
    assert call(handler, "status")["data"]["secrets_expired"] == 0


def test_an_expired_secret_still_resolves():
    rig, _ = _secrets_env()
    _create_expiring(rig, "expired", -timedelta(days=1))
    assert rig.service.resolve("hermes", ["secret://hermes/expired"])["secrets"][0]["value"] == "value-of-expired"


def test_a_handler_without_secrets_keeps_the_original_status_fields():
    reply = call(RequestHandler(make_service()), "status")
    assert set(reply["data"]) == STATUS_FIELDS
