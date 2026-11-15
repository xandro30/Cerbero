"""The throwaway PostgreSQL gives the hermes database and user (T006)."""

import psycopg
import pytest

from support.postgres import Database

pytestmark = pytest.mark.docker


def test_hermes_connects_to_its_own_database(postgres: Database) -> None:
    with psycopg.connect(postgres.conninfo()) as connection:
        row = connection.execute("SELECT current_user, current_database()").fetchone()
    assert row == ("hermes", "hermes")
    assert postgres.url == f"postgresql+psycopg://hermes@{postgres.host}:{postgres.port}/hermes"
