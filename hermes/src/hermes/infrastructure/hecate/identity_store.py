"""The identity of Hermes in files (research R4, R2).

- The state directory (`HERMES_STATE_DIR`, 0700, files 0600) keeps the metadata, the call key of each generation, the
  renewal in flight and the last publications of Hécate. Envoy never sees it.
- The proxy directory (`HERMES_PROXY_DIR`) keeps each certificate in `gen-<n>/` (`tls.crt` with the certificate and
  its intermediate, `tls.key` and `ca.crt`), and the link `current` is replaced at once, with a rename, so Envoy reloads
  it without a restart. Only the current generation and the previous one stay.
"""

from __future__ import annotations

import base64
import contextlib
import json
import os
import re
import shutil
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

from cryptography import x509
from cryptography.hazmat.primitives import serialization

from hermes.application.ports import (
    IssuedCertificate,
    PendingRenewal,
    PublishedKeys,
    RevocationLists,
    StoredIdentity,
)
from hermes.domain.shared.references import SpiffeId
from hermes.domain.shared.secret import SecretValue

PRIVATE = 0o700
FILE = 0o600
GENERATION = re.compile(r"gen-([1-9][0-9]*)")


class FileIdentityStore:
    def __init__(self, *, state_dir: Path, proxy_dir: Path) -> None:
        self.state_dir = state_dir
        self.proxy_dir = proxy_dir

    # --- Files -----------------------------------------------------------------------------------------------------

    def _prepare(self) -> None:
        for directory in (self.state_dir, self.proxy_dir):
            directory.mkdir(parents=True, exist_ok=True, mode=PRIVATE)
            directory.chmod(PRIVATE)

    @staticmethod
    def _write(path: Path, text: str) -> None:
        temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}")
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, FILE)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                handle.write(text)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise

    @staticmethod
    def _read_json(path: Path) -> Any:
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return None

    def _generations(self) -> list[int]:
        if not self.proxy_dir.is_dir():
            return []
        found = (GENERATION.fullmatch(entry.name) for entry in self.proxy_dir.iterdir() if entry.is_dir())
        return sorted(int(match.group(1)) for match in found if match is not None)

    def _point_current(self, generation: int) -> None:
        link = self.proxy_dir / "current"
        if link.is_symlink() and os.readlink(link) == f"gen-{generation}":
            return
        temporary = self.proxy_dir / f".current.{uuid.uuid4().hex}"
        os.symlink(f"gen-{generation}", temporary)
        os.replace(temporary, link)

    def _prune(self, keep: set[int]) -> None:
        for generation in self._generations():
            if generation not in keep:
                shutil.rmtree(self.proxy_dir / f"gen-{generation}", ignore_errors=True)
        for path in self.state_dir.glob("call_key-*.pem"):
            number = path.name.removeprefix("call_key-").removesuffix(".pem")
            if number.isdigit() and int(number) not in keep:
                path.unlink(missing_ok=True)

    def current_files(self) -> tuple[Path, Path, Path] | None:
        """The certificate with its intermediate, the TLS key and the root CA of the current generation."""
        meta = self._read_json(self.state_dir / "identity.json")
        if meta is None:
            return None
        folder = self.proxy_dir / f"gen-{meta['generation']}"
        return folder / "tls.crt", folder / "tls.key", folder / "ca.crt"

    # --- Identity --------------------------------------------------------------------------------------------------

    def load(self) -> StoredIdentity | None:
        meta = self._read_json(self.state_dir / "identity.json")
        if meta is None:
            return None
        generation = int(meta["generation"])
        folder = self.proxy_dir / f"gen-{generation}"
        chain = x509.load_pem_x509_certificates((folder / "tls.crt").read_bytes())
        certificate, intermediate = (item.public_bytes(serialization.Encoding.PEM).decode("ascii") for item in chain)
        self._point_current(generation)
        return StoredIdentity(
            certificate=IssuedCertificate(
                spiffe_id=SpiffeId.parse(meta["spiffe_id"]),
                serial=meta["serial"],
                certificate_pem=certificate,
                chain_pem=(intermediate, (folder / "ca.crt").read_text(encoding="utf-8")),
                not_after=datetime.fromisoformat(meta["not_after"]),
                call_key_id=meta["call_key_id"],
            ),
            tls_key_pem=SecretValue((folder / "tls.key").read_text(encoding="utf-8")),
            call_key_pem=SecretValue((self.state_dir / f"call_key-{generation}.pem").read_text(encoding="utf-8")),
            root_fingerprint=meta["root_fingerprint"],
        )

    def save(self, identity: StoredIdentity) -> None:
        """A new generation with the certificate, then its call key and its metadata, and last the link `current`."""
        self._prepare()
        generations = self._generations()
        generation = (generations[-1] if generations else 0) + 1
        folder = self.proxy_dir / f"gen-{generation}"
        folder.mkdir(mode=PRIVATE)
        certificate = identity.certificate
        self._write(folder / "tls.crt", certificate.certificate_pem + certificate.chain_pem[0])
        self._write(folder / "tls.key", identity.tls_key_pem.reveal())
        self._write(folder / "ca.crt", certificate.chain_pem[-1])
        self._write(self.state_dir / f"call_key-{generation}.pem", identity.call_key_pem.reveal())
        meta = {
            "spiffe_id": str(certificate.spiffe_id),
            "serial": certificate.serial,
            "not_after": certificate.not_after.isoformat(),
            "call_key_id": certificate.call_key_id,
            "root_fingerprint": identity.root_fingerprint,
            "generation": generation,
        }
        self._write(self.state_dir / "identity.json", json.dumps(meta, indent=2))
        self._point_current(generation)
        self._prune({generation, generations[-1]} if generations else {generation})

    # --- Renewal in flight -----------------------------------------------------------------------------------------

    def load_pending_renewal(self) -> PendingRenewal | None:
        data = self._read_json(self.state_dir / "pending_renewal.json")
        if data is None:
            return None
        return PendingRenewal(
            tls_key_pem=SecretValue(data["tls_key_pem"]),
            call_key_pem=SecretValue(data["call_key_pem"]),
            csr_pem=data["csr_pem"],
            call_public_pem=data["call_public_pem"],
            idempotency_key=data["idempotency_key"],
        )

    def save_pending_renewal(self, pending: PendingRenewal) -> None:
        self._prepare()
        data = {
            "tls_key_pem": pending.tls_key_pem.reveal(),
            "call_key_pem": pending.call_key_pem.reveal(),
            "csr_pem": pending.csr_pem,
            "call_public_pem": pending.call_public_pem,
            "idempotency_key": pending.idempotency_key,
        }
        self._write(self.state_dir / "pending_renewal.json", json.dumps(data))

    def clear_pending_renewal(self) -> None:
        with contextlib.suppress(FileNotFoundError):
            (self.state_dir / "pending_renewal.json").unlink()

    # --- Publications of Hécate ------------------------------------------------------------------------------------

    def load_revocations(self) -> RevocationLists | None:
        data = self._read_json(self.state_dir / "revocations.json")
        if data is None:
            return None
        crls = tuple(base64.b64decode(item) for item in data["crls"])
        return RevocationLists(crls_der=crls, expires_at=datetime.fromisoformat(data["expires_at"]))

    def save_revocations(self, revocations: RevocationLists) -> None:
        self._prepare()
        data = {
            "crls": [base64.b64encode(item).decode("ascii") for item in revocations.crls_der],
            "expires_at": revocations.expires_at.isoformat(),
        }
        self._write(self.state_dir / "revocations.json", json.dumps(data))

    def load_keys(self) -> PublishedKeys | None:
        data = self._read_json(self.state_dir / "keys.json")
        if data is None:
            return None
        return PublishedKeys(keys=tuple(data["keys"]), expires_at=datetime.fromisoformat(data["expires_at"]))

    def save_keys(self, keys: PublishedKeys) -> None:
        self._prepare()
        data = {"keys": [dict(key) for key in keys.keys], "expires_at": keys.expires_at.isoformat()}
        self._write(self.state_dir / "keys.json", json.dumps(data))
