"""The identity store in files (T054, research R4): the state directory and the generations of the proxy directory."""

import os
import stat
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest
from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec

from hermes.application.ports import (
    IssuedCertificate,
    PendingRenewal,
    PublishedKeys,
    RevocationLists,
    StoredIdentity,
)
from hermes.domain.shared.references import SpiffeId
from hermes.infrastructure.hecate.identity_store import FileIdentityStore
from hermes.infrastructure.hecate.keys import generate_keys
from support.pki import TestCa, spiffe

CA = TestCa()


def identity(serial_shift: int = 0) -> StoredIdentity:
    keys = generate_keys()
    tls = serialization.load_pem_private_key(keys.tls_key_pem.reveal().encode("ascii"), password=None)
    assert isinstance(tls, ec.EllipticCurvePrivateKey)
    certificate = CA.certificate_for(tls.public_key(), "hermes", not_before=CA.now + timedelta(minutes=serial_shift))
    return StoredIdentity(
        certificate=IssuedCertificate(
            spiffe_id=SpiffeId.parse(spiffe("hermes")),
            serial=format(certificate.serial_number, "x"),
            certificate_pem=certificate.public_bytes(serialization.Encoding.PEM).decode("ascii"),
            chain_pem=(CA.intermediate_pem, CA.root_pem),
            not_after=certificate.not_valid_after_utc,
            call_key_id="k" * 43,
        ),
        tls_key_pem=keys.tls_key_pem,
        call_key_pem=keys.call_key_pem,
        root_fingerprint=CA.root_fingerprint,
    )


def store(base: Path) -> FileIdentityStore:
    return FileIdentityStore(state_dir=base / "state", proxy_dir=base / "proxy")


def mode(path: Path) -> int:
    return stat.S_IMODE(path.lstat().st_mode)


def test_without_an_identity_there_is_nothing_to_load(tmp_path: Path) -> None:
    assert store(tmp_path).load() is None
    assert store(tmp_path).load_pending_renewal() is None
    assert store(tmp_path).load_revocations() is None
    assert store(tmp_path).load_keys() is None


def test_an_identity_is_written_as_a_generation_with_current_pointing_to_it(tmp_path: Path) -> None:
    saved = identity()
    store(tmp_path).save(saved)
    proxy = tmp_path / "proxy"
    assert (proxy / "current").is_symlink()
    assert os.readlink(proxy / "current") == "gen-1"
    generation = proxy / "gen-1"
    chain = x509.load_pem_x509_certificates((generation / "tls.crt").read_bytes())
    assert [item.public_bytes(serialization.Encoding.PEM).decode() for item in chain] == [
        saved.certificate.certificate_pem,
        CA.intermediate_pem,
    ]
    assert (generation / "ca.crt").read_text() == CA.root_pem
    assert (generation / "tls.key").read_text() == saved.tls_key_pem.reveal()
    assert mode(generation / "tls.key") == 0o600


def test_each_save_is_a_new_generation_and_only_the_previous_one_stays(tmp_path: Path) -> None:
    files = store(tmp_path)
    for shift in range(3):
        files.save(identity(shift))
    proxy = tmp_path / "proxy"
    assert os.readlink(proxy / "current") == "gen-3"
    assert sorted(path.name for path in proxy.iterdir()) == ["current", "gen-2", "gen-3"]


def test_the_link_is_replaced_with_a_rename_so_it_never_goes_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    files = store(tmp_path)
    files.save(identity())
    renames: list[tuple[str, str]] = []
    original = os.replace

    def recording(source: str | os.PathLike[str], target: str | os.PathLike[str]) -> None:
        renames.append((str(source), str(target)))
        original(source, target)

    monkeypatch.setattr(os, "replace", recording)
    files.save(identity(1))
    assert any(Path(target).name == "current" for _, target in renames)
    assert os.readlink(tmp_path / "proxy" / "current") == "gen-2"
    assert [name for name in os.listdir(tmp_path / "proxy") if name.startswith(".")] == []


def test_the_state_directory_is_0700_and_its_files_0600(tmp_path: Path) -> None:
    files = store(tmp_path)
    files.save(identity())
    files.save_pending_renewal(PendingRenewal(**_pending_fields()))
    state = tmp_path / "state"
    assert mode(state) == 0o700
    assert all(mode(path) == 0o600 for path in state.iterdir())
    assert mode(tmp_path / "proxy") == 0o700


def test_what_is_saved_is_read_back(tmp_path: Path) -> None:
    saved = identity()
    store(tmp_path).save(saved)
    assert store(tmp_path).load() == saved


def test_the_metadata_never_holds_a_private_key(tmp_path: Path) -> None:
    store(tmp_path).save(identity())
    for path in (tmp_path / "state").iterdir():
        if not path.name.startswith("call_key-"):
            assert "PRIVATE KEY" not in path.read_text()


def _pending_fields() -> dict[str, object]:
    keys = generate_keys()
    return {
        "tls_key_pem": keys.tls_key_pem,
        "call_key_pem": keys.call_key_pem,
        "csr_pem": keys.csr_pem,
        "call_public_pem": keys.call_public_pem,
        "idempotency_key": "renew-1",
    }


def test_the_pending_renewal_is_kept_until_it_is_cleared(tmp_path: Path) -> None:
    pending = PendingRenewal(**_pending_fields())  # type: ignore[arg-type]
    store(tmp_path).save_pending_renewal(pending)
    assert store(tmp_path).load_pending_renewal() == pending
    store(tmp_path).clear_pending_renewal()
    assert store(tmp_path).load_pending_renewal() is None


def test_the_last_publications_are_kept(tmp_path: Path) -> None:
    files = store(tmp_path)
    expires = datetime(2026, 11, 14, 10, 5, tzinfo=UTC)
    crl = CA.crl([7]).public_bytes(serialization.Encoding.DER)
    files.save_revocations(RevocationLists(crls_der=(crl,), expires_at=expires))
    files.save_keys(PublishedKeys(keys=({"kid": "k", "cerbero_status": "active"},), expires_at=expires))
    again = store(tmp_path)
    assert again.load_revocations() == RevocationLists(crls_der=(crl,), expires_at=expires)
    assert again.load_keys() == PublishedKeys(keys=({"kid": "k", "cerbero_status": "active"},), expires_at=expires)
