"""The reference client of the tests (T104, quickstart.md section 7): a component of Cerbero that enrolls in Hécate with
its token, signs its short token (with `cerbero_audit` if it is given) and calls Hermes through Envoy with its
certificate.

    python hermes/tests/support/caller.py enroll --hecate https://localhost:8443 --state /tmp/caller
    python hermes/tests/support/caller.py send --hermes https://localhost:9443 --state /tmp/caller \
        --recipient contacto_demo --template device_unknown --data '{"mac": "AA:BB:CC:DD:EE:FF", ...}' --key qs-1
    python hermes/tests/support/caller.py bench --hermes https://localhost:9443 --state /tmp/caller --count 20

Its identity uses the same pieces as Hermes, because the flow of a component is the one of the integration guide of
Hécate (docs/hecate/integracion.md). It checks that Envoy presents the certificate of Hermes.
"""

from __future__ import annotations

import argparse
import asyncio
import getpass
import json
import math
import sys
import time
import uuid
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

import httpx
import jwt

from hermes.application.identity import IdentityService
from hermes.application.ports import StoredIdentity
from hermes.domain.shared.references import SpiffeId
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.clock import SystemClock
from hermes.infrastructure.hecate.client import HecateClient
from hermes.infrastructure.hecate.identity_store import FileIdentityStore
from hermes.infrastructure.hecate.keys import generate_keys, load_call_key
from hermes.infrastructure.hecate.tls import HecateTransport, hecate_check, hecate_ssl_context

TOKEN_LIFETIME_S = 60
SC_001_S = 5.0
SAMPLE_DATA = {"mac": "AA:BB:CC:DD:EE:FF", "ip": "192.168.1.57", "case_id": "case-1234"}
SEND_PATH = "/chat/v1/send_message"


def identity_store(state: Path) -> FileIdentityStore:
    return FileIdentityStore(state_dir=state / "state", proxy_dir=state / "proxy")


async def enroll(hecate: str, state: Path, token: str) -> SpiffeId:
    """The identity of the caller, from its enrollment token."""
    store = identity_store(state)
    clock = SystemClock()
    service: IdentityService | None = None

    def current() -> StoredIdentity | None:
        return service.identity if service is not None else None

    client = HecateClient(
        hecate,
        identity=current,
        files=store.current_files,
        revocations=lambda: service.revocation_lists() if service is not None else (),
        clock=clock,
    )
    service = IdentityService(store=store, hecate=client, clock=clock, new_keys=generate_keys)
    service.load()
    try:
        issued = await service.enroll(SecretValue(token))
    finally:
        await client.aclose()
    return issued.spiffe_id


def call_token(
    identity: StoredIdentity,
    audience: SpiffeId,
    *,
    audit: Mapping[str, str] | None = None,
    issued_at: int | None = None,
    lifetime_s: int = TOKEN_LIFETIME_S,
    issuer: str | None = None,
) -> str:
    """The short token of one call, signed with the key of calls of the caller and its `kid`."""
    now = int(time.time()) if issued_at is None else issued_at
    claims: dict[str, Any] = {
        "iss": issuer if issuer is not None else str(identity.certificate.spiffe_id),
        "aud": str(audience),
        "iat": now,
        "exp": now + lifetime_s,
        "jti": uuid.uuid4().hex,
    }
    if audit is not None:
        claims["cerbero_audit"] = dict(audit)
    headers = {"kid": identity.certificate.call_key_id, "typ": "JWT"}
    return jwt.encode(claims, load_call_key(identity.call_key_pem), algorithm="EdDSA", headers=headers)


class Caller:
    """Calls Hermes through Envoy with the certificate and a new short token for each call.

    The tests of the invalid credentials can call without a certificate, or with another one.
    """

    def __init__(
        self,
        hermes: str,
        state: Path,
        *,
        timeout_s: float = 30.0,
        present_certificate: bool = True,
        certificate: tuple[Path, Path] | None = None,
    ) -> None:
        store = identity_store(state)
        identity = store.load()
        files = store.current_files()
        if identity is None or files is None:
            raise SystemExit("error: the caller is not enrolled: run enroll first")
        chain, key, root = files
        if certificate is not None:
            chain, key = certificate
        self.identity = identity
        self.audience = SpiffeId.parse(f"spiffe://{identity.certificate.spiffe_id.trust_domain}/hermes")
        context = hecate_ssl_context(
            root.read_text(encoding="utf-8"),
            certificate_chain=chain if present_certificate else None,
            key=key if present_certificate else None,
        )
        transport = HecateTransport(context, hecate_check(self.audience, lambda: ()))
        self.client = httpx.AsyncClient(transport=transport, base_url=hermes, timeout=timeout_s)

    async def aclose(self) -> None:
        await self.client.aclose()

    async def send(
        self,
        *,
        recipient: str,
        template: str,
        data: Mapping[str, Any],
        key: str,
        version: int | None = None,
        instance: str | None = None,
        audit: Mapping[str, str] | None = None,
        token: str | None = None,
    ) -> httpx.Response:
        body: dict[str, Any] = {"recipients": [recipient], "template": {"id": template}, "data": dict(data)}
        if version is not None:
            body["template"]["version"] = version
        if instance is not None:
            body["instance"] = instance
        bearer = token if token is not None else call_token(self.identity, self.audience, audit=audit)
        headers = {"Idempotency-Key": key}
        if bearer:
            headers["Authorization"] = f"Bearer {bearer}"
        return await self.client.post(SEND_PATH, json=body, headers=headers)


