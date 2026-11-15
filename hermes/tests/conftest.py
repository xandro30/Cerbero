"""Fixtures shared by every suite of Hermes."""

import pytest

pytest_plugins = ("support.postgres", "support.hecate")


@pytest.fixture
def anyio_backend() -> str:
    return "asyncio"
