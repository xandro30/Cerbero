from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum


class Operation(StrEnum):
    INIT = "init"
    UNSEAL = "unseal"
    SEAL = "seal"
    PASSPHRASE_CHANGE = "passphrase_change"
    SHARES_REKEY = "shares_rekey"
    SECRET_CREATE = "secret_create"
    SECRET_ADD_VERSION = "secret_add_version"
    SECRET_LIST = "secret_list"
    SECRET_GET = "secret_get"
    SECRET_REVOKE = "secret_revoke"
    SECRET_RESTORE = "secret_restore"
    SECRET_PROMOTE = "secret_promote"
    SECRET_DESTROY = "secret_destroy"
    SECRET_RESOLVE = "secret_resolve"


class Method(StrEnum):
    PASSPHRASE = "passphrase"
    RECOVERY = "recovery"
    LOCAL = "local"
    IDENTITY = "identity"


class Result(StrEnum):
    SUCCESS = "success"
    FAILURE = "failure"
    REJECTED_RATE_LIMITED = "rejected_rate_limited"
    REJECTED_CONFLICT = "rejected_conflict"
    INVALID_INPUT = "invalid_input"
    NOT_FOUND = "not_found"


@dataclass(frozen=True)
class AccessLogEntry:
    """One access. `subject` is an identity or `admin`; never a value, a fingerprint or a key."""

    at: datetime
    operation: Operation
    method: Method
    result: Result
    detail: str = ""
    subject: str | None = None
    path: str | None = None
    version: int | None = None
