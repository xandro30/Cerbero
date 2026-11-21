"""The authentication of each call (T044, FR-003, FR-004, research R3), with the test CA of T006; and the access list
of Hermes after the token (T024 of 009)."""

import logging
from datetime import datetime, timedelta
from typing import Any

import jwt
import pytest
from cryptography import x509

from hermes.application.authentication import AUTH_MESSAGE, CallAuthenticator
from hermes.application.ports import CallCredentials, PresentedCertificate
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SpiffeId
from hermes.infrastructure.api.xfcc import parse_xfcc
from support.clock import FakeClock
from support.doubles import MemoryUsedTokens, StaticTrust
from support.pki import CallKey, Leaf, TestCa, spiffe
from support.xfcc import HERMES, xfcc

pytestmark = pytest.mark.anyio


class World:
    def __init__(self) -> None:
        self.clock = FakeClock()
        self.ca = TestCa(now=self.clock.now())
        self.leaf = self.ca.issue("hades")
        self.key = self.ca.call_key("hades")
        self.revoked: list[int] = []
        self.keys: list[dict[str, Any]] = [self.key.jwk()]
        self.tokens = MemoryUsedTokens()
        self.root: x509.Certificate | None = self.ca.root
        self.later: dict[str, dict[str, Any]] = {}
        self.asked: list[str] = []
        self.down = False
        self.callers: list[str] | None = ["hades"]
        self.granted: list[str] | None = None

    def trust(self) -> StaticTrust:
        return StaticTrust(
            self.root,
            HERMES,
            [self.ca.crl(self.revoked)],
            self.keys,
            callers=self.callers,
            granted=self.granted,
            later=self.later,
            asked=self.asked,
            down=self.down,
        )

    def authenticator_on(self, trust: StaticTrust) -> CallAuthenticator:
        return CallAuthenticator(trust=trust, used_tokens=self.tokens, clock=self.clock)

    @property
    def authenticator(self) -> CallAuthenticator:
        return self.authenticator_on(self.trust())

    def token(self, key: CallKey | None = None, **overrides: Any) -> str:
        fields: dict[str, Any] = {"issuer": spiffe("hades"), "audience": HERMES, "issued_at": self.clock.now()}
        fields.update(overrides)
        return (key or self.key).token(**fields)

    def presented(
        self, leaf: Leaf | None = None, ca: TestCa | None = None, **xfcc_overrides: Any
    ) -> PresentedCertificate:
        issuer = ca or self.ca
        return parse_xfcc(xfcc((leaf or self.leaf).certificate, [issuer.intermediate], **xfcc_overrides))

    def credentials(
        self, presented: PresentedCertificate | None = None, token: str | None = None, *, bearer: bool = True
    ) -> CallCredentials:
        value = token if token is not None else self.token()
        return CallCredentials(
            certificate=presented if presented is not None else self.presented(),
            authorization=f"Bearer {value}" if bearer else value,
        )


async def refused(world: World, credentials: CallCredentials) -> HermesError:
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(credentials)
    assert raised.value.code is ErrorCode.AUTH
    assert raised.value.message == AUTH_MESSAGE
    return raised.value


async def test_a_valid_call_gives_the_caller_with_the_identity_of_its_certificate() -> None:
    world = World()
    caller = await world.authenticator.authenticate(world.credentials())
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))
    assert caller.audit_context is None


async def test_a_certificate_that_has_expired_is_refused() -> None:
    world = World()
    old = world.ca.issue("hades", not_before=world.clock.now() - timedelta(days=4), lifetime=timedelta(days=3))
    await refused(world, world.credentials(world.presented(old)))


async def test_a_certificate_of_another_ca_is_refused() -> None:
    world = World()
    other = TestCa(now=world.clock.now())
    await refused(world, world.credentials(world.presented(other.issue("hades"), other)))


async def test_a_certificate_whose_serial_is_in_the_crl_is_refused() -> None:
    world = World()
    world.revoked = [world.leaf.serial]
    await refused(world, world.credentials())


