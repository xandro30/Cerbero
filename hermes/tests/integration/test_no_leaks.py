"""Nothing secret or personal leaks (T106, SC-007, FR-023).

The test goes through the enrollment, applying and exporting the configuration, asking for a code and collecting it,
and sends that work and that fail, through the network API with a real certificate and real short tokens. Then it looks
for the token of the bot, the codes, the addresses, the short tokens, the enrollment token and the certificate in the
logs, the answers of the API, the audit and the export: as text, in base64, URL-encoded and escaped in JSON. And no
table of the database keeps a secret, only its reference.
"""

from __future__ import annotations

import base64
import io
import json
import logging
import urllib.parse
from collections.abc import Iterator
from typing import Any

import httpx
import pytest
import respx
from cryptography.hazmat.primitives import serialization
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.authentication import CallAuthenticator
from hermes.application.configuration import ConfigurationAdmin, ConfigurationState, HecateSecretChecker
from hermes.application.idempotency import Idempotency
from hermes.application.identity import IdentityService
from hermes.application.secrets import SecretCache
from hermes.application.send_message import SendMessage
from hermes.application.verification import Verification
from hermes.domain.configuration.model import Configuration
from hermes.domain.services.chat.provider import IncomingMessage
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.config_file.export import write_configuration
from hermes.infrastructure.config_file.models import read_configuration
from hermes.infrastructure.hecate.keys import generate_keys
from hermes.infrastructure.logging import configure_logging
from hermes.infrastructure.persistence.audit_repository import SqlAuditLog
from hermes.infrastructure.persistence.channels_repository import SqlChannelRepository
from hermes.infrastructure.persistence.config_repository import SqlConfigRepository
from hermes.infrastructure.persistence.idempotency_repository import SqlIdempotencyRepository
from hermes.infrastructure.persistence.schema import metadata
from hermes.infrastructure.persistence.tokens_repository import SqlUsedTokens
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.registry import read_manifest
from hermes.infrastructure.providers.retry import RetryTiming
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider
from support.clock import FakeClock
from support.config_files import VALID_YAML
from support.configuration import BOT_TOKEN, BOT_TOKEN_REF, CHAT, DATA
from support.doubles import MemoryIdentityStore, Providers, StaticTrust
from support.fake_hecate import FakeHecate
from support.pki import TestCa, spiffe
from support.telegram import API, recording
from support.xfcc import HERMES, xfcc

pytestmark = [pytest.mark.anyio, pytest.mark.docker]

MANIFESTS = {"telegram": read_manifest("telegram"), "fake": read_manifest("fake")}
SEND = f"/bot{BOT_TOKEN}/sendMessage"


def encodings(value: str) -> set[str]:
    """The value as text, in base64 (standard and URL-safe, with and without padding), URL-encoded and in JSON."""
    raw = value.encode("utf-8")
    standard = base64.b64encode(raw).decode("ascii")
    urlsafe = base64.urlsafe_b64encode(raw).decode("ascii")
    return {
        value,
        standard,
        standard.rstrip("="),
        urlsafe,
        urlsafe.rstrip("="),
        urllib.parse.quote(value, safe=""),
        urllib.parse.quote_plus(value),
        json.dumps(value)[1:-1],
    }


def leaks(secrets: dict[str, str], places: dict[str, str]) -> list[str]:
    return [
        f"{what} in {where}"
        for where, haystack in places.items()
        for what, value in secrets.items()
        if any(encoded in haystack for encoded in encodings(value))
    ]


@pytest.mark.parametrize("encoded", sorted(encodings("123456789:TEST-only/x+y")))
def test_the_search_finds_a_value_in_each_of_its_encodings(encoded: str) -> None:
    assert leaks({"it": "123456789:TEST-only/x+y"}, {"there": f"before {encoded} after"}) == ["it in there"]


@pytest.fixture
def logs() -> Iterator[io.StringIO]:
    """Every log of the process as Hermes writes it, at the debug level."""
    root = logging.getLogger()
    handlers, level = list(root.handlers), root.level
    hermes_level = logging.getLogger("hermes").level
    buffer = io.StringIO()
    configure_logging("debug", stream=buffer)
    yield buffer
    for handler in list(root.handlers):
        root.removeHandler(handler)
    for handler in handlers:
        root.addHandler(handler)
    root.setLevel(level)
    logging.getLogger("hermes").setLevel(hermes_level)


async def dump(engine: AsyncEngine, table: str) -> str:
    async with engine.connect() as connection:
        rows = await connection.execute(text(f"SELECT row_to_json(t)::text FROM {table} t"))  # noqa: S608
        return "\n".join(row[0] for row in rows)


