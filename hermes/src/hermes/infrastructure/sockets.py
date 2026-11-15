"""The two Unix sockets of Hermes, created with their permissions before uvicorn serves them (research R1, R15).

uvicorn gives 0666 to the sockets it creates, so Hermes binds its own and hands them over.
"""

from __future__ import annotations

import errno
import os
import socket
import stat
from pathlib import Path

SOCKET_MODE = 0o600
PRIVATE_DIR_MODE = 0o700


def prepare_private_dir(path: Path) -> None:
    """A directory that only the user of Hermes enters: the one of the local channel."""
    path.mkdir(parents=True, exist_ok=True, mode=PRIVATE_DIR_MODE)
    path.chmod(PRIVATE_DIR_MODE)


def bind_unix_socket(path: Path, mode: int = SOCKET_MODE) -> socket.socket:
    """A listening Unix socket in `path`, with `mode` from the start; a socket left by a stop is replaced."""
    if path.is_symlink() or path.exists():
        if not stat.S_ISSOCK(path.lstat().st_mode):
            raise FileExistsError(errno.EEXIST, "it exists and is not a socket", str(path))
        path.unlink()
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    previous = os.umask(0o777 & ~mode)
    try:
        sock.bind(str(path))
    except OSError:
        sock.close()
        raise
    finally:
        os.umask(previous)
    path.chmod(mode)
    sock.listen(128)
    return sock
