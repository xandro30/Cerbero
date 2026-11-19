"""The client of Hécate against a real Hécate (T060), launched from `HECATE_BIN` with certificates of one hour; the
access list of Hermes, with its copy on disk (T026 of 009); and the urgent revocation of the certificate of Hermes, with
the rows that its calls leave in the access log of Hécate (change 011)."""

from __future__ import annotations

import sqlite3
from collections.abc import AsyncIterator
from contextlib import closing
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest
from cryptography import x509
from cryptography.x509.verification import PolicyBuilder, Store

from hermes.application.ports import HecateTrustChanged, StoredIdentity
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.hecate.keys import generate_keys
from support.hecate import TRUST_DOMAIN, RunningHecate
from support.identity import HermesIdentity as Hermes
from support.identity import hermes_for

pytestmark = [pytest.mark.anyio, pytest.mark.hecate]

BOT_TOKEN_REF = "secret://hermes/telegram/family_bot/bot_token"
PASSWORD_REF = "secret://hermes/postgres/password"


@pytest.fixture(scope="module")
def prepared(hecate: RunningHecate) -> RunningHecate:
    hecate.secret(BOT_TOKEN_REF, "123:test-bot-token")
    hecate.secret(PASSWORD_REF, "test-db-password")
    return hecate


@pytest.fixture
async def hermes(prepared: RunningHecate, tmp_path: Path) -> AsyncIterator[Hermes]:
    created = hermes_for(prepared.url, tmp_path)
    await created.identity.enroll(SecretValue(prepared.token()))
    yield created
    await created.client.aclose()


async def test_the_enrollment_gives_the_certificate_its_chain_and_the_kid_of_the_call_key(
    prepared: RunningHecate, tmp_path: Path
) -> None:
    hermes = hermes_for(prepared.url, tmp_path)
    issued = await hermes.identity.enroll(SecretValue(prepared.token()))
    await hermes.client.aclose()
    assert str(issued.spiffe_id) == f"spiffe://{TRUST_DOMAIN}/hermes"
    assert len(issued.chain_pem) == 2
    assert len(issued.call_key_id) == 43
    assert timedelta(minutes=55) < issued.not_after - datetime.now(UTC) <= timedelta(hours=1)
    root = x509.load_pem_x509_certificate(issued.chain_pem[1].encode())
    intermediate = x509.load_pem_x509_certificate(issued.chain_pem[0].encode())
    certificate = x509.load_pem_x509_certificate(issued.certificate_pem.encode())
    verifier = PolicyBuilder().store(Store([root])).time(datetime.now(UTC)).build_client_verifier()
    verifier.verify(certificate, [intermediate])
    assert hermes.store.load() is not None
    assert hermes.identity.problem() is None


async def test_a_hecate_with_another_root_ca_is_refused_and_the_token_stays_unused(
    prepared: RunningHecate, other_hecate: RunningHecate, tmp_path: Path
) -> None:
    token = prepared.token()
    wrong = hermes_for(other_hecate.url, tmp_path / "wrong")
    with pytest.raises(HermesError) as raised:
        await wrong.identity.enroll(SecretValue(token))
    await wrong.client.aclose()
    assert raised.value.code is ErrorCode.AUTH
    assert wrong.identity.identity is None
    right = hermes_for(prepared.url, tmp_path / "right")
    issued = await right.identity.enroll(SecretValue(token))
    await right.client.aclose()
    assert str(issued.spiffe_id) == f"spiffe://{TRUST_DOMAIN}/hermes"


async def test_the_renewal_gives_a_new_certificate_and_repeating_it_gives_the_same(hermes: Hermes) -> None:
    before = hermes.identity.identity
    assert before is not None
    keys = generate_keys()
    first = await hermes.client.renew(keys.csr_pem, keys.call_public_pem, "renewal-repeated")
    again = await hermes.client.renew(keys.csr_pem, keys.call_public_pem, "renewal-repeated")
    assert first.serial != before.certificate.serial
    assert again.serial == first.serial
    assert again.certificate_pem == first.certificate_pem


async def test_after_a_renewal_hermes_calls_with_its_new_certificate(hermes: Hermes) -> None:
    before = hermes.identity.identity
    assert before is not None
    await hermes.identity.renew()
    after = hermes.identity.identity
    assert after is not None
    assert after.certificate.serial != before.certificate.serial
    [resolved] = await hermes.client.resolve([SecretReference.parse(BOT_TOKEN_REF)])
    assert resolved.value == SecretValue("123:test-bot-token")


async def test_the_resolution_is_all_or_nothing(hermes: Hermes) -> None:
    both = await hermes.client.resolve([SecretReference.parse(BOT_TOKEN_REF), SecretReference.parse(PASSWORD_REF)])
    assert [item.value for item in both] == [SecretValue("123:test-bot-token"), SecretValue("test-db-password")]
    assert {(item.cache_ttl_soft_s, item.cache_ttl_hard_s) for item in both} == {(300, 3600)}
    with pytest.raises(HermesError) as raised:
        await hermes.client.resolve([SecretReference.parse(BOT_TOKEN_REF), SecretReference.parse(f"{PASSWORD_REF}x")])
    assert raised.value.code is ErrorCode.NOT_FOUND


async def test_the_crls_and_the_published_keys_are_read(hermes: Hermes) -> None:
    revocations = await hermes.client.revocations()
    assert revocations.crls_der
    [crl] = [x509.load_der_x509_crl(item) for item in revocations.crls_der]
    assert crl.next_update_utc is not None
    keys = await hermes.client.keys()
    identity = hermes.identity.identity
    assert identity is not None
    mine = [key for key in keys.keys if key["kid"] == identity.certificate.call_key_id]
    assert [(key["cerbero_identity"], key["cerbero_status"]) for key in mine] == [
        (f"spiffe://{TRUST_DOMAIN}/hermes", "active")
    ]
    assert (await hermes.client.key(identity.certificate.call_key_id))["kid"] == identity.certificate.call_key_id


