"""A valid identity of Hermes written with its store, as an enrollment would leave it, for the tests of `serve`: with
its publications and the copy of its access list, which no one may call until a test grants it."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import timedelta
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec

from hermes.application.identity import IdentityService
from hermes.application.ports import AccessList, IssuedCertificate, PublishedKeys, RevocationLists, StoredIdentity
from hermes.domain.shared.references import SpiffeId
from hermes.infrastructure.clock import SystemClock
from hermes.infrastructure.hecate.client import HecateClient
from hermes.infrastructure.hecate.identity_store import FileIdentityStore
from hermes.infrastructure.hecate.keys import generate_keys
from support.pki import TestCa, spiffe


def prepare_identity(state_dir: Path, proxy_dir: Path, ca: TestCa | None = None) -> StoredIdentity:
    authority = ca or TestCa()
    keys = generate_keys()
    tls = serialization.load_pem_private_key(keys.tls_key_pem.reveal().encode("ascii"), password=None)
    assert isinstance(tls, ec.EllipticCurvePrivateKey)
    certificate = authority.certificate_for(tls.public_key(), "hermes")
    identity = StoredIdentity(
        certificate=IssuedCertificate(
            spiffe_id=SpiffeId.parse(spiffe("hermes")),
            serial=format(certificate.serial_number, "x"),
            certificate_pem=certificate.public_bytes(serialization.Encoding.PEM).decode("ascii"),
            chain_pem=(authority.intermediate_pem, authority.root_pem),
            not_after=certificate.not_valid_after_utc,
            call_key_id="k" * 43,
        ),
        tls_key_pem=keys.tls_key_pem,
        call_key_pem=keys.call_key_pem,
        root_fingerprint=authority.root_fingerprint,
    )
    store = FileIdentityStore(state_dir=state_dir, proxy_dir=proxy_dir)
    store.save(identity)
    expires = authority.now + timedelta(seconds=300)
    crl = authority.crl().public_bytes(serialization.Encoding.DER)
    store.save_revocations(RevocationLists(crls_der=(crl,), expires_at=expires))
    store.save_keys(PublishedKeys(keys=(), expires_at=expires))
    store.save_access(AccessList(app="hermes", callers=(), version=0, fetched_at=authority.now))
    return identity


@dataclass
class HermesIdentity:
    """The identity pieces of a Hermes on a directory of its own: its store, its client of Hécate and its service."""

    store: FileIdentityStore
    client: HecateClient
    identity: IdentityService


def hermes_for(url: str, base: Path) -> HermesIdentity:
    store = FileIdentityStore(state_dir=base / "state", proxy_dir=base / "proxy")
    clock = SystemClock()
    holder: list[IdentityService] = []
    client = HecateClient(
        url,
        identity=lambda: holder[0].identity,
        files=store.current_files,
        revocations=lambda: holder[0].revocation_lists(),
        clock=clock,
    )
    identity = IdentityService(store=store, hecate=client, clock=clock, new_keys=generate_keys)
    holder.append(identity)
    identity.load()
    return HermesIdentity(store=store, client=client, identity=identity)