async def test_no_secret_code_address_token_or_certificate_leaks(migrated: AsyncEngine, logs: io.StringIO) -> None:
    clock = FakeClock()
    ca = TestCa(now=clock.now())
    hecate = FakeHecate(ca, clock)
    hecate.secrets = {BOT_TOKEN_REF: BOT_TOKEN}
    enrollment_token = ca.enrollment_token()
    identity = IdentityService(store=MemoryIdentityStore(), hecate=hecate, clock=clock, new_keys=generate_keys)
    await identity.enroll(SecretValue(enrollment_token))

    state = ConfigurationState(Configuration.empty())
    audit = SqlAuditLog(migrated)
    channels = SqlChannelRepository(migrated)
    admin = ConfigurationAdmin(
        state=state,
        repository=SqlConfigRepository(migrated),
        read=read_configuration,
        write=write_configuration,
        manifests=MANIFESTS,
        fake_enabled=True,
        secrets=HecateSecretChecker(hecate),
        clock=clock,
    )
    plan = await admin.plan(VALID_YAML)
    await admin.apply(VALID_YAML, plan.digest)
    exported = await admin.export()

    leaf = ca.issue("hades")
    key = ca.call_key("hades")
    tokens: list[str] = []
    answers: list[httpx.Response] = []
    async with httpx.AsyncClient() as http:
        with respx.mock(base_url=API, assert_all_called=False) as router:
            route = router.post(SEND).mock(return_value=recording("send_message_ok"))
            timing = RetryTiming(sleep=clock.sleep, monotonic=clock.monotonic, jitter=lambda: 1.0)
            providers = Providers(
                telegram=TelegramProvider(read_manifest("telegram"), http, timing=timing),
                fake=FakeProvider(read_manifest("fake"), clock, sleep=clock.sleep),
            )
            cache = SecretCache(hecate, clock)
            verification = Verification(
                configuration=state,
                channels=channels,
                providers=providers,
                secrets=cache,
                clock=clock,
                code_minutes=30,
            )
            issued = await verification.issue_code("contacto_demo", "family_bot")
            assert await verification.collect("family_bot", IncomingMessage(CHAT, "private", issued.code))

            send = SendMessage(
                configuration=state, channels=channels, providers=providers, secrets=cache, audit=audit, clock=clock
            )
            trust = StaticTrust(ca.root, HERMES, [ca.crl([])], [key.jwk()], callers=["hades"])
            app = create_network_api()
            authenticator = CallAuthenticator(trust=trust, used_tokens=SqlUsedTokens(migrated), clock=clock)
            install_authentication(app, Authentication(authenticator=authenticator, ready=lambda: send.ready))
            idempotency = Idempotency(SqlIdempotencyRepository(migrated), clock)
            install_chat(app, ChatServices(send_message=send, idempotency=idempotency))
            body: dict[str, Any] = {
                "recipients": ["contacto_demo"],
                "template": {"id": "device_unknown", "version": 1},
                "data": DATA,
            }

            def refuse(request: httpx.Request) -> httpx.Response:
                raise httpx.ConnectError(f"refused {request.url}")

            outcomes: list[Any] = [
                recording("send_message_ok"),
                recording("send_message_403_blocked"),
                recording("send_message_502"),
                httpx.ReadTimeout(f"timed out reading {API}{SEND}"),
                refuse,
            ]
            transport = httpx.ASGITransport(app=app)
            async with httpx.AsyncClient(transport=transport, base_url="http://hermes") as client:
                for index, outcome in enumerate(outcomes):
                    route.mock(side_effect=outcome if callable(outcome) else [outcome])
                    token = key.token(issuer=spiffe("hades"), audience=HERMES, issued_at=clock.now())
                    tokens.append(token)
                    headers = {
                        "X-Forwarded-Client-Cert": xfcc(leaf.certificate, [ca.intermediate]),
                        "Authorization": f"Bearer {token}",
                        "Idempotency-Key": f"leaks-{index}",
                    }
                    answers.append(await client.post("/chat/v1/send_message", json=body, headers=headers))
                headers["Idempotency-Key"] = "leaks-reused-token"
                answers.append(await client.post("/chat/v1/send_message", json=body, headers=headers))
    assert [answer.status_code for answer in answers] == [200, 200, 200, 200, 200, 401]

    certificate = base64.b64encode(leaf.certificate.public_bytes(serialization.Encoding.DER)).decode("ascii")
    secrets = {
        "the token of the bot": BOT_TOKEN,
        "the code": issued.code,
        "the code as shown": f"{issued.code[:4]}-{issued.code[4:]}",
        "the enrollment token": enrollment_token,
        "the certificate": certificate[60:120],
        **{f"short token {index}": token for index, token in enumerate(tokens)},
        **{f"the signature of short token {index}": token.rsplit(".", 1)[1] for index, token in enumerate(tokens)},
    }
    captured = logs.getvalue()
    for event in ("identity.enrolled", "config.applied", "channel.verified", "auth.refused"):
        assert f'"event": "{event}"' in captured, event
    places = {
        "the logs": captured,
        "the answers": "\n".join(f"{answer.status_code} {dict(answer.headers)} {answer.text}" for answer in answers),
        "the audit": await dump(migrated, "audit_log"),
        "the export": exported,
    }
    assert leaks({**secrets, "the chat of the person": CHAT}, places) == []
    assert "Canal verificado." in [json.loads(call.request.content)["text"] for call in route.calls]

    tables = {table: await dump(migrated, table) for table in metadata.tables}
    assert leaks(secrets, tables) == []
    instances = await dump(migrated, "config_instances")
    assert BOT_TOKEN_REF in instances
