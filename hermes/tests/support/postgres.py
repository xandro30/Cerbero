"""A throwaway PostgreSQL for the tests (research R18).

It has the `hermes` database and user, as the shared server gives each app (ADR 0012), and a second database of another
app that `hermes` cannot reach.
"""

from __future__ import annotations

import os
import secrets
import shutil
import subprocess
from collections.abc import Iterator
from dataclasses import dataclass

import psycopg
import pytest
from psycopg import sql

IMAGE = "postgres:18.7-trixie"
OTHER_DATABASE = "otra_app"


@dataclass(frozen=True)
class Database:
    host: str
    port: int
    name: str
    user: str
    password: str
    admin_user: str
    admin_password: str

    def conninfo(self, *, dbname: str | None = None, user: str | None = None, password: str | None = None) -> str:
        return psycopg.conninfo.make_conninfo(
            host=self.host,
            port=self.port,
            dbname=dbname or self.name,
            user=user or self.user,
            password=password or self.password,
        )

    def admin_conninfo(self, *, dbname: str = "postgres") -> str:
        return self.conninfo(dbname=dbname, user=self.admin_user, password=self.admin_password)

    @property
    def url(self) -> str:
        """The SQLAlchemy URL without the password, as `HERMES_DATABASE_URL` gives it."""
        return f"postgresql+psycopg://{self.user}@{self.host}:{self.port}/{self.name}"


def use_docker_context() -> None:
    """Point testcontainers at the Docker of the current context (Colima on macOS) when DOCKER_HOST is not set."""
    if os.environ.get("DOCKER_HOST") or shutil.which("docker") is None:
        return
    endpoint = subprocess.run(
        ["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"],  # noqa: S607
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    if endpoint:
        os.environ["DOCKER_HOST"] = endpoint
        os.environ.setdefault("TESTCONTAINERS_DOCKER_SOCKET_OVERRIDE", "/var/run/docker.sock")


@pytest.fixture(scope="session")
def postgres() -> Iterator[Database]:
    use_docker_context()
    from testcontainers.community.postgres import PostgresContainer

    admin_password = secrets.token_urlsafe(24)
    password = secrets.token_urlsafe(24)
    with PostgresContainer(IMAGE, username="postgres", password=admin_password, dbname="postgres") as container:
        database = Database(
            host=container.get_container_host_ip(),
            port=int(container.get_exposed_port(5432)),
            name="hermes",
            user="hermes",
            password=password,
            admin_user="postgres",
            admin_password=admin_password,
        )
        with psycopg.connect(database.admin_conninfo(), autocommit=True) as connection:
            for template in ("postgres", "template1"):
                connection.execute(
                    sql.SQL("REVOKE CONNECT ON DATABASE {} FROM PUBLIC").format(sql.Identifier(template))
                )
            connection.execute(sql.SQL("CREATE ROLE hermes LOGIN PASSWORD {}").format(sql.Literal(password)))
            connection.execute("CREATE DATABASE hermes OWNER hermes")
            connection.execute("REVOKE CONNECT ON DATABASE hermes FROM PUBLIC")
            connection.execute("GRANT CONNECT ON DATABASE hermes TO hermes")
            connection.execute(sql.SQL("CREATE DATABASE {}").format(sql.Identifier(OTHER_DATABASE)))
            connection.execute(
                sql.SQL("REVOKE CONNECT ON DATABASE {} FROM PUBLIC").format(sql.Identifier(OTHER_DATABASE))
            )
        yield database
