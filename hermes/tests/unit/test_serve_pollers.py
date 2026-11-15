"""The pollings that `hermes serve` starts after an apply (T132): they outlive the request of the apply, so their lines
do not carry its `request_id`."""

import asyncio
import json
import logging
from pathlib import Path

import pytest

from hermes.domain.shared.references import SecretReference
from hermes.infrastructure.logging import JsonFormatter, request_logged_as, stamp_request_ids
from hermes.infrastructure.main import Hermes
from hermes.infrastructure.settings import ServeSettings
from support.configuration import configuration
from support.doubles import StaticSecrets

pytestmark = pytest.mark.anyio


class LoggingVerification:
    """Each polling leaves one line and waits until it is cancelled."""

    async def poll_forever(self, instance_name: str) -> None:
        logger = logging.getLogger("hermes.application.verification")
        logger.warning("polling.failed", extra={"fields": {"instance": instance_name}})
        await asyncio.Event().wait()


def settings(directory: Path) -> ServeSettings:
    return ServeSettings(
        state_dir=directory / "state",
        proxy_dir=directory / "proxy",
        run_dir=directory / "run",
        admin_dir=directory / "admin",
        hecate_url="https://hecate:8443",
        database_url="postgresql+psycopg://hermes@postgres:5432/hermes",
        database_password_ref=SecretReference.parse("secret://hermes/postgres/password"),
        verification_code_minutes=30,
        fake_providers=True,
        fake_dir=None,
        log_level="info",
    )


async def test_the_pollings_that_an_apply_starts_do_not_carry_its_request_id(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.INFO, logger="hermes")
    stamp_request_ids()
    hermes = Hermes(settings(tmp_path), StaticSecrets({}))
    hermes.polling = True
    hermes.verification = LoggingVerification()  # type: ignore[assignment]
    try:
        with request_logged_as("req-apply"):
            logging.getLogger("hermes.application.configuration").info("config.applied")
            hermes._sync_polling(configuration())
        await asyncio.sleep(0)
        formatter = JsonFormatter()
        lines = [json.loads(formatter.format(record)) for record in caplog.records]
        [applied] = [line for line in lines if line["event"] == "config.applied"]
        assert applied["request_id"] == "req-apply"
        polled = [line for line in lines if line["event"] == "polling.failed"]
        assert {line["instance"] for line in polled} == {instance.name for instance in configuration().instances}
        assert all("request_id" not in line for line in polled)
    finally:
        await hermes.close()
