"""The flow of Hermes from end to end (T105, quickstart.md sections 2 to 8), in containers: PostgreSQL, a Hécate of
tests, Hermes with the fake provider and Envoy, from the compose of hermes/deploy with its file for the tests.

It prepares Hécate, enrolls Hermes, applies the configuration, verifies the channel with a message left in the inbox of
the fake, sends with the reference client through Envoy, tries the invalid credentials of story 2 that the stack can
produce (SC-002; an expired certificate is in the tests of the API, because Hécate only issues valid ones) and, with
PostgreSQL stopped, checks that Hermes is not ready and that the sends are unavailable.
"""

from __future__ import annotations

import asyncio
import json
import re
import subprocess
import time
import uuid
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import httpx
import jwt
import pytest
from cryptography.hazmat.primitives.asymmetric import ed25519

from hermes.application.authentication import AUTH_MESSAGE
from hermes.domain.shared.references import SpiffeId
from support.caller import SAMPLE_DATA, Caller, call_token, enroll, identity_store
from support.compose import DEPLOY, Compose, ComposeHecate, compose_command, wait_until
from support.config_files import FAKE_ONLY_YAML
from support.contract import contract
from support.pki import TestCa
from support.xfcc import pem

pytestmark = [pytest.mark.anyio, pytest.mark.docker, pytest.mark.hecate]

HECATE_URL = "https://127.0.0.1:18443"
HERMES_URL = "https://127.0.0.1:19443"
FAKE = "/home/hermes/fake/test_fake"


@dataclass
class Stack:
    compose: Compose
    state: Path
    second: Path
    workdir: Path
    before_enrollment: str
    enrolled: str
    applied: str


def health(compose: Compose) -> tuple[int, str]:
    done = compose.run("exec", "-T", "hermes", "hermes", "health", check=False, timeout=60)
    return done.returncode, done.stdout


def refusals(compose: Compose) -> list[str]:
    """The reasons that Hermes logged for each call it refused, to say why a test failed."""
    reasons = []
    for line in compose.logs("hermes").splitlines():
        start = line.find("{")
        entry = json.loads(line[start:]) if start >= 0 and line.rstrip().endswith("}") else {}
        if entry.get("event") == "auth.refused":
            reasons.append(str(entry.get("reason")))
    return reasons


def outbox(compose: Compose) -> list[dict[str, Any]]:
    text = compose.exec("hermes", "cat", f"{FAKE}/outbox.jsonl", check=False)
    return [json.loads(line) for line in text.splitlines() if line.strip()]


@pytest.fixture(scope="module")
def stack(tmp_path_factory: pytest.TempPathFactory) -> Iterator[Stack]:
    command = compose_command()
    if not command:
        pytest.skip("docker-compose is needed for the E2E")
    compose = Compose(command)
    compose.down()
    subprocess.run([str(DEPLOY / "prepare.sh")], check=True, capture_output=True)  # noqa: S603
    try:
        compose.run("up", "-d", "--build", "postgres", "hecate")
        hecate = ComposeHecate(compose)
        hecate.prepare()
        password = (DEPLOY / ".secrets" / "postgres_hermes_password").read_text(encoding="utf-8").strip()
        hecate.secret("secret://hermes/postgres/password", password)
        hermes_token = hecate.token("hermes")
        caller_token = hecate.token("caller")
        compose.run("up", "-d", "--build", "hermes", "envoy")
        _, before = health(compose)
        enrolled = compose.exec("hermes", "hermes", "enroll", stdin=f"{hermes_token}\n")
        wait_until(lambda: health(compose)[0] == 0, "hermes ready", timeout=120)
        applied = compose.exec("hermes", "hermes", "config", "apply", "/dev/stdin", "--yes", stdin=FAKE_ONLY_YAML)
        out = compose.exec("hermes", "hermes", "contacts", "code", "contacto_fake", "--instance", "test_fake")
        found = re.match(r"code (\S+) for contacto_fake on test_fake expires ", out)
        assert found, out
        message = json.dumps({"chat": "chat-e2e", "chat_type": "private", "text": found.group(1)})
        inbox = f"{FAKE}/inbox"
        compose.exec(
            "hermes",
            "sh",
            "-c",
            f"mkdir -p {inbox} && cat > {inbox}/001.tmp && mv {inbox}/001.tmp {inbox}/001.json",
            stdin=message,
        )
        wait_until(lambda: any(line["text"] == "Canal verificado." for line in outbox(compose)), "the verification")
        state = tmp_path_factory.mktemp("caller")
        asyncio.run(enroll(HECATE_URL, state, caller_token))
        second = tmp_path_factory.mktemp("caller-two")
        asyncio.run(enroll(HECATE_URL, second, hecate.token("caller-two")))
        yield Stack(compose, state, second, tmp_path_factory.mktemp("e2e"), before, enrolled, applied)
    except BaseException:
        print(compose.logs("hermes", "envoy", "hecate")[-12000:])
        raise
    finally:
        compose.down()