async def test_without_a_crl_of_its_intermediate_the_certificate_is_refused() -> None:
    world = World()
    trust = StaticTrust(world.ca.root, HERMES, [], world.keys, callers=["hades"])
    with pytest.raises(HermesError) as raised:
        await CallAuthenticator(trust=trust, used_tokens=world.tokens, clock=world.clock).authenticate(
            world.credentials()
        )
    assert raised.value.code is ErrorCode.AUTH


@pytest.mark.parametrize(
    "names",
    [
        [x509.UniformResourceIdentifier(spiffe("hades")), x509.UniformResourceIdentifier(spiffe("otro"))],
        [x509.UniformResourceIdentifier(spiffe("hades")), x509.DNSName("hades.local")],
        [x509.DNSName("hades.local")],
        [x509.UniformResourceIdentifier("https://hades.local")],
        [x509.UniformResourceIdentifier(spiffe("hades", "otro.test"))],
    ],
)
async def test_a_certificate_whose_only_name_is_not_a_spiffe_id_of_the_trust_domain_is_refused(
    names: list[x509.GeneralName],
) -> None:
    world = World()
    leaf = world.ca.issue("hades", names=names)
    uri = next((str(name.value) for name in names if isinstance(name, x509.UniformResourceIdentifier)), "none")
    await refused(world, world.credentials(world.presented(leaf, uri=uri)))


async def test_a_header_whose_uri_is_not_the_name_of_the_certificate_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(world.presented(uri=spiffe("admin"))))


async def test_a_header_written_for_another_service_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(world.presented(by=spiffe("hecate"))))


async def test_without_the_header_of_the_proxy_the_call_is_refused() -> None:
    world = World()
    await refused(world, CallCredentials(certificate=None, authorization=f"Bearer {world.token()}"))


@pytest.mark.parametrize("authorization", [None, "", "Bearer", "Bearer ", "Basic abc", "bearer x.y.z"])
async def test_without_a_bearer_token_the_call_is_refused(authorization: str | None) -> None:
    world = World()
    await refused(world, CallCredentials(certificate=world.presented(), authorization=authorization))


async def test_an_expired_token_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(issued_at=world.clock.now() - timedelta(seconds=400))))


async def test_a_token_that_lives_more_than_300_seconds_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(lifetime=timedelta(seconds=301))))


async def test_a_token_that_lives_300_seconds_is_accepted() -> None:
    world = World()
    caller = await world.authenticator.authenticate(
        world.credentials(token=world.token(lifetime=timedelta(seconds=300)))
    )
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))


@pytest.mark.parametrize(("offset", "accepted"), [(30, True), (31, False)])
async def test_a_token_from_the_future_has_30_seconds_of_margin(offset: int, accepted: bool) -> None:
    world = World()
    credentials = world.credentials(token=world.token(issued_at=world.clock.now() + timedelta(seconds=offset)))
    if accepted:
        await world.authenticator.authenticate(credentials)
    else:
        await refused(world, credentials)


@pytest.mark.parametrize(("ago", "accepted"), [(329, True), (330, False)])
async def test_an_expired_token_has_30_seconds_of_margin(ago: int, accepted: bool) -> None:
    world = World()
    credentials = world.credentials(token=world.token(issued_at=world.clock.now() - timedelta(seconds=ago)))
    if accepted:
        await world.authenticator.authenticate(credentials)
    else:
        await refused(world, credentials)


async def test_a_token_for_another_audience_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(audience=spiffe("hecate"))))


async def test_a_token_whose_issuer_is_not_the_certificate_is_refused() -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(issuer=spiffe("otro"))))


async def test_a_token_signed_with_a_key_that_hecate_does_not_publish_is_refused() -> None:
    world = World()
    unknown = world.ca.call_key("hades")
    await refused(world, world.credentials(token=world.token(key=unknown)))
    assert world.asked == [unknown.kid]


async def test_a_key_that_the_list_does_not_have_yet_is_asked_to_hecate_and_accepted() -> None:
    """A component that has just enrolled or renewed signs with a key that the last list of Hécate does not have."""
    world = World()
    renewed = world.ca.call_key("hades")
    world.later[renewed.kid] = renewed.jwk()
    caller = await world.authenticator.authenticate(world.credentials(token=world.token(key=renewed)))
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))
    assert world.asked == [renewed.kid]


