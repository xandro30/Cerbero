"""The logs (T010, T132): one JSON line per event, with the `request_id` of the request it belongs to, and nothing
secret or personal in them."""

import asyncio
import io
import json
import logging
from collections.abc import Iterator
from datetime import datetime

import pytest

from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.logging import (
    JsonFormatter,
    configure_logging,
    outside_requests,
    request_logged_as,
    stamp_request_ids,
)

TOKEN = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzcGlmZmUifQ.c2lnbmF0dXJlLWJ5dGVz"
BOT_URL = "https://api.telegram.org/bot123456:ABCdefGHI/sendMessage"
PEM = "-----BEGIN CERTIFICATE-----\nMIIBszCCAVmgAwIBAgIUQmVhcg\n-----END CERTIFICATE-----\n"


@pytest.fixture
def stream() -> Iterator[io.StringIO]:
    output = io.StringIO()
    handler = logging.StreamHandler(output)
    handler.setFormatter(JsonFormatter())
    logger = logging.getLogger("hermes.test")
    logger.addHandler(handler)
    logger.setLevel(logging.DEBUG)
    logger.propagate = False
    yield output
    logger.removeHandler(handler)


def lines(stream: io.StringIO) -> list[dict[str, object]]:
    return [json.loads(line) for line in stream.getvalue().splitlines()]


def test_one_json_line_per_event_with_ts_level_logger_and_event(stream: io.StringIO) -> None:
    logger = logging.getLogger("hermes.test")
    logger.info("request.completed", extra={"fields": {"request_id": "req-1", "status": 200}})
    logger.warning("instance.degraded")
    first, second = lines(stream)
    assert first["level"] == "info"
    assert first["logger"] == "hermes.test"
    assert first["event"] == "request.completed"
    assert first["request_id"] == "req-1"
    assert first["status"] == 200
    assert datetime.fromisoformat(str(first["ts"])).tzinfo is not None
    assert second["level"] == "warning"


def test_nothing_secret_or_personal_leaves(stream: io.StringIO) -> None:
    logger = logging.getLogger("hermes.test")
    logger.info(
        "provider.failed at %s",
        BOT_URL,
        extra={
            "fields": {
                "secret": SecretValue("s3cr3t-value"),
                "value": SecretValue("another-s3cr3t"),
                "authorization": f"Bearer {TOKEN}",
                "code": "ABCD-EFGH",
                "address": "987654321",
                "chat_id": 987654321,
                "certificate": PEM,
                "detail": f"failed calling {BOT_URL} with {TOKEN}",
                "nested": {"token": "abc-token", "items": [PEM, SecretValue("x-s3cr3t")]},
            }
        },
    )
    output = stream.getvalue()
    for leaked in ("s3cr3t", TOKEN, "123456:ABCdefGHI", "ABCD-EFGH", "987654321", "MIIBszCC", "abc-token"):
        assert leaked not in output, leaked
    (line,) = lines(stream)
    assert line["secret"] == "***"
    assert line["code"] == "***"
    assert "api.telegram.org/bot***" in str(line["event"])


def test_an_exception_shows_its_type_but_not_its_message(stream: io.StringIO) -> None:
    logger = logging.getLogger("hermes.test")
    try:
        raise ValueError(f"cannot call {BOT_URL} with token {TOKEN}")
    except ValueError:
        logger.exception("request.failed")
    output = stream.getvalue()
    assert TOKEN not in output
    assert "123456:ABCdefGHI" not in output
    (line,) = lines(stream)
    assert line["exception"] == "ValueError"


def test_a_line_inside_a_request_carries_its_request_id(stream: io.StringIO) -> None:
    stamp_request_ids()
    logger = logging.getLogger("hermes.test")
    with request_logged_as("req-inside"):
        logger.info("auth.refused", extra={"fields": {"reason": "signature"}})
    logger.info("polling.failed")
    inside, outside = lines(stream)
    assert inside["request_id"] == "req-inside"
    assert inside["reason"] == "signature"
    assert "request_id" not in outside


def test_a_request_id_field_wins_over_the_one_of_the_request(stream: io.StringIO) -> None:
    stamp_request_ids()
    with request_logged_as("req-bound"):
        logging.getLogger("hermes.test").info("request.failed", extra={"fields": {"request_id": "req-given"}})
    (line,) = lines(stream)
    assert line["request_id"] == "req-given"


def test_the_request_id_of_a_line_is_masked_like_any_field(stream: io.StringIO) -> None:
    stamp_request_ids()
    with request_logged_as(f"req-{TOKEN}"):
        logging.getLogger("hermes.test").info("auth.refused")
    assert TOKEN not in stream.getvalue()
    (line,) = lines(stream)
    assert line["request_id"] == "req-***"


def test_the_records_are_stamped_once_however_many_times_it_is_installed() -> None:
    stamp_request_ids()
    factory = logging.getLogRecordFactory()
    stamp_request_ids()
    assert logging.getLogRecordFactory() is factory


@pytest.mark.anyio
async def test_a_task_that_outlives_its_request_does_not_carry_its_request_id(stream: io.StringIO) -> None:
    stamp_request_ids()
    logger = logging.getLogger("hermes.test")

    async def poll() -> None:
        logger.info("polling.failed")

    with request_logged_as("req-apply"):
        await asyncio.create_task(poll(), context=outside_requests())
        await asyncio.create_task(poll())
    outside, inherited = lines(stream)
    assert "request_id" not in outside
    assert inherited["request_id"] == "req-apply"


def test_httpx_and_httpcore_stay_at_warning() -> None:
    output = io.StringIO()
    configure_logging("debug", stream=output)
    assert logging.getLogger("httpx").level == logging.WARNING
    assert logging.getLogger("httpcore").level == logging.WARNING
    assert logging.getLogger("hermes").getEffectiveLevel() == logging.DEBUG
    logging.getLogger("hermes.anything").debug("started")
    assert json.loads(output.getvalue().splitlines()[-1])["event"] == "started"
    configure_logging("warning", stream=output)
    assert logging.getLogger("hermes").getEffectiveLevel() == logging.WARNING
