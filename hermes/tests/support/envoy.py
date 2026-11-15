"""Envoy with the configuration of hermes/deploy/envoy and an echo upstream on the socket of the API, in Docker.

Envoy and the upstream share two named volumes: the directory of the proxy, with `current -> gen-<n>`, and the one of
the API socket. A keeper container writes into them, so nothing is bind-mounted from the host: a Unix socket and the
watch of a directory do not cross the VM of Docker on macOS.
"""

from __future__ import annotations

import io
import socket
import ssl
import tarfile
import time
import uuid
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from support.postgres import use_docker_context

HERMES = Path(__file__).resolve().parents[2]
ENVOY_DIR = HERMES / "deploy" / "envoy"
COMPOSE = HERMES / "deploy" / "compose.yaml"
ECHO = HERMES / "tests" / "e2e" / "stubs" / "echo_upstream.py"
PYTHON_IMAGE = "python:3.15.2-slim-trixie"
UID = "10001:10001"


def compose_service(name: str) -> dict[str, Any]:
    services: dict[str, Any] = yaml.safe_load(COMPOSE.read_text(encoding="utf-8"))["services"]
    service: dict[str, Any] = services[name]
    return service


ENVOY_IMAGE: str = compose_service("envoy")["image"]


def tar_of(files: dict[str, bytes], mode: int = 0o600) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as archive:
        for directory in sorted({str(Path(name).parent) for name in files} - {"."}):
            info = tarfile.TarInfo(directory)
            info.type = tarfile.DIRTYPE
            info.mode = 0o755 if mode & 0o044 else 0o700
            archive.addfile(info)
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(content)
            info.mode = mode
            archive.addfile(info, io.BytesIO(content))
    return buffer.getvalue()


def wait_for_tls(port: int, root_pem: str, timeout: float = 60.0) -> None:
    context = ssl.create_default_context(cadata=root_pem)
    context.check_hostname = False
    deadline = time.monotonic() + timeout
    while True:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=2) as raw, context.wrap_socket(raw):
                return
        except OSError:
            if time.monotonic() > deadline:
                raise
            time.sleep(0.5)


@dataclass
class EnvoyRig:
    port: int
    envoy: Any
    keeper: Any

    def run(self, *command: str) -> None:
        result = self.keeper.exec_run(list(command), user="0")
        assert result.exit_code == 0, result.output.decode(errors="replace")

    def publish(self, generation: int, files: dict[str, bytes]) -> None:
        """A new generation in the directory of the proxy and the link `current` replaced with a rename."""
        self.keeper.put_archive("/proxy", tar_of({f"gen-{generation}/{name}": data for name, data in files.items()}))
        self.run("chown", "-R", UID, f"/proxy/gen-{generation}")
        self.run("ln", "-s", f"gen-{generation}", "/proxy/.current.new")
        self.run("chown", "-h", UID, "/proxy/.current.new")
        self.run("mv", "-T", "/proxy/.current.new", "/proxy/current")

    def logs(self) -> str:
        text: str = self.envoy.logs().decode(errors="replace")
        return text


@contextmanager
def envoy_rig(first_generation: dict[str, bytes], root_pem: str) -> Iterator[EnvoyRig]:
    """Envoy serving the first generation; `first_generation` has `tls.crt`, `tls.key` and `ca.crt`."""
    use_docker_context()
    import docker

    client = docker.from_env()
    suffix = uuid.uuid4().hex[:8]
    proxy = client.volumes.create(f"hermes-proxy-test-{suffix}")
    run = client.volumes.create(f"hermes-run-test-{suffix}")
    containers: list[Any] = []
    try:
        for image in (PYTHON_IMAGE, ENVOY_IMAGE):
            client.images.pull(image)
        keeper = client.containers.run(
            PYTHON_IMAGE,
            command=["sleep", "infinity"],
            user="0",
            volumes={proxy.name: {"bind": "/proxy"}, run.name: {"bind": "/run/hermes"}},
            detach=True,
        )
        containers.append(keeper)
        keeper.exec_run(["chown", UID, "/proxy", "/run/hermes"], user="0")
        rig = EnvoyRig(port=0, envoy=None, keeper=keeper)
        rig.publish(1, first_generation)
        echo = client.containers.run(
            PYTHON_IMAGE,
            command=["python3", "-c", ECHO.read_text(encoding="utf-8"), "/run/hermes/api.sock"],
            user=UID,
            volumes={run.name: {"bind": "/run/hermes"}},
            detach=True,
        )
        containers.append(echo)
        envoy = client.containers.create(
            ENVOY_IMAGE,
            command=compose_service("envoy")["command"],
            user=UID,
            volumes={
                proxy.name: {"bind": "/var/lib/hermes/proxy", "mode": "ro"},
                run.name: {"bind": "/run/hermes"},
            },
            ports={"8443/tcp": None},
        )
        containers.append(envoy)
        configuration = {
            "envoy.yaml": (ENVOY_DIR / "envoy.yaml").read_bytes(),
            "sds/certificate.yaml": (ENVOY_DIR / "sds" / "certificate.yaml").read_bytes(),
            "sds/validation.yaml": (ENVOY_DIR / "sds" / "validation.yaml").read_bytes(),
        }
        envoy.put_archive("/etc/envoy", tar_of(configuration, mode=0o644))
        envoy.start()
        envoy.reload()
        rig.envoy = envoy
        rig.port = int(envoy.ports["8443/tcp"][0]["HostPort"])
        try:
            wait_for_tls(rig.port, root_pem)
        except OSError:
            print(rig.logs())
            print(echo.logs().decode(errors="replace"))
            raise
        yield rig
    finally:
        for container in containers:
            container.remove(force=True)
        for volume in (proxy, run):
            volume.remove(force=True)
