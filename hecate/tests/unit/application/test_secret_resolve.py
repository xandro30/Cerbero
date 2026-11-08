import pytest

from hecate.domain.sealing.access_log import Method, Operation, Result
from hecate.domain.sealing.errors import InvalidInput, NotFound, StateConflict
from tests.fakes import SecretsRig

TOKEN = "secret://hermes/telegram/bot_token"
PROXY = "secret://hermes/certs/proxy"
OTHER = "secret://echidna/api_key"
TOKEN_VALUE = "hermes-token-value"
PROXY_VALUE = '{"cert": "value"}'
OTHER_VALUE = "echidna-key-value"
MISSING = "secret://hermes/does/not/exist"


@pytest.fixture
def rig():
    rig = SecretsRig()
    rig.service.create(TOKEN, TOKEN_VALUE, "string")
    rig.service.create(PROXY, PROXY_VALUE, "json")
    rig.service.create(OTHER, OTHER_VALUE, "string")
    rig.access_log.items.clear()
    return rig


def test_resolves_the_active_version_with_the_cache_times(rig):
    out = rig.service.resolve("hermes", [TOKEN])
    assert out == {"secrets": [{
        "path": TOKEN, "version": 1, "value": TOKEN_VALUE,
        "cache_ttl_soft_s": 300, "cache_ttl_hard_s": 3600,
    }]}


def test_resolves_a_concrete_version(rig):
    (item,) = rig.service.resolve("hermes", [TOKEN + "@1"])["secrets"]
    assert (item["path"], item["version"], item["value"]) == (TOKEN, 1, TOKEN_VALUE)


def test_a_version_that_does_not_exist_is_not_found(rig):
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", [TOKEN + "@2"])


def test_resolves_several_references_in_one_request_in_order(rig):
    out = rig.service.resolve("hermes", [PROXY, TOKEN, PROXY + "@1"])["secrets"]
    assert [(i["path"], i["version"], i["value"]) for i in out] == [
        (PROXY, 1, PROXY_VALUE), (TOKEN, 1, TOKEN_VALUE), (PROXY, 1, PROXY_VALUE)]


def test_all_or_nothing(rig):
    with pytest.raises(NotFound) as exc:
        rig.service.resolve("hermes", [TOKEN, MISSING])
    assert TOKEN_VALUE not in str(exc.value)


def test_another_identitys_secret_and_a_missing_one_are_indistinguishable(rig):
    with pytest.raises(NotFound) as foreign:
        rig.service.resolve("hermes", [OTHER])
    with pytest.raises(NotFound) as missing:
        rig.service.resolve("hermes", [MISSING])
    with pytest.raises(NotFound) as foreign_version:
        rig.service.resolve("hermes", [OTHER + "@1"])
    with pytest.raises(NotFound) as missing_version:
        rig.service.resolve("hermes", ["secret://hermes/does/not/exist@1"])
    messages = {str(e.value) for e in (foreign, missing, foreign_version, missing_version)}
    assert len(messages) == 1
    assert {type(e.value) for e in (foreign, missing, foreign_version, missing_version)} == {NotFound}
    assert all(e.value.code == "not_found" and not e.value.retryable
               for e in (foreign, missing, foreign_version, missing_version))
    assert not any(v in " ".join(messages) for v in (OTHER, MISSING, OTHER_VALUE))


def test_an_identity_never_resolves_outside_its_own_space(rig):
    rig.service.create("secret://hermes-eu/x", "eu-value", "string")
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", ["secret://hermes-eu/x"])
    with pytest.raises(NotFound):
        rig.service.resolve("hermes-eu", [TOKEN])
    assert rig.service.resolve("hermes-eu", ["secret://hermes-eu/x"])["secrets"][0]["value"] == "eu-value"


@pytest.mark.parametrize("references", [[], "secret://hermes/x", None, [5], ["not a reference"],
                                        [TOKEN + "@0"], [TOKEN] * 101])
def test_malformed_requests_are_invalid(rig, references):
    with pytest.raises(InvalidInput):
        rig.service.resolve("hermes", references)


@pytest.mark.parametrize("identity", ["", "Hermes", 5, None, "he/rmes"])
def test_a_malformed_identity_is_invalid(rig, identity):
    with pytest.raises(InvalidInput):
        rig.service.resolve(identity, [TOKEN])


def test_resolving_while_sealed_conflicts(rig):
    rig.sealing.seal()
    with pytest.raises(StateConflict):
        rig.service.resolve("hermes", [TOKEN])


def test_every_resolution_is_logged_with_the_identity_as_subject_and_no_value(rig):
    rig.service.resolve("hermes", [TOKEN, PROXY])
    first, second = rig.secret_rows()
    for row, path in ((first, TOKEN), (second, PROXY)):
        assert (row.operation, row.subject, row.method, row.result) == (
            Operation.SECRET_RESOLVE, "hermes", Method.IDENTITY, Result.SUCCESS)
        assert (row.path, row.version) == (path, 1)
    assert TOKEN_VALUE not in repr(rig.access_log.items) and PROXY_VALUE not in repr(rig.access_log.items)


def test_a_rejection_is_logged_with_the_requested_path_and_logs_no_success(rig):
    with pytest.raises(NotFound):
        rig.service.resolve("hermes", [TOKEN, OTHER])
    (row,) = rig.secret_rows()
    assert (row.operation, row.subject, row.result, row.path) == (
        Operation.SECRET_RESOLVE, "hermes", Result.NOT_FOUND, OTHER)


def test_rejections_are_logged_the_same_for_foreign_and_missing_secrets(rig):
    for path in (OTHER, MISSING):
        with pytest.raises(NotFound):
            rig.service.resolve("hermes", [path])
    foreign, missing = rig.secret_rows()
    assert (foreign.result, foreign.version) == (missing.result, missing.version)


def test_a_sealed_refusal_is_logged_too(rig):
    rig.sealing.seal()
    with pytest.raises(StateConflict):
        rig.service.resolve("hermes", [TOKEN])
    assert rig.secret_rows()[-1].result is Result.REJECTED_CONFLICT


def test_what_is_resolved_is_exactly_what_was_written(rig):
    rig.service.create("secret://hermes/unicode", "pässwörd €\n\ttabs", "string")
    (item,) = rig.service.resolve("hermes", ["secret://hermes/unicode"])["secrets"]
    assert item["value"] == "pässwörd €\n\ttabs"
