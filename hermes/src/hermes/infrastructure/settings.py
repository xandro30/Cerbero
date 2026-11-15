"""The configuration by environment of contracts/cli.md (12-factor); no variable carries a secret."""

from __future__ import annotations

import re
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

from hermes.domain.shared.errors import HermesError
from hermes.domain.shared.references import SecretReference

DEFAULT_RUN_DIR = "/run/hermes"
DEFAULT_ADMIN_DIR = "/run/hermes-admin"
HERMES_IDENTITY = "hermes"
DEFAULT_PASSWORD_REF = "secret://hermes/postgres/password"  # noqa: S105  # a secret:// reference, not a value
LOG_LEVELS = ("debug", "info", "warning", "error")
MAX_SOCKET_PATH = 100
API_SOCKET = "api.sock"
ADMIN_SOCKET = "admin.sock"


class SettingsError(Exception):
    """A variable that does not fit; the CLI writes `error: <message>` and exits with 2."""


@dataclass(frozen=True)
class CliSettings:
    admin_dir: Path

    @property
    def admin_socket(self) -> Path:
        return self.admin_dir / ADMIN_SOCKET


@dataclass(frozen=True)
class ServeSettings:
    state_dir: Path
    proxy_dir: Path
    run_dir: Path
    admin_dir: Path
    hecate_url: str
    database_url: str
    database_password_ref: SecretReference
    verification_code_minutes: int
    fake_providers: bool
    fake_dir: Path | None
    log_level: str

    @property
    def api_socket(self) -> Path:
        return self.run_dir / API_SOCKET

    @property
    def admin_socket(self) -> Path:
        return self.admin_dir / ADMIN_SOCKET


def _required(env: Mapping[str, str], name: str) -> str:
    value = env.get(name, "")
    if not value:
        raise SettingsError(f"{name} is required")
    return value


def _directory(env: Mapping[str, str], name: str, default: str | None = None) -> Path:
    raw = env.get(name) or default
    if raw is None:
        raise SettingsError(f"{name} is required")
    path = Path(raw)
    if not path.is_absolute():
        raise SettingsError(f"{name} must be an absolute path")
    return path


def _socket_dir(env: Mapping[str, str], name: str, default: str, socket: str) -> Path:
    path = _directory(env, name, default)
    if len(str(path / socket).encode()) > MAX_SOCKET_PATH:
        raise SettingsError(f"{name} must be short: {socket} inside it must fit in {MAX_SOCKET_PATH} bytes")
    return path


def _hecate_url(env: Mapping[str, str]) -> str:
    raw = _required(env, "HERMES_HECATE_URL")
    parts = urlsplit(raw)
    try:
        port = parts.port
    except ValueError:
        port = None
    if parts.scheme != "https" or not parts.hostname or parts.path not in ("", "/") or parts.query or port is None:
        raise SettingsError("HERMES_HECATE_URL must be https://<host>:<port>")
    return f"https://{parts.netloc}"


def _database_url(env: Mapping[str, str]) -> str:
    raw = _required(env, "HERMES_DATABASE_URL")
    parts = urlsplit(raw)
    if parts.password is not None:
        raise SettingsError(
            "HERMES_DATABASE_URL must not carry a password: Hermes resolves it from HERMES_DATABASE_PASSWORD_REF"
        )
    database = parts.path.lstrip("/")
    if parts.scheme != "postgresql+psycopg" or not parts.username or not parts.hostname or not database:
        raise SettingsError("HERMES_DATABASE_URL must be postgresql+psycopg://<user>@<host>[:<port>]/<database>")
    return raw


def _password_ref(env: Mapping[str, str]) -> SecretReference:
    """The password of the database is a secret of Hermes in Hécate (FR-023, ADR 0012)."""
    try:
        reference = SecretReference.parse(env.get("HERMES_DATABASE_PASSWORD_REF") or DEFAULT_PASSWORD_REF)
    except HermesError as error:
        raise SettingsError("HERMES_DATABASE_PASSWORD_REF must be a secret:// reference") from error
    if reference.identity != HERMES_IDENTITY:
        raise SettingsError(f"HERMES_DATABASE_PASSWORD_REF must be a secret of hermes (secret://{HERMES_IDENTITY}/...)")
    return reference


def _code_minutes(env: Mapping[str, str]) -> int:
    raw = env.get("HERMES_VERIFICATION_CODE_MINUTES", "30")
    if re.fullmatch(r"[0-9]{1,4}", raw) is None or not 5 <= int(raw) <= 1440:
        raise SettingsError("HERMES_VERIFICATION_CODE_MINUTES must be a whole number of minutes from 5 to 1440")
    return int(raw)


def _fake_providers(env: Mapping[str, str]) -> bool:
    raw = env.get("HERMES_FAKE_PROVIDERS", "disabled")
    if raw not in ("disabled", "enabled"):
        raise SettingsError("HERMES_FAKE_PROVIDERS must be disabled or enabled")
    return raw == "enabled"


def _log_level(env: Mapping[str, str]) -> str:
    raw = env.get("HERMES_LOG_LEVEL", "info")
    if raw not in LOG_LEVELS:
        raise SettingsError("HERMES_LOG_LEVEL must be debug, info, warning or error")
    return raw


def load_serve_settings(env: Mapping[str, str]) -> ServeSettings:
    state_dir = _directory(env, "HERMES_STATE_DIR")
    proxy_dir = _directory(env, "HERMES_PROXY_DIR")
    hecate_url = _hecate_url(env)
    database_url = _database_url(env)
    return ServeSettings(
        state_dir=state_dir,
        proxy_dir=proxy_dir,
        run_dir=_socket_dir(env, "HERMES_RUN_DIR", DEFAULT_RUN_DIR, API_SOCKET),
        admin_dir=_socket_dir(env, "HERMES_ADMIN_DIR", DEFAULT_ADMIN_DIR, ADMIN_SOCKET),
        hecate_url=hecate_url,
        database_url=database_url,
        database_password_ref=_password_ref(env),
        verification_code_minutes=_code_minutes(env),
        fake_providers=_fake_providers(env),
        fake_dir=_directory(env, "HERMES_FAKE_DIR") if env.get("HERMES_FAKE_DIR") else None,
        log_level=_log_level(env),
    )


def load_cli_settings(env: Mapping[str, str]) -> CliSettings:
    return CliSettings(admin_dir=_socket_dir(env, "HERMES_ADMIN_DIR", DEFAULT_ADMIN_DIR, ADMIN_SOCKET))
