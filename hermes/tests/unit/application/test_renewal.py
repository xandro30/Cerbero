"""The renewal of the certificate (T056, FR-006, SC-005), with a double of Hécate and the clock moved by hand."""

from __future__ import annotations

from datetime import timedelta

import pytest

from hermes.application.identity import RETRY_S, IdentityService
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from support.clock import FakeClock
from support.doubles import MemoryIdentityStore
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = pytest.mark.anyio

TWO_THIRDS = timedelta(hours=48)


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.store = MemoryIdentityStore()
        self.hecate = FakeHecate(self.ca, self.clock)
        self.service = IdentityService(store=self.store, hecate=self.hecate, clock=self.clock, new_keys=generate_keys)
        self.hecate.caller_serial = self.serial

    def serial(self) -> str | None:
        identity = self.service.identity
        return identity.certificate.serial if identity is not None else None

    async def enrolled(self) -> World:
        await self.service.enroll(SecretValue(self.ca.enrollment_token()))
        return self


async def test_it_waits_until_two_thirds_of_the_life_of_the_certificate() -> None:
    world = await World().enrolled()
    assert await world.service.renewal_tick() == pytest.approx(TWO_THIRDS.total_seconds())
    world.clock.advance(TWO_THIRDS - timedelta(seconds=1))
    await world.service.renewal_tick()
    assert world.hecate.renewals == []


async def test_at_two_thirds_it_renews_once_with_new_keys() -> None:
    world = await World().enrolled()
    first = world.service.identity
    assert first is not None
    world.clock.advance(TWO_THIRDS)
    await world.service.renewal_tick()
    renewed = world.service.identity
    assert renewed is not None
    assert len(world.hecate.renewals) == 1
    assert renewed.certificate.serial != first.certificate.serial
    assert renewed.tls_key_pem != first.tls_key_pem
    assert renewed.call_key_pem != first.call_key_pem
    assert world.store.saved[-1] == renewed
    assert world.store.pending is None
    await world.service.renewal_tick()
    assert len(world.hecate.renewals) == 1


async def test_without_the_answer_it_repeats_the_same_request_and_gets_the_same_certificate() -> None:
    world = await World().enrolled()
    first = world.serial()
    world.clock.advance(TWO_THIRDS)
    world.hecate.drop_next = 1
    assert await world.service.renewal_tick() == RETRY_S
    assert world.serial() == first
    pending = world.store.pending
    assert pending is not None
    world.clock.advance(timedelta(seconds=RETRY_S))
    await world.service.renewal_tick()
    assert world.hecate.second_attempts == 0
    assert world.hecate.renewals == [(pending.idempotency_key, pending.csr_pem)]
    identity = world.service.identity
    assert identity is not None
    assert identity.call_key_pem == pending.call_key_pem
    assert identity.tls_key_pem == pending.tls_key_pem
    assert world.store.pending is None


async def test_with_hecate_sealed_or_down_it_keeps_its_certificate_and_retries_every_60_seconds() -> None:
    world = await World().enrolled()
    first = world.serial()
    world.clock.advance(TWO_THIRDS)
    world.hecate.down = True
    assert await world.service.renewal_tick() == RETRY_S
    world.hecate.down = False
    world.hecate.sealed = True
    world.clock.advance(timedelta(seconds=RETRY_S))
    assert await world.service.renewal_tick() == RETRY_S
    assert world.serial() == first
    assert world.service.problem() is None
    pending = world.store.pending
    assert pending is not None
    world.hecate.sealed = False
    world.clock.advance(timedelta(seconds=RETRY_S))
    await world.service.renewal_tick()
    assert world.hecate.renewals == [(pending.idempotency_key, pending.csr_pem)]


async def test_a_pending_renewal_survives_a_restart_and_is_repeated() -> None:
    world = await World().enrolled()
    world.clock.advance(TWO_THIRDS)
    world.hecate.drop_next = 1
    await world.service.renewal_tick()
    pending = world.store.pending
    assert pending is not None
    restarted = IdentityService(store=world.store, hecate=world.hecate, clock=world.clock, new_keys=generate_keys)
    restarted.load()
    world.service = restarted
    await restarted.renewal_tick()
    assert world.hecate.second_attempts == 0
    assert world.hecate.renewals == [(pending.idempotency_key, pending.csr_pem)]


async def test_a_refused_renewal_leaves_hermes_needing_a_new_token() -> None:
    world = await World().enrolled()
    world.clock.advance(TWO_THIRDS)
    world.hecate.fail_next.append(HermesError(ErrorCode.AUTH, "the certificate is not valid"))
    await world.service.renewal_tick()
    assert world.service.problem() == "needs a new enrollment token"


async def test_fifteen_renewals_in_thirty_days_without_a_second_attempt() -> None:
    world = await World().enrolled()
    start = world.clock.now()
    turn = 0
    while len(world.hecate.renewals) < 15 and world.clock.now() - start < timedelta(days=31):
        turn += 1
        if turn % 3 == 0:
            world.hecate.drop_next = 1
        delay = await world.service.renewal_tick()
        assert world.service.problem() is None
        world.clock.advance(timedelta(seconds=min(delay, 3600)))
    assert len(world.hecate.renewals) == 15
    assert world.clock.now() - start <= timedelta(days=30, hours=1)
    assert world.hecate.second_attempts == 0


async def test_the_double_applies_the_rule_of_hecate_on_a_second_attempt() -> None:
    world = await World().enrolled()
    first = generate_keys()
    await world.hecate.renew(first.csr_pem, first.call_public_pem, "renew-1")
    other = generate_keys()
    with pytest.raises(HermesError) as raised:
        await world.hecate.renew(other.csr_pem, other.call_public_pem, "renew-2")
    assert raised.value.code is ErrorCode.AUTH
    assert world.hecate.second_attempts == 1
