"""The compose of hermes/deploy for the E2E (research R21): PostgreSQL, a Hécate of tests, Hermes and Envoy, with the
file for the tests on top, driven with `docker-compose` and the standard input of each CLI."""

from __future__ import annotations

import re
import shutil
import subprocess
import time
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path

from support.hecate import CA_PASSPHRASE, PASSPHRASE, TRUST_DOMAIN

DEPLOY = Path(__file__).resolve().parents[2] / "deploy"
PROJECT = "cerbero-hermes-e2e"


def compose_command() -> list[str]:
    for candidate in (["docker-compose"], ["docker", "compose"]):
        if shutil.which(candidate[0]) is None:
            continue
        probe = subprocess.run([*candidate, "version"], capture_output=True, check=False)  # noqa: S603
        if probe.returncode == 0:
            return candidate
    return []


@dataclass
class Compose:
    command: list[str]
    project: str = PROJECT
    files: tuple[Path, ...] = (DEPLOY / "compose.yaml", DEPLOY / "compose.test.yaml")

    def _base(self) -> list[str]:
        flags = [item for path in self.files for item in ("-f", str(path))]
        return [*self.command, *flags, "-p", self.project]

    def run(
        self, *args: str, stdin: str | None = None, check: bool = True, timeout: float = 900
    ) -> subprocess.CompletedProcess[str]:
        done = subprocess.run(  # noqa: S603
            [*self._base(), *args], input=stdin, capture_output=True, text=True, timeout=timeout, check=False
        )
        if check and done.returncode != 0:
            raise AssertionError(f"compose {' '.join(args[:3])} failed with {done.returncode}: {done.stderr[-2000:]}")
        return done

    def exec(self, service: str, *command: str, stdin: str | None = None, check: bool = True) -> str:
        return self.run("exec", "-T", service, *command, stdin=stdin, check=check, timeout=120).stdout

    def logs(self, *services: str) -> str:
        return self.run("logs", "--no-color", *services, check=False).stdout

    def down(self) -> None:
        self.run("down", "-v", "--remove-orphans", check=False)


def wait_until(condition: object, what: str, timeout: float = 60.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():  # type: ignore[operator]
        if time.monotonic() > deadline:
            raise AssertionError(f"{what} did not happen in {timeout} s")
        time.sleep(0.5)


@dataclass
class ComposeHecate:
    """The Hécate of the compose, prepared through its CLI as docs/hecate/operacion.md says."""

    compose: Compose
    root_fingerprint: str = field(default="")

    def cli(self, *args: str, lines: Sequence[str] = ()) -> str:
        return self.compose.exec("hecate", "hecate", *args, stdin="".join(f"{line}\n" for line in lines))

    def _init(self) -> None:
        init = subprocess.Popen(  # noqa: S603
            [*self.compose._base(), "exec", "-T", "hecate", "hecate", "init"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        assert init.stdin is not None
        assert init.stdout is not None
        init.stdin.write(f"yes\n{PASSPHRASE}\n{PASSPHRASE}\n")
        init.stdin.flush()
        shares: list[str] = []
        while len(shares) < 3:
            line = init.stdout.readline()
            if not line:
                break
            found = re.search(r"share \d: (\S+)", line)
            if found:
                shares.append(found.group(1))
        init.stdin.write(f"{shares[0]}\n{shares[1]}\n")
        init.stdin.close()
        init.stdout.read()
        if init.wait(timeout=180) != 0:
            raise AssertionError("hecate init failed")

    def prepare(self) -> None:
        wait_until(
            lambda: (
                self.compose.run(
                    "exec", "-T", "hecate", "test", "-S", "/var/lib/hecate/control.sock", check=False
                ).returncode
                == 0
            ),
            "the local channel of Hécate",
        )
        self._init()
        self.cli("unseal", lines=(PASSPHRASE,))
        copy = "/var/lib/hecate/root-ca.pem"
        out = self.cli(
            "ca", "create", "--trust-domain", TRUST_DOMAIN, "--out", copy, lines=(CA_PASSPHRASE, copy, CA_PASSPHRASE)
        )
        found = re.search(r"root fingerprint sha256:([0-9a-f]{64})", out)
        assert found, out
        self.root_fingerprint = found.group(1)

    def token(self, identity: str) -> str:
        found = re.search(r"^token (HET1\.\S+)$", self.cli("tokens", "create", identity), re.MULTILINE)
        assert found
        return found.group(1)

    def secret(self, reference: str, value: str) -> None:
        self.cli("secrets", "create", reference, "--type", "string", lines=(value,))

    def grant_access(self, identity: str, app: str = "hermes") -> None:
        """`hecate access grant`: `identity` may call `app` from now on."""
        self.cli("access", "grant", identity, app)

    def revoke_access(self, identity: str, app: str = "hermes") -> None:
        """`hecate access revoke`: `identity` no longer calls `app`."""
        self.cli("access", "revoke", identity, app)
