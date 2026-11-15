"""`hermes serve` (T016, T086, T101): two private sockets, no TCP port, no proxy headers, migrations at start, the test
of each instance, one polling per instance and a clean stop.

The start follows research R1, so the test leaves a valid identity with its publications, as an enrollment would.
"""

import io
import json
import os
import re
import shutil
import signal
import stat
import subprocess
import sys
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

import psycopg
import pytest
from fastapi import FastAPI

from hermes.infrastructure.admin import cli
from hermes.infrastructure.servers import server_config
from support.identity import prepare_identity
from support.paths import short_dir
from support.postgres import Database

TESTS = Path(__file__).resolve().parents[1]


def test_uvicorn_does_not_take_the_proxy_headers() -> None:
    config = server_config(FastAPI())
    assert config.proxy_headers is False
    assert config.access_log is False
    assert config.lifespan == "off"


def wait_for(*paths: Path, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    while not all(path.exists() for path in paths):
        if time.monotonic() > deadline:
            raise AssertionError(f"not created in {timeout} s: {paths}")
        time.sleep(0.05)


def migrated_version(postgres: Database, timeout: float = 30.0) -> tuple[str, ...] | None:
    """The migrations run in the background once the sockets are open (research R1), so the check waits for them."""
    deadline = time.monotonic() + timeout
    while True:
        with psycopg.connect(postgres.conninfo()) as connection:
            table = connection.execute("SELECT to_regclass('public.alembic_version')").fetchone()
            if table is not None and table[0] is not None:
                return connection.execute("SELECT version_num FROM alembic_version").fetchone()
        if time.monotonic() > deadline:
            return None
        time.sleep(0.1)


def empty_database(postgres: Database) -> None:
    with psycopg.connect(postgres.admin_conninfo(dbname=postgres.name), autocommit=True) as connection:
        connection.execute("DROP SCHEMA IF EXISTS public CASCADE")
        connection.execute("CREATE SCHEMA public AUTHORIZATION hermes")


@contextmanager
def running_hermes(postgres: Database, base: Path, **variables: str) -> Iterator[subprocess.Popen[bytes]]:
    """`hermes serve` in its own process, with Hécate out of reach: the start lives on the publications on disk."""
    password = base / "password"
    password.write_text(postgres.password, encoding="utf-8")
    password.chmod(0o600)
    prepare_identity(base / "state", base / "proxy")
    env = {
        **os.environ,
        "PYTHONPATH": str(TESTS),
        "HERMES_STATE_DIR": str(base / "state"),
        "HERMES_PROXY_DIR": str(base / "proxy"),
        "HERMES_RUN_DIR": str(base / "run"),
        "HERMES_ADMIN_DIR": str(base / "admin"),
        "HERMES_HECATE_URL": "https://hecate.invalid:8443",
        "HERMES_DATABASE_URL": postgres.url,
        **variables,
    }
    process = subprocess.Popen(  # noqa: S603
        [sys.executable, "-m", "support.serve_with_password", str(password)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    try:
        yield process
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


@pytest.mark.docker
def test_serve_opens_its_sockets_migrates_and_stops_with_sigterm(postgres: Database) -> None:
    empty_database(postgres)
    with short_dir() as base, running_hermes(postgres, base) as process:
        api, admin = base / "run" / "api.sock", base / "admin" / "admin.sock"
        try:
            wait_for(api, admin)
            assert stat.S_IMODE(api.stat().st_mode) == 0o600
            assert stat.S_IMODE((base / "admin").stat().st_mode) == 0o700
            assert stat.S_IMODE(admin.stat().st_mode) == 0o600
            lsof = shutil.which("lsof")
            assert lsof, "lsof is needed to check that Hermes opens no TCP port"
            listening = subprocess.run(  # noqa: S603
                [lsof, "-nP", "-a", "-p", str(process.pid), "-iTCP", "-sTCP:LISTEN"],
                capture_output=True,
                text=True,
                check=False,
            )
            assert listening.stdout == ""
            assert migrated_version(postgres) == ("0001",)
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0
            assert not api.exists()
            assert not admin.exists()
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            output = process.stdout.read().decode() if process.stdout else ""
        assert postgres.password not in output


FAKE_CONFIGURATION = """\
version: 1
instances:
  - name: test_fake
    provider: fake
contacts:
  - name: contacto_fake
    channels:
      - service: chat
        provider: fake
        instance: test_fake
"""


def admin_order(admin_dir: Path, args: list[str], capsys: pytest.CaptureFixture[str], timeout: float = 30.0) -> str:
    """An order of the CLI, repeated while Hermes is still starting and answers `unavailable`."""
    deadline = time.monotonic() + timeout
    while True:
        code = cli.main(args, env={"HERMES_ADMIN_DIR": str(admin_dir)}, stdin=io.StringIO(""))
        captured = capsys.readouterr()
        if code == 0:
            return captured.out
        if time.monotonic() > deadline:
            raise AssertionError(f"{args} kept failing: {captured.err}")
        time.sleep(0.2)


def sent_lines(outbox: Path, timeout: float = 30.0) -> list[dict[str, str]]:
    deadline = time.monotonic() + timeout
    while not outbox.exists():
        if time.monotonic() > deadline:
            raise AssertionError(f"nothing sent in {timeout} s")
        time.sleep(0.1)
    return [json.loads(line) for line in outbox.read_text(encoding="utf-8").splitlines()]


@pytest.mark.docker
def test_serve_polls_each_instance_and_verifies_the_channel_with_the_code(
    postgres: Database, capsys: pytest.CaptureFixture[str]
) -> None:
    empty_database(postgres)
    with short_dir() as base:
        fake = base / "fake"
        variables = {"HERMES_FAKE_PROVIDERS": "enabled", "HERMES_FAKE_DIR": str(fake)}
        with running_hermes(postgres, base, **variables) as process:
            admin_dir = base / "admin"
            wait_for(admin_dir / "admin.sock")
            config = base / "hermes.yaml"
            config.write_text(FAKE_CONFIGURATION, encoding="utf-8")
            assert "applied 3 changes" in admin_order(admin_dir, ["config", "apply", str(config), "--yes"], capsys)
            out = admin_order(admin_dir, ["contacts", "code", "contacto_fake", "--instance", "test_fake"], capsys)
            match = re.match(r"code (\S+) for contacto_fake on test_fake expires ", out)
            assert match is not None
            inbox = fake / "test_fake" / "inbox"
            inbox.mkdir(parents=True)
            message = {"chat": "chat-ana", "chat_type": "private", "text": match.group(1)}
            (inbox / "001.tmp").write_text(json.dumps(message), encoding="utf-8")
            (inbox / "001.tmp").rename(inbox / "001.json")
            sent = sent_lines(fake / "test_fake" / "outbox.jsonl")
            assert [(line["chat"], line["text"]) for line in sent] == [("chat-ana", "Canal verificado.")]
            with psycopg.connect(postgres.conninfo()) as connection:
                row = connection.execute(
                    "SELECT address, verified_at IS NOT NULL, code_hash FROM channels WHERE contact = 'contacto_fake'"
                ).fetchone()
            assert row == ("chat-ana", True, None)
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0


def output_lines(process: subprocess.Popen[bytes]) -> list[str]:
    """The lines that Hermes writes, read in a thread while it runs."""
    lines: list[str] = []

    def read() -> None:
        assert process.stdout is not None
        for raw in iter(process.stdout.readline, b""):
            lines.append(raw.decode())

    threading.Thread(target=read, daemon=True).start()
    return lines


def wait_for_event(lines: list[str], event: str, timeout: float = 30.0) -> dict[str, object]:
    deadline = time.monotonic() + timeout
    while True:
        for line in list(lines):
            entry = json.loads(line)
            if entry.get("event") == event:
                return dict(entry)
        if time.monotonic() > deadline:
            raise AssertionError(f"no {event} in {timeout} s")
        time.sleep(0.1)


@pytest.mark.docker
def test_serve_does_not_start_with_a_configuration_of_the_fake_without_its_variable(
    postgres: Database, capsys: pytest.CaptureFixture[str]
) -> None:
    empty_database(postgres)
    with short_dir() as base:
        config = base / "hermes.yaml"
        config.write_text(FAKE_CONFIGURATION, encoding="utf-8")
        with running_hermes(postgres, base, HERMES_FAKE_PROVIDERS="enabled") as process:
            wait_for(base / "admin" / "admin.sock")
            admin_order(base / "admin", ["config", "apply", str(config), "--yes"], capsys)
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0
        with running_hermes(postgres, base) as process:
            lines = output_lines(process)
            invalid = wait_for_event(lines, "configuration.invalid")
            assert invalid["problems"] == [
                "instances[0].provider: the fake provider needs HERMES_FAKE_PROVIDERS=enabled"
            ]
            failed = wait_for_event(lines, "startup.failed")
            assert (failed["step"], failed["reason"]) == ("configuration", "validation")
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0
        assert not any(json.loads(line).get("event") == "startup.ready" for line in lines)


@pytest.mark.docker
def test_serve_tests_each_instance_at_the_start_and_says_when_it_is_ready(
    postgres: Database, capsys: pytest.CaptureFixture[str]
) -> None:
    empty_database(postgres)
    with short_dir() as base:
        config = base / "hermes.yaml"
        config.write_text(FAKE_CONFIGURATION, encoding="utf-8")
        fake = {"HERMES_FAKE_PROVIDERS": "enabled"}
        with running_hermes(postgres, base, **fake) as process:
            wait_for(base / "admin" / "admin.sock")
            admin_order(base / "admin", ["config", "apply", str(config), "--yes"], capsys)
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0
        with running_hermes(postgres, base, **fake) as process:
            lines = output_lines(process)
            tested = wait_for_event(lines, "instance.tested")
            assert (tested["instance"], tested["status"]) == ("test_fake", "healthy")
            assert admin_order(base / "admin", ["health"], capsys) == "ready\n"
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=15) == 0