async def send(stack: Stack, key: str, **options: Any) -> httpx.Response:
    """A send through Envoy; its answer, whatever it is, follows the contract (SC-009)."""
    client_options = {name: options.pop(name) for name in ("present_certificate", "certificate") if name in options}
    caller = Caller(HERMES_URL, options.pop("state", stack.state), **client_options)
    try:
        data = options.pop("data", SAMPLE_DATA)
        response = await caller.send(
            recipient="contacto_fake", template="device_unknown", data=data, key=key, **options
        )
    finally:
        await caller.aclose()
    checked(response)
    return response


def checked(response: httpx.Response) -> dict[str, Any]:
    body: dict[str, Any] = response.json()
    contract().assert_valid("sendMessage", response.status_code, body)
    return body


def test_hermes_waits_for_its_enrollment_and_then_says_who_it_is(stack: Stack) -> None:
    assert stack.before_enrollment == "not ready: needs a new enrollment token\n"
    assert re.fullmatch(r"enrolled spiffe://cerbero\.test/hermes serial [0-9a-f]+ until \S+\n", stack.enrolled)
    assert stack.applied.endswith("applied 5 changes\n")


def test_hermes_publishes_no_port_and_envoy_is_the_only_way_in(stack: Stack) -> None:
    listed = stack.compose.run("ps", "--format", "json", "hermes").stdout.strip()
    entries = json.loads(listed) if listed.startswith("[") else [json.loads(line) for line in listed.splitlines()]
    assert [entry.get("Publishers") or [] for entry in entries] == [[]]


async def test_a_component_sends_through_envoy_and_the_repetition_does_not_send_twice(stack: Stack) -> None:
    before = len(outbox(stack.compose))
    first = await send(stack, "e2e-send", audit={"source": "e2e"})
    assert first.status_code == 200, refusals(stack.compose)
    body = checked(first)
    assert body["status"] == "completed"
    assert body["meta"]["provider"] == "fake"
    sent = outbox(stack.compose)[before:]
    assert [line["chat"] for line in sent] == ["chat-e2e"]
    assert sent[0]["text"].startswith("Dispositivo desconocido en la red")
    assert sent[0]["provider_message_ref"] == body["data"]["provider_message_ref"]
    again = await send(stack, "e2e-send", audit={"source": "e2e"})
    assert again.json()["data"] == body["data"]
    assert len(outbox(stack.compose)) == before + 1
    other = await send(stack, "e2e-send", data={**SAMPLE_DATA, "case_id": "case-9999"})
    assert other.status_code == 409


