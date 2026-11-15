"""Each answer of the Bot API as a failure of Hermes (contracts/telegram.md#errores).

No message carries the URL, the token, the chat or the text that Telegram answered.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

from hermes.domain.services.chat.provider import ProviderFailure
from hermes.domain.shared.errors import ErrorCode

DEFAULT_RETRY_AFTER_S = 1


def outcome_unknown() -> ProviderFailure:
    """The request left Hermes and its answer was lost: the message may have arrived, so it is not retried."""
    return ProviderFailure(
        ErrorCode.TIMEOUT,
        "the answer of telegram was lost after sending: the message may have arrived",
        retryable=False,
        outcome_unknown=True,
    )


def retry_after(body: Any) -> int:
    parameters = body.get("parameters") if isinstance(body, Mapping) else None
    value = parameters.get("retry_after") if isinstance(parameters, Mapping) else None
    if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
        return value
    return DEFAULT_RETRY_AFTER_S


def unreachable_for_test() -> ProviderFailure:
    return ProviderFailure(ErrorCode.PROVIDER_UNAVAILABLE, "telegram did not answer the connection test")


def check_refusal(status: int, body: Any) -> ProviderFailure:
    """A failure of `getMe`: the credentials, a limit, or telegram failing; getMe changes nothing, so it is never an
    unknown outcome."""
    if status in (401, 404):
        return ProviderFailure(
            ErrorCode.PROVIDER_UNAVAILABLE,
            "telegram does not accept the credentials of the instance",
            retryable=False,
            credentials_rejected=True,
        )
    if status == 429:
        return ProviderFailure(ErrorCode.RATE_LIMITED, "telegram limits the bot", retry_after_s=retry_after(body))
    return ProviderFailure(
        ErrorCode.PROVIDER_UNAVAILABLE,
        "telegram failed the connection test",
        retryable=status >= 500 or status == 200,
    )


def refusal(status: int, body: Any) -> ProviderFailure:
    """A failure for an HTTP status that is not 200."""
    if status == 403:
        return ProviderFailure(ErrorCode.CONFLICT, "the person does not accept messages from the bot", retryable=False)
    if status in (401, 404):
        return ProviderFailure(
            ErrorCode.PROVIDER_UNAVAILABLE,
            "telegram does not accept the credentials of the instance",
            retryable=False,
            credentials_rejected=True,
        )
    if status == 429:
        return ProviderFailure(ErrorCode.RATE_LIMITED, "telegram limits the bot", retry_after_s=retry_after(body))
    if status >= 500:
        return outcome_unknown()
    return ProviderFailure(ErrorCode.PROVIDER_UNAVAILABLE, "telegram refused the request", retryable=False)
