from hecate.domain.sealing.state import SealState


class HecateError(Exception):
    """Base error. Messages never carry secrets (FR-014)."""

    code = "internal"
    retryable = False


class InvalidInput(HecateError):
    code = "validation"


class AuthenticationFailed(HecateError):
    code = "auth"


class RateLimited(HecateError):
    code = "rate_limited"
    retryable = True

    def __init__(self, retry_after_s: int) -> None:
        super().__init__(f"too many failed attempts; retry in {retry_after_s}s")
        self.retry_after_s = retry_after_s


class NotFound(HecateError):
    code = "not_found"


class StateConflict(HecateError):
    """The operation is not allowed now: Hécate is not in the right seal state, or the
    secret is not in a state that allows it (`current_version` marks a stale write)."""

    code = "conflict"

    def __init__(
        self,
        state: SealState | None = None,
        message: str | None = None,
        *,
        current_version: int | None = None,
    ) -> None:
        if message is None:
            message = f"operation not allowed while {state.value}" if state else "operation not allowed"
        super().__init__(message)
        self.state = state
        self.current_version = current_version
