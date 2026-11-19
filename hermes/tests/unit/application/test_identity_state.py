"""The validity of the identity (T059, FR-010): without a valid certificate Hermes is not ready and asks for a token.

Since change 011, a call made with the identity that Hécate refuses with `auth` leaves it refused until a new enrollment
or a restart, and no other call goes out with it (research R2 to R5); the double of Hécate does what Hécate does after
an urgent revocation.
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from datetime import timedelta
from typing import Any

import pytest
from cryptography.hazmat.primitives import serialization

from hermes.application.configuration import HecateSecretChecker
from hermes.application.identity import NEEDS_TOKEN, PUBLICATIONS_INTERVAL_S, RETRY_S, IdentityService
from hermes.application.ports import HecateTrustChanged, TrustMaterial
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from support.clock import FakeClock
from support.doubles import MemoryIdentityStore
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = pytest.mark.anyio

PASSWORD = SecretReference.parse("secret://hermes/postgres/password")
TOKEN_REF = SecretReference.parse("secret://hermes/telegram/family_bot/bot_token")


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.store = MemoryIdentityStore()
        self.hecate = FakeHecate(self.ca, self.clock)
        self.hecate.secrets = {str(PASSWORD): "db-password", str(TOKEN_REF): "123:bot-token"}
        self.service = IdentityService(store=self.store, hecate=self.hecate, clock=self.clock, new_keys=generate_keys)
        self.hecate.caller_serial = lambda: (
            self.service.identity.certificate.serial if self.service.identity is not None else None
        )

    async def enrolled(self) -> World:
        await self.service.enroll(SecretValue(self.ca.enrollment_token()))
        await self.service.refresh_publications()
        return self


def test_without_an_identity_it_needs_a_token() -> None:
    world = World()
    assert world.service.problem() == NEEDS_TOKEN
    assert NEEDS_TOKEN == "needs a new enrollment token"


async def test_a_valid_identity_is_ready() -> None:
    world = await World().enrolled()
    assert world.service.problem() is None


async def test_an_expired_certificate_needs_a_new_token() -> None:
    world = await World().enrolled()
    world.clock.advance(timedelta(days=3))
    assert world.service.problem() == NEEDS_TOKEN


async def test_a_certificate_in_the_crl_needs_a_new_token() -> None:
    world = await World().enrolled()
    identity = world.service.identity
    assert identity is not None
    world.hecate.revoked = [int(identity.certificate.serial, 16)]
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN


async def test_a_renewal_refused_with_auth_needs_a_new_token() -> None:
    world = await World().enrolled()
    world.clock.advance(timedelta(hours=48))
    world.hecate.fail_next.append(HermesError(ErrorCode.AUTH, "the certificate is not valid"))
    await world.service.renewal_tick()
    assert world.service.problem() == NEEDS_TOKEN
    calls = world.hecate.authenticated_calls
    world.clock.advance(timedelta(seconds=RETRY_S))
    assert await world.service.renewal_tick() == RETRY_S
    assert world.hecate.authenticated_calls == calls


async def test_hecate_with_another_root_ca_needs_a_new_token() -> None:
    world = await World().enrolled()
    world.hecate.fail_next.append(HecateTrustChanged())
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN


async def test_an_identity_that_is_not_valid_gives_no_trust_to_check_callers() -> None:
    world = await World().enrolled()
    assert world.service.material() is not None
    world.clock.advance(timedelta(days=3))
    assert world.service.material() is None


async def test_a_valid_identity_cannot_enroll_again() -> None:
    world = await World().enrolled()
    with pytest.raises(HermesError) as raised:
        await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert raised.value.code is ErrorCode.CONFLICT


async def test_an_expired_identity_can_enroll_again() -> None:
    world = await World().enrolled()
    world.clock.advance(timedelta(days=3))
    await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert world.service.problem() is None


async def test_a_token_without_the_format_of_hecate_is_validation() -> None:
    world = World()
    with pytest.raises(HermesError) as raised:
        await world.service.enroll(SecretValue("not-a-token"))
    assert raised.value.code is ErrorCode.VALIDATION
    assert world.hecate.enrollments == []


# --- The double of Hécate after an urgent revocation (change 011, T005) ---------------------------------------------

CALLS = ("renew", "revocations", "keys", "key", "callers", "resolve")


def serial_of(world: World) -> str:
    identity = world.service.identity
    assert identity is not None
    return identity.certificate.serial


async def renewal_with_new_keys(hecate: FakeHecate) -> object:
    keys = generate_keys()
    return await hecate.renew(keys.csr_pem, keys.call_public_pem, "renewal-011")


def call(hecate: FakeHecate, name: str, kid: str = "k" * 43) -> Awaitable[object]:
    """One call of each kind that Hermes makes to Hécate with its identity (research R2)."""
    calls: dict[str, Callable[[], Awaitable[object]]] = {
        "renew": lambda: renewal_with_new_keys(hecate),
        "revocations": hecate.revocations,
        "keys": hecate.keys,
        "key": lambda: hecate.key(kid),
        "callers": hecate.callers,
        "resolve": lambda: hecate.resolve([PASSWORD]),
    }
    return calls[name]()


@pytest.mark.parametrize("name", CALLS)
async def test_the_double_refuses_with_auth_each_call_of_a_certificate_revoked_urgently(name: str) -> None:
    world = await World().enrolled()
    serial = serial_of(world)
    world.hecate.revoke_urgently(serial)
    with pytest.raises(HermesError) as raised:
        await call(world.hecate, name)
    assert raised.value.code is ErrorCode.AUTH
    assert world.hecate.refused == [serial]
    assert int(serial, 16) in world.hecate.revoked


async def test_after_an_urgent_revocation_the_double_still_enrolls_and_answers_another_certificate() -> None:
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    keys = generate_keys()
    issued = await world.hecate.enroll(
        SecretValue(world.ca.enrollment_token()), keys.csr_pem, keys.call_public_pem, "enrollment-011"
    )
    world.hecate.caller_serial = lambda: issued.serial
    for name in CALLS:
        await call(world.hecate, name, kid=issued.call_key_id)


async def test_the_double_counts_each_authenticated_call_also_a_refused_one_and_not_the_enrollments() -> None:
    world = World()
    keys = generate_keys()
    issued = await world.hecate.enroll(
        SecretValue(world.ca.enrollment_token()), keys.csr_pem, keys.call_public_pem, "enrollment-011"
    )
    world.hecate.caller_serial = lambda: issued.serial
    assert world.hecate.authenticated_calls == 0
    await world.hecate.revocations()
    world.hecate.revoke_urgently(issued.serial)
    with pytest.raises(HermesError):
        await world.hecate.keys()
    assert world.hecate.authenticated_calls == 2


async def test_a_held_call_waits_and_is_decided_with_the_serial_it_started_with() -> None:
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    release = asyncio.Event()
    world.hecate.hold_next = release
    held = asyncio.ensure_future(world.hecate.revocations())
    await asyncio.sleep(0)
    assert not held.done()
    assert world.hecate.hold_next is None
    world.hecate.caller_serial = lambda: "b0b0"
    assert (await world.hecate.revocations()).crls_der
    release.set()
    with pytest.raises(HermesError) as raised:
        await held
    assert raised.value.code is ErrorCode.AUTH


async def test_without_urgent_revocations_or_a_hold_the_double_answers_as_before() -> None:
    world = await World().enrolled()
    identity = world.service.identity
    assert identity is not None
    assert (world.hecate.refused, world.hecate.hold_next) == ([], None)
    for name in CALLS:
        await call(world.hecate, name, kid=identity.certificate.call_key_id)


# --- What leaves the identity refused (change 011, T007 to T010) ----------------------------------------------------

REFUSED = "hecate refused the identity of hermes: it needs a new enrollment token"
TWO_THIRDS = timedelta(hours=48)


def logged(caplog: pytest.LogCaptureFixture, event: str) -> list[dict[str, Any]]:
    """The fields of each line `event` of the log."""
    return [getattr(record, "fields", {}) for record in caplog.records if record.msg == event]


def der(material: TrustMaterial) -> list[bytes]:
    return [crl.public_bytes(serialization.Encoding.DER) for crl in material.revocations]


async def refreshing(world: World) -> object:
    await world.service.refresh_publications()
    return None


async def resolving(world: World) -> object:
    return await world.service.resolve([PASSWORD])


async def checking_the_configuration(world: World) -> object:
    return await HecateSecretChecker(world.service).unresolved({"instances[0].secrets.bot_token": TOKEN_REF})


async def asking_a_key(world: World) -> object:
    return await world.service.key("k" * 43)


async def asking_the_access_list(world: World) -> object:
    return await world.service.lookup_access()


async def renewing(world: World) -> object:
    world.clock.advance(TWO_THIRDS)
    return await world.service.renewal_tick()


@dataclass(frozen=True)
class Route:
    """A way from Hermes to Hécate with its identity, the call that Hécate sees first, and how the way fails today when
    Hécate refuses that call: an error, or a line of the log with the reason `auth`."""

    action: Callable[[World], Awaitable[object]]
    operation: str
    raises: ErrorCode | None = None
    logs: str | None = None


ROUTES = {
    "refresh": Route(refreshing, "revocations", logs="publications.refresh_failed"),
    "resolve": Route(resolving, "resolve", raises=ErrorCode.AUTH),
    "configuration": Route(checking_the_configuration, "resolve", raises=ErrorCode.UNAVAILABLE),
    "key": Route(asking_a_key, "key", raises=ErrorCode.UNAVAILABLE),
    "access": Route(asking_the_access_list, "callers", raises=ErrorCode.UNAVAILABLE),
    "renewal": Route(renewing, "renew", logs="identity.renewal_failed"),
}


async def fails_as_before(world: World, route: Route, caplog: pytest.LogCaptureFixture) -> None:
    if route.raises is not None:
        with pytest.raises(HermesError) as raised:
            await route.action(world)
        assert raised.value.code is route.raises
        return
    assert route.logs is not None
    before = len(logged(caplog, route.logs))
    await route.action(world)
    lines = logged(caplog, route.logs)
    assert len(lines) == before + 1
    assert lines[-1]["reason"] == "auth"


@pytest.mark.parametrize("name", sorted(ROUTES))
async def test_the_first_call_that_hecate_refuses_leaves_the_identity_needing_a_new_token(
    name: str, caplog: pytest.LogCaptureFixture
) -> None:
    """FR-001 and FR-002, story 1, scenarios 1 and 6: each way notices it at once, without a restart."""
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    assert world.service.problem() is None
    await fails_as_before(world, ROUTES[name], caplog)
    assert world.service.problem() == NEEDS_TOKEN
    assert world.service.material() is None


@pytest.mark.parametrize("code", [code for code in ErrorCode if code is not ErrorCode.AUTH], ids=str)
async def test_a_failure_that_is_not_auth_leaves_the_identity_valid(code: ErrorCode) -> None:
    """FR-003: only a refusal of Hécate changes the validity of the identity."""
    world = await World().enrolled()
    world.hecate.fail_next.append(HermesError(code, "a failure of hecate"))
    await world.service.refresh_publications()
    world.hecate.fail_next.append(HermesError(code, "a failure of hecate"))
    with pytest.raises(HermesError):
        await world.service.resolve([PASSWORD])
    assert world.service.problem() is None


@pytest.mark.parametrize("trouble", ["sealed", "down"])
async def test_an_hour_of_hecate_sealed_or_down_leaves_the_identity_valid_with_its_last_copy(trouble: str) -> None:
    """SC-004, with a refresh every 240 s and a resolution each time."""
    world = await World().enrolled()
    before = world.service.material()
    assert before is not None
    setattr(world.hecate, trouble, True)
    for _ in range(15):
        world.clock.advance(timedelta(seconds=PUBLICATIONS_INTERVAL_S))
        await world.service.refresh_publications()
        with pytest.raises(HermesError):
            await world.service.resolve([PASSWORD])
        assert world.service.problem() is None
    after = world.service.material()
    assert after is not None
    assert der(after) == der(before)
    assert (after.keys, after.access) == (before.keys, before.access)


async def test_the_urgent_revocation_of_another_certificate_leaves_hermes_ready_and_in_its_list() -> None:
    """Story 1, scenario 5."""
    world = await World().enrolled()
    world.hecate.revoke_urgently("abc123")
    await world.service.refresh_publications()
    assert world.service.problem() is None
    material = world.service.material()
    assert material is not None
    assert material.revocations[0].get_revoked_certificate_by_serial_number(0xABC123) is not None


async def test_hecate_with_another_root_ca_blocks_no_call_and_its_return_makes_the_identity_valid_again() -> None:
    """FR-009: the CA replaced is not a refusal."""
    world = await World().enrolled()
    world.hecate.fail_next.append(HecateTrustChanged())
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN
    calls = world.hecate.authenticated_calls
    await world.service.refresh_publications()
    assert world.hecate.authenticated_calls == calls + 3
    assert world.service.problem() is None


async def test_with_the_identity_refused_no_call_reaches_hecate_and_each_way_fails_as_if_hecate_refused_it(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """FR-008, SC-006, story 1, scenario 7."""
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN
    copy = (world.store.revocations, world.store.keys, world.store.access)
    calls = world.hecate.authenticated_calls
    for name in ("refresh", "resolve", "configuration", "key", "access"):
        await fails_as_before(world, ROUTES[name], caplog)
    with pytest.raises(HermesError) as raised:
        await world.service.resolve([PASSWORD])
    assert (raised.value.code, raised.value.message) == (ErrorCode.AUTH, REFUSED)
    world.clock.advance(TWO_THIRDS)
    assert await world.service.renewal_tick() == RETRY_S
    assert world.hecate.authenticated_calls == calls
    assert (world.store.revocations, world.store.keys, world.store.access) == copy


async def test_the_first_refusal_logs_identity_refused_once_with_the_serial_and_the_call(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """FR-005: a call already in flight gets its refusal too, without another line, and the blocked ones leave none."""
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    serial = serial_of(world)
    world.hecate.revoke_urgently(serial)
    release = asyncio.Event()
    world.hecate.hold_next = release
    in_flight = asyncio.ensure_future(world.service.resolve([PASSWORD]))
    await asyncio.sleep(0)
    await world.service.refresh_publications()
    release.set()
    with pytest.raises(HermesError) as raised:
        await in_flight
    assert raised.value.code is ErrorCode.AUTH
    for name in ("resolve", "key", "access", "refresh"):
        await fails_as_before(world, ROUTES[name], caplog)
    [line] = [record for record in caplog.records if record.msg == "identity.refused"]
    assert line.levelno == logging.WARNING
    assert getattr(line, "fields", None) == {"serial": serial, "operation": "revocations"}
    assert "BEGIN" not in caplog.text


async def test_after_a_restart_the_identity_is_valid_until_hecate_refuses_its_first_call(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """FR-007: the refusal lives only in memory, so the first call of the start checks it again."""
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN
    world.service = IdentityService(store=world.store, hecate=world.hecate, clock=world.clock, new_keys=generate_keys)
    world.service.load()
    assert world.service.problem() is None
    with pytest.raises(HermesError) as raised:
        await world.service.resolve([PASSWORD])
    assert raised.value.code is ErrorCode.AUTH
    assert raised.value.message != REFUSED
    assert world.service.problem() == NEEDS_TOKEN
    assert [fields["operation"] for fields in logged(caplog, "identity.refused")] == ["revocations", "resolve"]


# --- The enrollment after a refusal (change 011, T018 and T019) -----------------------------------------------------


async def test_with_the_identity_refused_a_new_token_enrolls_and_drops_the_renewal_in_flight() -> None:
    """FR-006, story 2, scenarios 1 and 5: nothing in the store is touched by hand."""
    world = await World().enrolled()
    first = serial_of(world)
    world.hecate.revoke_urgently(first)
    world.clock.advance(TWO_THIRDS)
    assert await world.service.renewal_tick() == RETRY_S
    assert world.store.pending is not None
    assert world.service.problem() == NEEDS_TOKEN
    calls = world.hecate.authenticated_calls
    issued = await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert issued.serial != first
    assert world.service.problem() is None
    assert world.store.pending is None
    stored = world.store.load()
    assert stored is not None
    assert stored.certificate.serial == issued.serial
    assert world.hecate.authenticated_calls == calls + 3
    assert world.service.material() is not None
    [resolved] = await world.service.resolve([PASSWORD])
    assert resolved.value == SecretValue("db-password")
    with pytest.raises(HermesError) as raised:
        await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert raised.value.code is ErrorCode.CONFLICT


async def test_an_enrollment_that_hecate_refuses_marks_nothing_and_another_token_enrolls_after_it(
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    await world.service.refresh_publications()
    with pytest.raises(HermesError) as raised:
        await world.service.enroll(SecretValue(TestCa(now=world.clock.now()).enrollment_token()))
    assert raised.value.code is ErrorCode.AUTH
    assert world.service.problem() == NEEDS_TOKEN
    assert len(logged(caplog, "identity.refused")) == 1
    await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    assert world.service.problem() is None


async def test_a_refusal_that_arrives_after_a_new_enrollment_does_not_count_for_the_new_identity(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """FR-004, story 2, scenario 4: the call in flight was made with the identity that the enrollment replaced."""
    caplog.set_level(logging.WARNING, logger="hermes")
    world = await World().enrolled()
    world.hecate.revoke_urgently(serial_of(world))
    release = asyncio.Event()
    world.hecate.hold_next = release
    late = asyncio.ensure_future(world.service.resolve([PASSWORD]))
    await asyncio.sleep(0)
    await world.service.refresh_publications()
    assert world.service.problem() == NEEDS_TOKEN
    await world.service.enroll(SecretValue(world.ca.enrollment_token()))
    release.set()
    with pytest.raises(HermesError) as raised:
        await late
    assert raised.value.code is ErrorCode.AUTH
    assert world.service.problem() is None
    calls = world.hecate.authenticated_calls
    [resolved] = await world.service.resolve([PASSWORD])
    assert resolved.value == SecretValue("db-password")
    assert world.hecate.authenticated_calls == calls + 1
    assert len(logged(caplog, "identity.refused")) == 1
