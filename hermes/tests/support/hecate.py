"""A real Hécate for the tests (research R18): `hecate serve` from `HECATE_BIN`, with its own data directory, the
`moderate` profile and certificates of one hour, prepared through the standard input of its CLI (docs/hecate/cli.md).

Without `HECATE_BIN` the tests that need it are skipped.
"""

from __future__ import annotations

import os
import re
import socket
import subprocess
import time
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import pytest

from support.paths import short_dir

PASSPHRASE = "orchid lantern quarry 42 unseal"
CA_PASSPHRASE = "harbor velvet prism 77 root"
TRUST_DOMAIN = "cerbero.test"


def _free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port: int = probe.getsockname()[1]
        return port


def _wait(condition: object, what: str, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    while not condition():  # type: ignore[operator]
        if time.monotonic() > deadline:
            raise AssertionError(f"{what} did not happen in {timeout} s")
        time.sleep(0.05)


def _listening(port: int) -> bool:
    with socket.socket() as probe:
        return probe.connect_ex(("127.0.0.1", port)) == 0


@dataclass
class RunningHecate:
    binary: str
    base: Path
    port: int
    process: subprocess.Popen[bytes]
    root_fingerprint: str = ""

    @property
    def env(self) -> dict[str, str]:
        return {
            **os.environ,
            "HECATE_DATA_DIR": str(self.base / "data"),
            "HECATE_KDF_PROFILE": "moderate",
            "HECATE_API_ADDR": f"127.0.0.1:{self.port}",
            "HECATE_CERT_LIFETIME_HOURS": "1",
        }

    @property
    def url(self) -> str:
        return f"https://127.0.0.1:{self.port}"

    def cli(self, *args: str, lines: tuple[str, ...] = ()) -> str:
        done = subprocess.run(  # noqa: S603
            [self.binary, *args],
            env=self.env,
            input="".join(f"{line}\n" for line in lines),
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        if done.returncode != 0:
            raise AssertionError(f"hecate {args[0]} failed with {done.returncode}: {done.stderr.strip()}")
        return done.stdout

    def _init(self) -> None:
        init = subprocess.Popen(  # noqa: S603
            [self.binary, "init"],
            env=self.env,
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
        if init.wait(timeout=120) != 0:
            raise AssertionError("hecate init failed")

    def prepare(self) -> None:
        """Initialization, unsealing and the CA, as the start-up of Hécate (docs/hecate/operacion.md) describes."""
        self._init()
        self.unseal()
        copy = self.base / "root-ca.pem"
        out = self.cli(
            "ca",
            "create",
            "--trust-domain",
            TRUST_DOMAIN,
            "--out",
            str(copy),
            lines=(CA_PASSPHRASE, str(copy), CA_PASSPHRASE),
        )
        found = re.search(r"root fingerprint sha256:([0-9a-f]{64})", out)
        assert found, out
        self.root_fingerprint = found.group(1)
        _wait(lambda: _listening(self.port), "the API of Hécate listening")

    def unseal(self) -> None:
        self.cli("unseal", lines=(PASSPHRASE,))

    def seal(self) -> None:
        self.cli("seal")

    def token(self, identity: str = "hermes") -> str:
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

    def stop(self) -> None:
        self.process.terminate()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def start_hecate(binary: str, base: Path) -> RunningHecate:
    port = _free_port()
    log = (base / "serve.log").open("wb")
    running = RunningHecate(binary=binary, base=base, port=port, process=None)  # type: ignore[arg-type]
    running.process = subprocess.Popen(  # noqa: S603
        [binary, "serve"], env=running.env, stdout=log, stderr=subprocess.STDOUT
    )
    try:
        _wait(lambda: (base / "data" / "control.sock").exists(), "the local channel of Hécate")
        running.prepare()
    except BaseException:
        running.stop()
        print((base / "serve.log").read_text(errors="replace")[-4000:])
        raise
    return running


@pytest.fixture(scope="session")
def hecate_bin() -> str:
    binary = os.environ.get("HECATE_BIN", "")
    if not binary:
        pytest.skip("HECATE_BIN is not set: the tests against a real Hécate are skipped")
    return binary


@pytest.fixture(scope="module")
def hecate(hecate_bin: str) -> Iterator[RunningHecate]:
    with short_dir() as base:
        running = start_hecate(hecate_bin, base)
        try:
            yield running
        finally:
            running.stop()


@pytest.fixture(scope="module")
def other_hecate(hecate_bin: str) -> Iterator[RunningHecate]:
    """A second Hécate, with another root CA."""
    with short_dir() as base:
        running = start_hecate(hecate_bin, base)
        try:
            yield running
        finally:
            running.stop()
