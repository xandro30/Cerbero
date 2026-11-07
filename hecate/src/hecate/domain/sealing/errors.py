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


class StateConflict(HecateError):
    code = "conflict"

    def __init__(self, state: SealState) -> None:
        super().__init__(f"operation not allowed while {state.value}")
        self.state = state
