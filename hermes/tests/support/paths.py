"""Short temporary directories: a Unix socket path must fit in about 100 bytes."""

import shutil
import tempfile
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path


@contextmanager
def short_dir() -> Iterator[Path]:
    path = Path(tempfile.mkdtemp(prefix="h", dir="/tmp"))
    try:
        yield path
    finally:
        shutil.rmtree(path, ignore_errors=True)