async def test_a_key_in_the_list_is_not_asked_to_hecate() -> None:
    world = World()
    await world.authenticator.authenticate(world.credentials())
    assert world.asked == []


async def test_a_kid_without_the_form_of_a_thumbprint_is_refused_without_asking_hecate() -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(kid="../secrets:resolve")))
    assert world.asked == []


async def test_a_token_without_kid_in_its_header_is_refused_without_asking_hecate(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """What hermes.v1.json 1.0.2 says: the header of the call token carries the kid (FR-007 of 012)."""
    world = World()
    caplog.set_level(logging.INFO, logger="hermes")
    now = world.clock.now()
    claims = {
        "iss": spiffe("hades"),
        "aud": HERMES,
        "iat": int(now.timestamp()),
        "exp": int((now + timedelta(seconds=300)).timestamp()),
        "jti": "a-token-without-kid",
    }
    token = jwt.encode(claims, world.key.private, algorithm="EdDSA", headers={"typ": "JWT"})
    assert "kid" not in jwt.get_unverified_header(token)
    await refused(world, world.credentials(token=token))
    assert world.asked == []
    assert reasons(caplog) == ["unknown_key"]


async def test_the_key_is_asked_only_after_the_chain_is_valid() -> None:
    world = World()
    other = TestCa(now=world.clock.now())
    renewed = world.ca.call_key("hades")
    world.later[renewed.kid] = renewed.jwk()
    presented = world.presented(other.issue("hades"), other)
    await refused(world, world.credentials(presented, token=world.token(key=renewed)))
    assert world.asked == []


@pytest.mark.parametrize("change", ["identity", "status", "kid"])
async def test_a_key_from_hecate_that_is_not_active_of_the_identity_and_of_the_kid_is_refused(change: str) -> None:
    world = World()
    renewed = world.ca.call_key("hades")
    jwk = renewed.jwk(status="verify_only") if change == "status" else renewed.jwk()
    if change == "identity":
        jwk["cerbero_identity"] = spiffe("otro")
    if change == "kid":
        jwk["kid"] = "x" * 43
    world.later[renewed.kid] = jwk
    await refused(world, world.credentials(token=world.token(key=renewed)))


async def test_when_hecate_does_not_answer_about_a_key_the_call_is_unavailable() -> None:
    world = World()
    world.down = True
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(world.credentials(token=world.token(key=world.ca.call_key("hades"))))
    assert raised.value.code is ErrorCode.UNAVAILABLE


async def test_a_token_with_the_key_of_another_identity_is_refused() -> None:
    world = World()
    stranger = world.ca.call_key("otro")
    world.keys.append(stranger.jwk())
    await refused(world, world.credentials(token=world.token(key=stranger)))


async def test_a_token_whose_key_is_not_active_is_refused() -> None:
    world = World()
    world.keys = [world.key.jwk(status="verify_only")]
    await refused(world, world.credentials())


async def test_a_token_whose_signature_does_not_match_its_key_is_refused() -> None:
    world = World()
    forger = world.ca.call_key("hades")
    await refused(world, world.credentials(token=world.token(key=forger, kid=world.key.kid)))


async def test_a_token_without_signature_or_with_another_algorithm_is_refused() -> None:
    world = World()
    claims = {"iss": spiffe("hades"), "aud": HERMES, "iat": 0, "exp": 300, "jti": "j"}
    for token in (
        jwt.encode(claims, key="", algorithm="none", headers={"kid": world.key.kid}),
        jwt.encode(claims, key="a-shared-secret-of-32-bytes-long!", algorithm="HS256", headers={"kid": world.key.kid}),
        "not.a.token",
    ):
        await refused(world, world.credentials(token=token))


async def test_a_jti_is_accepted_only_once() -> None:
    world = World()
    token = world.token(jti="call-1")
    await world.authenticator.authenticate(world.credentials(token=token))
    await refused(world, world.credentials(token=token))
    assert list(world.tokens.rows) == [(spiffe("hades"), "call-1")]
    assert world.tokens.rows[(spiffe("hades"), "call-1")] == world.clock.now() + timedelta(seconds=330)


@pytest.mark.parametrize("jti", ["", "x" * 129, "ñandú", "tab\tjti"])
async def test_a_jti_must_have_from_1_to_128_printable_ascii_characters(jti: str) -> None:
    world = World()
    await refused(world, world.credentials(token=world.token(jti=jti)))


class DownUsedTokens:
    """The repository of the `jti` with the database down."""

    async def remember(self, issuer: str, jti: str, expires_at: datetime) -> bool:
        raise HermesError(ErrorCode.UNAVAILABLE, "the database does not answer")

    async def purge(self, now: datetime) -> int:
        raise HermesError(ErrorCode.UNAVAILABLE, "the database does not answer")


async def test_with_the_database_down_a_valid_call_is_unavailable_and_an_invalid_one_auth() -> None:
    """The certificate and the token go before the `jti`, which needs the database (T133, contracts/api.md)."""
    world = World()
    world.tokens = DownUsedTokens()  # type: ignore[assignment]
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(world.credentials())
    assert raised.value.code is ErrorCode.UNAVAILABLE
    await refused(world, world.credentials(token=world.token(audience=spiffe("hecate"))))
    other = TestCa(now=world.clock.now())
    await refused(world, world.credentials(world.presented(other.issue("hades"), other)))


async def test_every_refusal_has_the_same_message_and_says_why_only_in_the_log(
    caplog: pytest.LogCaptureFixture,
) -> None:
    world = World()
    caplog.set_level(logging.INFO, logger="hermes")
    token = world.token(audience=spiffe("hecate"))
    error = await refused(world, world.credentials(token=token))
    assert error.message == AUTH_MESSAGE
    assert "auth.refused" in caplog.text
    assert token not in caplog.text
    assert "BEGIN CERTIFICATE" not in caplog.text


async def test_no_permission_comes_from_the_token() -> None:
    world = World()
    token = world.token(extra={"cerbero_permissions": ["admin"], "scope": "everything", "roles": ["root"]})
    caller = await world.authenticator.authenticate(world.credentials(token=token))
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))
    assert caller.audit_context is None
    assert set(vars(caller)) == {"spiffe_id", "audit_context"}


