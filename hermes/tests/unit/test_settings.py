"""The configuration by environment (T009), with the variables of contracts/cli.md."""

from pathlib import Path

import pytest

from hermes.domain.shared.references import SecretReference
from hermes.infrastructure.admin import cli
from hermes.infrastructure.settings import SettingsError, load_cli_settings, load_serve_settings

BASE = {
    "HERMES_STATE_DIR": "/var/lib/hermes/state",
    "HERMES_PROXY_DIR": "/var/lib/hermes/proxy",
    "HERMES_HECATE_URL": "https://hecate:8443",
    "HERMES_DATABASE_URL": "postgresql+psycopg://hermes@postgres:5432/hermes",
}


def with_(**changes: str) -> dict[str, str]:
    return {**BASE, **changes}


def test_serve_takes_the_defaults_with_the_required_variables() -> None:
    settings = load_serve_settings(BASE)
    assert settings.state_dir == Path("/var/lib/hermes/state")
    assert settings.proxy_dir == Path("/var/lib/hermes/proxy")
    assert settings.run_dir == Path("/run/hermes")
    assert settings.admin_dir == Path("/run/hermes-admin")
    assert settings.api_socket == Path("/run/hermes/api.sock")
    assert settings.admin_socket == Path("/run/hermes-admin/admin.sock")
    assert settings.hecate_url == "https://hecate:8443"
    assert settings.database_url == "postgresql+psycopg://hermes@postgres:5432/hermes"
    assert settings.database_password_ref == SecretReference.parse("secret://hermes/postgres/password")
    assert settings.verification_code_minutes == 30
    assert settings.fake_providers is False
    assert settings.fake_dir is None
    assert settings.log_level == "info"


@pytest.mark.parametrize("name", ["HERMES_STATE_DIR", "HERMES_PROXY_DIR", "HERMES_HECATE_URL", "HERMES_DATABASE_URL"])
def test_serve_needs_each_required_variable(name: str) -> None:
    env = dict(BASE)
    del env[name]
    with pytest.raises(SettingsError, match=name):
        load_serve_settings(env)


@pytest.mark.parametrize(("value", "minutes"), [("5", 5), ("30", 30), ("1440", 1440)])
def test_the_code_lasts_from_5_to_1440_minutes(value: str, minutes: int) -> None:
    assert load_serve_settings(with_(HERMES_VERIFICATION_CODE_MINUTES=value)).verification_code_minutes == minutes


@pytest.mark.parametrize("value", ["4", "1441", "0", "-1", "30.5", "abc", "", " 30"])
def test_any_other_code_duration_is_refused(value: str) -> None:
    with pytest.raises(SettingsError, match="HERMES_VERIFICATION_CODE_MINUTES must be a whole number of minutes"):
        load_serve_settings(with_(HERMES_VERIFICATION_CODE_MINUTES=value))


def test_the_fake_providers_are_disabled_or_enabled() -> None:
    assert load_serve_settings(with_(HERMES_FAKE_PROVIDERS="enabled")).fake_providers is True
    assert load_serve_settings(with_(HERMES_FAKE_PROVIDERS="disabled")).fake_providers is False
    for value in ("yes", "true", "", "Enabled"):
        with pytest.raises(SettingsError, match="HERMES_FAKE_PROVIDERS must be disabled or enabled"):
            load_serve_settings(with_(HERMES_FAKE_PROVIDERS=value))


def test_the_fake_dir_is_an_absolute_path() -> None:
    assert load_serve_settings(with_(HERMES_FAKE_DIR="/srv/hermes-fake")).fake_dir == Path("/srv/hermes-fake")
    with pytest.raises(SettingsError, match="HERMES_FAKE_DIR must be an absolute path"):
        load_serve_settings(with_(HERMES_FAKE_DIR="fake"))