async def test_the_invalid_credentials_of_story_2_answer_auth_through_envoy(stack: Stack) -> None:
    caller = Caller(HERMES_URL, stack.state)
    identity, hermes = caller.identity, caller.audience
    await caller.aclose()
    other_ca = TestCa()
    stranger = other_ca.issue("caller")
    chain = stack.workdir / "stranger.crt"
    key = stack.workdir / "stranger.key"
    chain.write_text(pem(stranger.certificate) + other_ca.intermediate_pem, encoding="utf-8")
    key.write_text(stranger.key_pem, encoding="utf-8")
    now = int(time.time())
    forged = jwt.encode(
        {"iss": str(identity.certificate.spiffe_id), "aud": str(hermes), "iat": now, "exp": now + 60, "jti": "forged"},
        ed25519.Ed25519PrivateKey.generate(),
        algorithm="EdDSA",
        headers={"kid": identity.certificate.call_key_id, "typ": "JWT"},
    )
    cases: dict[str, dict[str, Any]] = {
        "without a certificate": {"present_certificate": False},
        "with a certificate of another CA": {"certificate": (chain, key)},
        "without a token": {"token": ""},
        "an expired token": {"token": call_token(identity, hermes, issued_at=1_700_000_000)},
        "a token that lives more than 300 s": {"token": call_token(identity, hermes, lifetime_s=301)},
        "a token for another audience": {"token": call_token(identity, SpiffeId.parse("spiffe://cerbero.test/hecate"))},
        "a token of another identity": {"token": call_token(identity, hermes, issuer="spiffe://cerbero.test/otro")},
        "a token signed with a key that Hécate does not publish": {"token": forged},
    }
    before = len(outbox(stack.compose))
    for name, options in cases.items():
        response = await send(stack, f"e2e-auth-{uuid.uuid4().hex}", **options)
        assert response.status_code == 401, name
        assert checked(response)["error"]["message"] == AUTH_MESSAGE, name
    reused = call_token(identity, hermes)
    assert (await send(stack, f"e2e-reuse-{uuid.uuid4().hex}", token=reused)).status_code == 200
    assert (await send(stack, f"e2e-reuse-{uuid.uuid4().hex}", token=reused)).status_code == 401
    assert len(outbox(stack.compose)) == before + 1


async def capabilities_status(state: Path) -> int:
    caller = Caller(HERMES_URL, state)
    try:
        token = call_token(caller.identity, caller.audience)
        response = await caller.client.get("/v1/capabilities", headers={"Authorization": f"Bearer {token}"})
    finally:
        await caller.aclose()
    contract().assert_valid("getCapabilities", response.status_code, response.json())
    return response.status_code


async def test_an_urgently_revoked_certificate_answers_auth_once_hermes_refreshes_its_list(stack: Stack) -> None:
    """The second component calls first, so its key is in the cache of Hermes and the revocation list is what refuses
    it, once Hermes refreshes it (every 240 s)."""
    assert (await send(stack, f"e2e-before-revoking-{uuid.uuid4().hex}", state=stack.second)).status_code == 200
    stored = identity_store(stack.second).load()
    assert stored is not None
    revoked = stack.compose.exec("hecate", "hecate", "certificates", "revoke", stored.certificate.serial, "--urgent")
    assert revoked.startswith(f"revoked {stored.certificate.serial}")
    deadline = time.monotonic() + 300
    while await capabilities_status(stack.second) == 200:
        assert time.monotonic() < deadline, "hermes did not refresh its revocation list in 300 s"
        await asyncio.sleep(5)
    assert refusals(stack.compose)[-1] == "revoked"
    before = len(outbox(stack.compose))
    response = await send(stack, f"e2e-revoked-{uuid.uuid4().hex}", state=stack.second)
    assert response.status_code == 401
    assert checked(response)["error"]["message"] == AUTH_MESSAGE
    assert len(outbox(stack.compose)) == before


async def test_without_postgresql_hermes_is_not_ready_and_the_sends_are_unavailable(stack: Stack) -> None:
    stack.compose.run("stop", "postgres")
    wait_until(lambda: health(stack.compose) == (1, "not ready: database unavailable\n"), "hermes not ready")
    response = await send(stack, f"e2e-down-{uuid.uuid4().hex}")
    assert response.status_code == 503
    assert checked(response)["error"]["code"] == "unavailable"
