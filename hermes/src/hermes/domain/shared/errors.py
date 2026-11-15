"""The closed error codes of the common envelope and the error that carries one."""

from __future__ import annotations

from enum import StrEnum


class ErrorCode(StrEnum):
    VALIDATION = "validation"
    AUTH = "auth"
    NOT_FOUND = "not_found"
    CONFLICT = "conflict"
    RATE_LIMITED = "rate_limited"
    PROVIDER_UNAVAILABLE = "provider_unavailable"
    TIMEOUT = "timeout"
    UNAVAILABLE = "unavailable"
    INTERNAL = "internal"

    @property
    def retryable(self) -> bool:
        """Whether retrying is worth it unless the failure says otherwise."""
        return self in _RETRYABLE


_RETRYABLE = frozenset(
    {ErrorCode.RATE_LIMITED, ErrorCode.PROVIDER_UNAVAILABLE, ErrorCode.UNAVAILABLE, ErrorCode.INTERNAL}
)


class HermesError(Exception):
    """A failure that answers with one of the closed codes; its message never carries secrets."""

    def __init__(
        self,
        code: ErrorCode,
        message: str,
        *,
        retryable: bool | None = None,
        retry_after_s: int | None = None,
        outcome_unknown: bool | None = None,
        from_provider: bool = False,
        details: list[dict[str, str]] | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.retryable = code.retryable if retryable is None else retryable
        self.retry_after_s = retry_after_s
        self.outcome_unknown = outcome_unknown
        self.from_provider = from_provider
        self.details = details or []