@pytest.mark.parametrize("level", ["debug", "info", "warning", "error"])
def test_the_log_level_is_one_of_four(level: str) -> None:
    assert load_serve_settings(with_(HERMES_LOG_LEVEL=level)).log_level == level


def test_any_other_log_level_is_refused() -> None:
    with pytest.raises(SettingsError, match="HERMES_LOG_LEVEL must be debug, info, warning or error"):
        load_serve_settings(with_(HERMES_LOG_LEVEL="trace"))


def test_a_database_url_with_a_password_is_refused_without_showing_it() -> None:
    with pytest.raises(SettingsError, match="must not carry a password") as raised:
        load_serve_settings(with_(HERMES_DATABASE_URL="postgresql+psycopg://hermes:s3cr3t@postgres:5432/hermes"))
    assert "s3cr3t" not in str(raised.value)


@pytest.mark.parametrize(
    "value",
    [
        "postgres://hermes@postgres:5432/hermes",
        "postgresql+psycopg://postgres:5432/hermes",
        "postgresql+psycopg://hermes@postgres:5432",
        "postgresql+psycopg://hermes@:5432/hermes",
    ],
)
def test_a_database_url_without_its_parts_is_refused(value: str) -> None:
    with pytest.raises(SettingsError, match="HERMES_DATABASE_URL must be"):
        load_serve_settings(with_(HERMES_DATABASE_URL=value))


@pytest.mark.parametrize("value", ["http://hecate:8443", "https://hecate:8443/v1", "hecate:8443", "https://"])
def test_the_hecate_url_is_https_with_host_and_port(value: str) -> None:
    with pytest.raises(SettingsError, match="HERMES_HECATE_URL must be"):
        load_serve_settings(with_(HERMES_HECATE_URL=value))


def test_the_password_reference_is_a_secret_reference() -> None:
    with pytest.raises(SettingsError, match="HERMES_DATABASE_PASSWORD_REF must be a secret:// reference"):
        load_serve_settings(with_(HERMES_DATABASE_PASSWORD_REF="plain-password"))


def test_the_password_reference_is_a_secret_of_hermes() -> None:
    """The password of the database is a secret of Hermes in Hécate (FR-023, ADR 0012), never one of another app."""
    settings = load_serve_settings(with_(HERMES_DATABASE_PASSWORD_REF="secret://hermes/db/main@2"))
    assert settings.database_password_ref == SecretReference.parse("secret://hermes/db/main@2")
    with pytest.raises(
        SettingsError, match=r"HERMES_DATABASE_PASSWORD_REF must be a secret of hermes \(secret://hermes/"
    ):
        load_serve_settings(with_(HERMES_DATABASE_PASSWORD_REF="secret://hades/postgres/password"))


def test_the_directories_are_absolute_and_the_sockets_short() -> None:
    with pytest.raises(SettingsError, match="HERMES_STATE_DIR must be an absolute path"):
        load_serve_settings(with_(HERMES_STATE_DIR="state"))
    with pytest.raises(SettingsError, match="HERMES_RUN_DIR"):
        load_serve_settings(with_(HERMES_RUN_DIR="/" + "x" * 120))


def test_the_other_orders_only_need_the_admin_dir() -> None:
    assert load_cli_settings({}).admin_socket == Path("/run/hermes-admin/admin.sock")
    assert load_cli_settings({"HERMES_ADMIN_DIR": "/srv/admin"}).admin_socket == Path("/srv/admin/admin.sock")
    with pytest.raises(SettingsError, match="HERMES_ADMIN_DIR must be an absolute path"):
        load_cli_settings({"HERMES_ADMIN_DIR": "admin"})


def test_a_value_that_does_not_fit_gives_error_and_exit_2(capsys: pytest.CaptureFixture[str]) -> None:
    assert cli.main(["serve"], env=with_(HERMES_LOG_LEVEL="trace")) == 2
    assert capsys.readouterr().err == "error: HERMES_LOG_LEVEL must be debug, info, warning or error\n"