async def test_the_access_list_is_empty_without_grants_and_has_the_caller_after_a_grant(
    prepared: RunningHecate, hermes: Hermes
) -> None:
    empty = await hermes.client.callers()
    assert (empty.app, empty.callers, empty.version) == ("hermes", (), 0)
    prepared.grant_access("caller")
    try:
        granted = await hermes.client.callers()
        assert (granted.app, granted.callers, granted.version) == ("hermes", ("caller",), 1)
        await hermes.identity.refresh_publications()
        saved = hermes.store.load_access()
        assert saved is not None
        assert (saved.callers, saved.version) == (("caller",), 1)
        material = hermes.identity.material()
        assert material is not None
        assert material.access == saved
    finally:
        prepared.revoke_access("caller")


async def test_with_hecate_sealed_the_calls_are_unavailable(prepared: RunningHecate, hermes: Hermes) -> None:
    prepared.seal()
    try:
        with pytest.raises(HermesError) as raised:
            await hermes.client.resolve([SecretReference.parse(BOT_TOKEN_REF)])
        assert raised.value.code is ErrorCode.UNAVAILABLE
        with pytest.raises(HermesError) as raised:
            await hermes.client.callers()
        assert raised.value.code is ErrorCode.UNAVAILABLE
    finally:
        prepared.unseal()


# --- The urgent revocation of Hermes (change 011) -------------------------------------------------------------------

REFUSED = "hecate refused the identity of hermes: it needs a new enrollment token"


def access_rows(hecate: RunningHecate, identity: StoredIdentity) -> int:
    """The rows that the calls of this Hermes leave in the access log of Hécate (docs/hecate/auditoria.md): the ones
    with its SPIFFE ID as `subject` and its serial. The orders of the admin about Hermes, like the revocation, carry
    `subject` = `admin`."""
    database = hecate.base / "data" / "hecate.db"
    with closing(sqlite3.connect(f"file:{database}?mode=ro", uri=True)) as connection:
        [(count,)] = connection.execute(
            "SELECT count(*) FROM access_log WHERE subject = ? AND certificate = ?",
            (str(identity.certificate.spiffe_id), identity.certificate.serial),
        ).fetchall()
    return int(count)


def revoke_urgently(hecate: RunningHecate, identity: StoredIdentity) -> None:
    out = hecate.cli("certificates", "revoke", identity.certificate.serial, "--urgent")
    assert out.startswith(f"revoked {identity.certificate.serial}"), out


async def test_after_an_urgent_revocation_hecate_answers_auth_and_hermes_stops_calling_with_that_certificate(
    prepared: RunningHecate, hermes: Hermes
) -> None:
    """Each refused call leaves a row of Hermes in the access log; the blocked one leaves none (SC-006)."""
    identity = hermes.identity.identity
    assert identity is not None
    revoke_urgently(prepared, identity)
    rows = access_rows(prepared, identity)
    with pytest.raises(HermesError) as raised:
        await hermes.client.revocations()
    assert raised.value.code is ErrorCode.AUTH
    assert not isinstance(raised.value, HecateTrustChanged)
    assert access_rows(prepared, identity) == rows + 1
    await hermes.identity.refresh_publications()
    assert hermes.identity.problem() == "needs a new enrollment token"
    assert access_rows(prepared, identity) == rows + 2
    with pytest.raises(HermesError) as raised:
        await hermes.identity.resolve([SecretReference.parse(PASSWORD_REF)])
    assert (raised.value.code, raised.value.message) == (ErrorCode.AUTH, REFUSED)
    assert access_rows(prepared, identity) == rows + 2


async def test_a_hermes_started_again_on_the_same_directories_finds_out_at_its_first_call(
    prepared: RunningHecate, hermes: Hermes, tmp_path: Path
) -> None:
    """FR-007: the refusal is not saved, so the new start checks it again with Hécate."""
    identity = hermes.identity.identity
    assert identity is not None
    revoke_urgently(prepared, identity)
    await hermes.identity.refresh_publications()
    assert hermes.identity.problem() == "needs a new enrollment token"
    again = hermes_for(prepared.url, tmp_path)
    try:
        assert again.identity.problem() is None
        with pytest.raises(HermesError) as raised:
            await again.identity.resolve([SecretReference.parse(PASSWORD_REF)])
        assert raised.value.code is ErrorCode.AUTH
        assert raised.value.message != REFUSED
        assert again.identity.problem() == "needs a new enrollment token"
    finally:
        await again.client.aclose()


async def test_after_the_refusal_a_new_token_enrolls_the_same_hermes_on_the_same_directories(
    prepared: RunningHecate, hermes: Hermes
) -> None:
    """FR-006, story 2: nothing is emptied, and the calls go out again with the new certificate."""
    identity = hermes.identity.identity
    assert identity is not None
    revoke_urgently(prepared, identity)
    await hermes.identity.refresh_publications()
    assert hermes.identity.problem() == "needs a new enrollment token"
    issued = await hermes.identity.enroll(SecretValue(prepared.token()))
    assert issued.serial != identity.certificate.serial
    assert hermes.identity.problem() is None
    stored = hermes.store.load()
    assert stored is not None
    assert stored.certificate.serial == issued.serial
    [resolved] = await hermes.identity.resolve([SecretReference.parse(PASSWORD_REF)])
    assert resolved.value == SecretValue("test-db-password")
