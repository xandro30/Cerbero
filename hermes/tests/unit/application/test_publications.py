"""The publications of Hécate (T057, FR-008): the CRLs and the call keys, refreshed and kept for the next start."""

from __future__ import annotations

import asyncio

import pytest
from cryptography import x509

from hermes.application.identity import PUBLICATIONS_INTERVAL_S, IdentityService
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
        self.service = self.started()

    def started(self) -> IdentityService:
        service = IdentityService(store=self.store, hecate=self.hecate, clock=self.clock, new_keys=generate_keys)
        service.load()
        return service

    async def enrolled(self) -> World:
        await self.service.enroll(SecretValue(self.ca.enrollment_token()))
        return self


def test_they_are_refreshed_every_240_seconds_because_they_last_300() -> None:
    assert PUBLICATIONS_INTERVAL_S == 240


async def test_a_refresh_keeps_them_in_memory_and_on_disk() -> None:
    world = await World().enrolled()
    world.hecate.revoked = [12345]
    await world.service.refresh_publications()
    material = world.service.material()
    assert material is not None
    [crl] = material.revocations
    assert crl.get_revoked_certificate_by_serial_number(12345) is not None
    assert world.store.revocations is not None
    assert world.store.keys is not None
    assert [key["kid"] for key in material.keys] == [key["kid"] for key in world.store.keys.keys]


async def test_when_hecate_does_not_answer_the_last_copy_stays() -> None:
    world = await World().enrolled()
    world.hecate.revoked = [111]
    await world.service.refresh_publications()
    world.hecate.down = True
    world.hecate.revoked = [222]
    await world.service.refresh_publications()
    material = world.service.material()
    assert material is not None
    assert material.revocations[0].get_revoked_certificate_by_serial_number(111) is not None


async def test_at_start_the_saved_copy_is_used_without_asking_hecate() -> None:
    world = await World().enrolled()
    world.hecate.revoked = [333]
    await world.service.refresh_publications()
    world.hecate.down = True
    restarted = world.started()
    await restarted.ensure_publications()
    material = restarted.material()
    assert material is not None
    assert material.revocations[0].get_revoked_certificate_by_serial_number(333) is not None


async def test_without_a_saved_copy_the_start_asks_hecate() -> None:
    world = await World().enrolled()
    world.store.revocations = None
    world.store.keys = None
    restarted = world.started()
    await restarted.ensure_publications()
    material = restarted.material()
    assert material is not None
    assert isinstance(material.revocations[0], x509.CertificateRevocationList)


async def test_without_a_copy_and_without_hecate_the_start_fails() -> None:
    world = await World().enrolled()
    world.store.revocations = None
    world.store.keys = None
    world.hecate.down = True
    restarted = world.started()
    with pytest.raises(HermesError):
        await restarted.ensure_publications()
    assert restarted.material() is None


async def test_a_key_that_the_list_does_not_have_is_asked_once_and_kept_until_the_next_refresh() -> None:
    world = await World().enrolled()
    renewed = world.ca.call_key("hades")
    world.hecate.publish(renewed.jwk())
    assert await world.service.key(renewed.kid) == renewed.jwk()
    assert await world.service.key(renewed.kid) == renewed.jwk()
    assert world.hecate.key_lookups == [renewed.kid]
    await world.service.refresh_publications()
    await world.service.key(renewed.kid)
    assert world.hecate.key_lookups == [renewed.kid, renewed.kid]


async def test_lookups_of_one_kid_at_once_make_one_request() -> None:
    world = await World().enrolled()
    renewed = world.ca.call_key("hades")
    world.hecate.publish(renewed.jwk())
    found = await asyncio.gather(*(world.service.key(renewed.kid) for _ in range(5)))
    assert found == [renewed.jwk()] * 5
    assert world.hecate.key_lookups == [renewed.kid]


async def test_a_kid_that_hecate_does_not_know_is_none_and_is_asked_again_next_time() -> None:
    world = await World().enrolled()
    assert await world.service.key("k" * 43) is None
    assert await world.service.key("k" * 43) is None
    assert world.hecate.key_lookups == ["k" * 43, "k" * 43]


async def test_hecate_not_answering_about_a_key_is_unavailable() -> None:
    world = await World().enrolled()
    world.hecate.down = True
    with pytest.raises(HermesError) as raised:
        await world.service.key("k" * 43)
    assert raised.value.code is ErrorCode.UNAVAILABLE
