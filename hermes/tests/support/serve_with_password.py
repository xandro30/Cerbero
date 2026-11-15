"""Runs `hermes serve` with the database password read from a file instead of Hécate: the double of T016.

Usage: python -m support.serve_with_password <password file>, with the HERMES_* variables of `hermes serve`.
"""

import asyncio
import os
import sys
from pathlib import Path

from hermes.domain.shared.references import SecretReference
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.main import serve
from hermes.infrastructure.settings import load_serve_settings


class FileSecretSource:
    def __init__(self, path: Path) -> None:
        self._path = path

    async def resolve(self, reference: SecretReference) -> SecretValue:
        return SecretValue(self._path.read_text(encoding="utf-8").strip())


def main() -> None:
    settings = load_serve_settings(os.environ)
    asyncio.run(serve(settings, secrets=FileSecretSource(Path(sys.argv[1]))))


if __name__ == "__main__":
    main()