@dataclass(frozen=True)
class Bench:
    sent: int
    completed: int
    p95_s: float
    under_limit: float

    def line(self) -> str:
        meets = "yes" if self.under_limit >= 0.95 else "no"
        return (
            f"sent {self.sent}, completed {self.completed}, p95 {self.p95_s:.2f} s, 95% under {SC_001_S:.0f} s: {meets}"
        )


async def bench(caller: Caller, count: int, *, recipient: str, template: str, data: Mapping[str, Any]) -> Bench:
    """`count` sends one after another, each with its own key (SC-001: 95 % in less than 5 s)."""
    latencies: list[float] = []
    completed = 0
    for _ in range(count):
        started = time.monotonic()
        response = await caller.send(recipient=recipient, template=template, data=data, key=f"bench-{uuid.uuid4().hex}")
        latencies.append(time.monotonic() - started)
        completed += response.json().get("status") == "completed"
    ordered = sorted(latencies)
    p95 = ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)] if ordered else 0.0
    under = sum(1 for latency in latencies if latency < SC_001_S) / count if count else 1.0
    return Bench(sent=count, completed=completed, p95_s=p95, under_limit=under)


def _token(stdin: TextIO) -> str:
    if stdin.isatty():
        return getpass.getpass("enrollment token: ").strip()
    return stdin.readline().strip()


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="caller", description="The reference client of Hermes for the tests")
    orders = parser.add_subparsers(dest="order", required=True)
    enrolling = orders.add_parser("enroll", help="Enroll in Hécate with the token read from the standard input")
    enrolling.add_argument("--hecate", required=True)
    enrolling.add_argument("--state", required=True, type=Path)
    sending = orders.add_parser("send", help="Send one message and write the envelope of the answer")
    sending.add_argument("--hermes", required=True)
    sending.add_argument("--state", required=True, type=Path)
    sending.add_argument("--recipient", required=True)
    sending.add_argument("--template", required=True)
    sending.add_argument("--version", type=int)
    sending.add_argument("--instance")
    sending.add_argument("--data", required=True, help="The data of the template, as a JSON object")
    sending.add_argument("--key", required=True, help="The Idempotency-Key")
    sending.add_argument("--audit", help="The cerbero_audit claim, as a JSON object of text values")
    benching = orders.add_parser("bench", help="Send several messages and say how long they took")
    benching.add_argument("--hermes", required=True)
    benching.add_argument("--state", required=True, type=Path)
    benching.add_argument("--count", type=int, default=20)
    benching.add_argument("--recipient", default="contacto_demo")
    benching.add_argument("--template", default="device_unknown")
    benching.add_argument("--data", default=json.dumps(SAMPLE_DATA))
    return parser


async def _run(args: argparse.Namespace, stdin: TextIO, out: TextIO) -> int:
    if args.order == "enroll":
        spiffe_id = await enroll(args.hecate, args.state, _token(stdin))
        out.write(f"enrolled {spiffe_id}\n")
        return 0
    caller = Caller(args.hermes, args.state)
    try:
        data = json.loads(args.data)
        if args.order == "send":
            audit = json.loads(args.audit) if args.audit else None
            response = await caller.send(
                recipient=args.recipient,
                template=args.template,
                data=data,
                key=args.key,
                version=args.version,
                instance=args.instance,
                audit=audit,
            )
            envelope = response.json()
            out.write(json.dumps(envelope, ensure_ascii=False, indent=2) + "\n")
            return 0 if envelope.get("status") == "completed" else 1
        result = await bench(caller, args.count, recipient=args.recipient, template=args.template, data=data)
        out.write(result.line() + "\n")
        return 0 if result.completed == result.sent and result.under_limit >= 0.95 else 1
    finally:
        await caller.aclose()


def main(argv: Sequence[str] | None = None, *, stdin: TextIO | None = None, out: TextIO | None = None) -> int:
    args = _parser().parse_args(argv)
    return asyncio.run(_run(args, stdin or sys.stdin, out or sys.stdout))


if __name__ == "__main__":
    raise SystemExit(main())