async def test_without_its_trust_hermes_is_not_ready() -> None:
    world = World()
    world.root = None
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(world.credentials())
    assert raised.value.code is ErrorCode.UNAVAILABLE


async def test_the_audit_context_of_the_token_reaches_the_caller() -> None:
    world = World()
    context = {"source": "hades", "response": "r-1", "playbook": "pb-device"}
    caller = await world.authenticator.authenticate(
        world.credentials(token=world.token(extra={"cerbero_audit": context}))
    )
    assert caller.audit_context == context


def sixteen_keys() -> dict[str, str]:
    return {f"k{index:02d}": "v" * 140 for index in range(16)}


@pytest.mark.parametrize(
    "context",
    [
        {f"k{index:02d}": "v" for index in range(16)},
        {"k" * 64: "v" * 256},
        {"k": ""},
    ],
)
async def test_an_audit_context_within_its_limits_is_accepted(context: dict[str, str]) -> None:
    world = World()
    caller = await world.authenticator.authenticate(
        world.credentials(token=world.token(extra={"cerbero_audit": context}))
    )
    assert caller.audit_context == context


@pytest.mark.parametrize(
    "context",
    [
        ["source", "hades"],
        "hades",
        {f"k{index:02d}": "v" for index in range(17)},
        {"": "v"},
        {"k" * 65: "v"},
        {"k": 1},
        {"k": None},
        {"k": "v" * 257},
        sixteen_keys(),
    ],
)
async def test_an_audit_context_of_another_shape_is_validation(context: object) -> None:
    world = World()
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(world.credentials(token=world.token(extra={"cerbero_audit": context})))
    assert raised.value.code is ErrorCode.VALIDATION


# --- The access list of Hermes ----------------------------------------------------------------------------------


def reasons(caplog: pytest.LogCaptureFixture) -> list[str]:
    return [getattr(record, "fields", {}).get("reason") for record in caplog.records if record.msg == "auth.refused"]


