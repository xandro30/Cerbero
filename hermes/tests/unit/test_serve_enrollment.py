"""The password of the database after an enrollment (change 011, research R7, FR-006): while Hécate refused the
identity of Hermes, the cache could drop it, so `hermes serve` asks for it again as soon as each enrollment ends,
without waiting for the refresh of every 240 s."""

import asyncio
import logging
from collections.abc import Callable
from pathlib import Path

import pytest

from hermes.application.identity import IdentityService
from hermes.application.secrets import SecretCache
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from hermes.infrastructure.main import Hermes
from hermes.infrastructure.persistence.engine import create_engine
from hermes.infrastructure.settings import ServeSettings
from support.clock import FakeClock
from support.doubles import MemoryIdentityStore
from support.fake_hecate import FakeHecate
from support.pki import TestCa

pytestmark = pytest.mark.anyio

PASSWORD = SecretReference.parse("secret://hermes/postgres/password")


def settings(directory: Path) -> ServeSettings:
    return ServeSettings(
        state_dir=directory / "state",
        proxy_dir=directory / "proxy",
        run_dir=directory / "run",
        admin_dir=directory / "admin",
        hecate_url="https://hecate:8443",
        database_url="postgresql+psycopg://hermes@postgres:5432/hermes",
        database_password_ref=PASSWORD,
        verification_code_minutes=30,
        fake_providers=True,
        fake_dir=None,
        log_level="info",
    )


class Served:
    """A `Hermes` of `hermes serve` without secrets injected, on the identity service and the cache of a `FakeHecate`,
    with its engine created as after the step of the connection, without connecting to any database (without it,
    `_refresh_password` does nothing)."""

    def __init__(self, directory: Path) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.hecate = FakeHecate(self.ca, self.clock)
        self.hecate.secrets = {str(PASSWORD): "db-password"}
        self.hermes = Hermes(settings(directory), None)
        identity = IdentityService(
            store=MemoryIdentityStore(), hecate=self.hecate, clock=self.clock, new_keys=generate_keys
        )
        self.hermes.identity = identity
        self.hermes.cache = SecretCache(identity, self.clock)
        self.hermes.secrets = self.hermes.cache
        self.hermes.runtime.engine = create_engine(self.hermes.settings.database_url, self.hermes._password)
        self.hecate.caller_serial = self.serial

    def serial(self) -> str | None:
        stored = self.hermes.identity.identity
        return stored.certificate.serial if stored is not None else None

    async def enroll(self) -> None:
        await self.hermes.identity.enroll(SecretValue(self.ca.enrollment_token()))

    async def refuse(self) -> None:
        """Hécate revokes the certificate of Hermes urgently and refuses its next call."""
        serial = self.serial()
        assert serial is not None
        self.hecate.revoke_urgently(serial)
        await self.hermes.identity.refresh_publications()
        assert self.hermes.identity.problem() is not None

    def has_password(self) -> bool:
        try:
            return self.hermes.cache.current(PASSWORD) == SecretValue("db-password")
        except HermesError:
            return False


async def eventually(condition: Callable[[], bool]) -> None:
    for _ in range(100):
        if condition():
            return
        await asyncio.sleep(0)
    raise AssertionError("it did not happen")


async def test_after_an_enrollment_the_cache_has_the_password_again_without_waiting_for_the_refresh(
    tmp_path: Path,
) -> None:
    served = Served(tmp_path)
    hermes = served.hermes
    waiting: asyncio.Task[None] | None = None
    try:
        await served.enroll()
        await hermes.cache.resolve(PASSWORD)
        await served.refuse()
        served.clock.advance(300)
        with pytest.raises(HermesError) as raised:
            await hermes._refresh_password()
        assert raised.value.code is ErrorCode.AUTH
        assert not served.has_password()
        waiting = asyncio.create_task(hermes._password_after_enrollments())
        await asyncio.sleep(0)
        resolutions = served.hecate.resolutions
        await served.enroll()
        await eventually(served.has_password)
        assert served.hecate.resolutions == resolutions + 1
    finally:
        if waiting is not None:
            waiting.cancel()
        await hermes.close()


async def test_without_an_answer_of_hecate_it_logs_the_type_of_the_error_and_waits_for_the_next_enrollment(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.WARNING, logger="hermes")
    served = Served(tmp_path)
    hermes = served.hermes
    waiting: asyncio.Task[None] | None = None
    try:
        waiting = asyncio.create_task(hermes._password_after_enrollments())
        await asyncio.sleep(0)
        await served.enroll()
        served.hecate.down = True
        await eventually(lambda: any(record.msg == "secrets.refresh.failed" for record in caplog.records))
        [line] = [record for record in caplog.records if record.msg == "secrets.refresh.failed"]
        assert getattr(line, "fields", None) == {"reason": "HermesError"}
        assert not waiting.done()
        assert not served.has_password()
        served.hecate.down = False
        await served.refuse()
        await served.enroll()
        await eventually(served.has_password)
    finally:
        if waiting is not None:
            waiting.cancel()
        await hermes.close()
