"""Envoy in front of Hermes (T047, contracts/proxy.md), with the configuration of hermes/deploy/envoy and an echo
upstream on the socket of the API (support/envoy.py)."""

from __future__ import annotations

import json
import socket
import ssl
import uuid
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import httpx
import pytest

from hermes.infrastructure.api.xfcc import parse_xfcc
from support.envoy import UID, compose_service, envoy_rig
from support.pki import Leaf, TestCa, spiffe
from support.xfcc import pem

pytestmark = pytest.mark.docker


@dataclass
class Proxy:
    port: int
    ca: TestCa
    workdir: Path

    def client(self, leaf: Leaf | None = None, issuer: TestCa | None = None) -> httpx.Client:
        context = ssl.create_default_context(cadata=self.ca.root_pem)
        context.check_hostname = False
        context.minimum_version = ssl.TLSVersion.TLSv1_3
        if leaf is not None:
            name = uuid.uuid4().hex
            certificate = self.workdir / f"{name}.crt"
            key = self.workdir / f"{name}.key"
            certificate.write_text(pem(leaf.certificate) + pem((issuer or self.ca).intermediate))
            key.write_text(leaf.key_pem)
            context.load_cert_chain(certificate, key)
        return httpx.Client(verify=context, base_url=f"https://127.0.0.1:{self.port}", timeout=10)


@pytest.fixture(scope="module")
def proxy(tmp_path_factory: pytest.TempPathFactory) -> Iterator[Proxy]:
    ca = TestCa()
    hermes = ca.issue("hermes")
    first = {
        "tls.crt": (pem(hermes.certificate) + ca.intermediate_pem).encode(),
        "tls.key": hermes.key_pem.encode(),
        "ca.crt": ca.root_pem.encode(),
    }
    with envoy_rig(first, ca.root_pem) as rig:
        yield Proxy(port=rig.port, ca=ca, workdir=tmp_path_factory.mktemp("proxy"))


def echoed_xfcc(response: httpx.Response) -> list[str]:
    assert response.status_code == 200, response.text
    headers: list[list[str]] = json.loads(response.text)["headers"]
    return [value for name, value in headers if name == "x-forwarded-client-cert"]


def test_the_header_of_the_client_never_arrives_only_the_one_of_its_certificate(proxy: Proxy) -> None:
    leaf = proxy.ca.issue("hades")
    spoofed = f"By={spiffe('hermes')};Hash=00;URI={spiffe('admin')}"
    with proxy.client(leaf) as client:
        [header] = echoed_xfcc(client.get("/chat/v1/send_message", headers={"X-Forwarded-Client-Cert": spoofed}))
    assert spiffe("admin") not in header
    presented = parse_xfcc(header)
    assert presented.certificate == leaf.certificate
    assert presented.chain == (leaf.certificate, proxy.ca.intermediate)
    assert presented.uri == spiffe("hades")
    assert presented.by == spiffe("hermes")


def test_without_a_client_certificate_the_request_arrives_without_the_header(proxy: Proxy) -> None:
    with proxy.client() as client:
        response = client.get("/", headers={"X-Forwarded-Client-Cert": f"URI={spiffe('admin')}"})
    assert echoed_xfcc(response) == []


def test_a_certificate_of_another_ca_arrives_with_its_header_for_hermes_to_refuse(proxy: Proxy) -> None:
    other = TestCa()
    leaf = other.issue("hades")
    with proxy.client(leaf, other) as client:
        [header] = echoed_xfcc(client.get("/"))
    presented = parse_xfcc(header)
    assert presented.certificate == leaf.certificate
    assert presented.chain[1] == other.intermediate


def test_only_tls_1_3_is_offered(proxy: Proxy) -> None:
    context = ssl.create_default_context(cadata=proxy.ca.root_pem)
    context.check_hostname = False
    context.maximum_version = ssl.TLSVersion.TLSv1_2
    with pytest.raises(ssl.SSLError), socket.create_connection(("127.0.0.1", proxy.port), timeout=5) as raw:
        context.wrap_socket(raw)


def test_the_container_of_hermes_publishes_no_port() -> None:
    assert "ports" not in compose_service("hermes")
    envoy = compose_service("envoy")
    assert [port.rsplit(":", 1)[-1] for port in envoy["ports"]] == ["8443"]
    assert envoy["image"].count(":") == 1
    assert not envoy["image"].endswith(":latest")
    assert envoy["user"] == UID
    assert "hermes-proxy:/var/lib/hermes/proxy:ro" in envoy["volumes"]
    assert "hermes-run:/run/hermes" in envoy["volumes"]