async def test_a_caller_in_the_copy_of_the_access_list_enters_without_asking_hecate() -> None:
    world = World()
    trust = world.trust()
    caller = await world.authenticator_on(trust).authenticate(world.credentials())
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))
    assert trust.access_lookups == 0


async def test_a_caller_outside_the_list_is_refused_with_the_same_message_after_asking_again(
    caplog: pytest.LogCaptureFixture,
) -> None:
    world = World()
    world.callers = ["caller"]
    trust = world.trust()
    caplog.set_level(logging.INFO, logger="hermes")
    with pytest.raises(HermesError) as raised:
        await world.authenticator_on(trust).authenticate(world.credentials())
    assert (raised.value.code, raised.value.message) == (ErrorCode.AUTH, AUTH_MESSAGE)
    assert trust.access_lookups == 1
    assert reasons(caplog) == ["not_allowed"]


@pytest.mark.parametrize("identity", ["hades", "estigia"])
async def test_with_the_copy_empty_at_version_0_any_authenticated_identity_is_refused(identity: str) -> None:
    """Story 3, scenario 6: nothing granted, also never, lets no one in."""
    world = World()
    world.callers = []
    leaf, key = world.ca.issue(identity), world.ca.call_key(identity)
    world.keys.append(key.jwk())
    trust = world.trust()
    assert trust.access is not None
    assert (trust.access.callers, trust.access.version) == ((), 0)
    token = key.token(issuer=spiffe(identity), audience=HERMES, issued_at=world.clock.now())
    with pytest.raises(HermesError) as raised:
        await world.authenticator_on(trust).authenticate(world.credentials(world.presented(leaf), token=token))
    assert (raised.value.code, raised.value.message) == (ErrorCode.AUTH, AUTH_MESSAGE)


async def test_a_grant_counts_in_the_next_call_because_the_lookup_finds_it() -> None:
    world = World()
    world.callers = []
    world.granted = ["hades"]
    trust = world.trust()
    caller = await world.authenticator_on(trust).authenticate(world.credentials())
    assert caller.spiffe_id == SpiffeId.parse(spiffe("hades"))
    assert trust.access_lookups == 1
    assert trust.access is not None
    assert trust.access.callers == ("hades",)


async def test_when_hecate_does_not_answer_the_lookup_the_call_is_unavailable() -> None:
    world = World()
    world.callers = []
    world.down = True
    with pytest.raises(HermesError) as raised:
        await world.authenticator.authenticate(world.credentials())
    assert raised.value.code is ErrorCode.UNAVAILABLE
    assert world.tokens.rows == {}


async def test_what_the_token_says_about_permissions_roles_or_access_does_not_count() -> None:
    """FR-007: only the identity of the certificate, in the list, lets a caller in."""
    world = World()
    world.callers = ["caller"]
    claims = {
        "cerbero_access": ["hermes"],
        "cerbero_permissions": ["chat.send_message"],
        "roles": ["admin"],
        "scope": "hermes",
    }
    await refused(world, world.credentials(token=world.token(extra=claims)))


async def test_a_caller_without_access_leaves_no_jti() -> None:
    world = World()
    world.callers = []
    await refused(world, world.credentials(token=world.token(jti="without-access")))
    assert world.tokens.rows == {}


async def test_the_access_is_checked_only_after_the_certificate_and_the_token() -> None:
    world = World()
    world.callers = []
    trust = world.trust()
    other = TestCa(now=world.clock.now())
    for credentials in (
        world.credentials(world.presented(other.issue("hades"), other)),
        world.credentials(token=world.token(audience=spiffe("hecate"))),
    ):
        with pytest.raises(HermesError):
            await world.authenticator_on(trust).authenticate(credentials)
    assert trust.access_lookups == 0


async def test_without_a_copy_of_the_access_list_hermes_has_nothing_to_authenticate_with() -> None:
    world = World()
    world.callers = None
    trust = world.trust()
    with pytest.raises(HermesError) as raised:
        await world.authenticator_on(trust).authenticate(world.credentials())
    assert raised.value.code is ErrorCode.UNAVAILABLE
    assert trust.access_lookups == 0
