from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum


class Operation(StrEnum):
    INIT = "init"
    UNSEAL = "unseal"
    SEAL = "seal"
    PASSPHRASE_CHANGE = "passphrase_change"
    SHARES_REKEY = "shares_rekey"


class Method(StrEnum):
    PASSPHRASE = "passphrase"
    RECOVERY = "recovery"
    LOCAL = "local"


class Result(StrEnum):
    SUCCESS = "success"
    FAILURE = "failure"
    REJECTED_RATE_LIMITED = "rejected_rate_limited"
    REJECTED_CONFLICT = "rejected_conflict"
    INVALID_INPUT = "invalid_input"


@dataclass(frozen=True)
class AccessLogEntry:
    at: datetime
    operation: Operation
    method: Method
    result: Result
    detail: str = ""
