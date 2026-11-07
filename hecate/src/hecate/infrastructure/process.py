import ctypes
import logging
import resource
import sys

log = logging.getLogger("hecate.process")

_MCL_CURRENT, _MCL_FUTURE = 1, 2


def harden_process() -> None:
    """Best-effort protection of the root in memory: no core dumps and, on Linux, no swap."""
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    if not sys.platform.startswith("linux"):
        log.warning("memory locking is only available on Linux; the root may reach swap")
        return
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.mlockall(_MCL_CURRENT | _MCL_FUTURE) != 0:
        log.warning("mlockall failed (missing permission?); the root may reach swap")
