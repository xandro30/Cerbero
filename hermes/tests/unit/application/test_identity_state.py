"""The validity of the identity (T059, FR-010): without a valid certificate Hermes is not ready and asks for a token."""

from __future__ import annotations

from datetime import timedelta

import pytest

from hermes.application.identity import NEEDS_TOKEN, IdentityService
from hermes.application.ports import HecateTrustChanged
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from support.clock import FakeClock
from support.doubles import MemoryIdentityStore
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = pytest.mark.anyio


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.store = MemoryIdentityStore()
        self.hecate = FakeHecate(self.ca, self.clock)
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
