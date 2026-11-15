"""The authentication dependency of every route of the network API (research R3).

First whether Hermes is ready, so a Hermes that is not answers `unavailable` before anything else; then the header of
the proxy and the short token. Until `install_authentication` gives it its authenticator, every route is `unavailable`.
"""

from __future__ import annotations

import logging
from collections.abc import Callable
from dataclasses import dataclass

from fastapi import Request

from hermes.application.authentication import AUTH_MESSAGE
from hermes.application.ports import Authenticator, CallCredentials, Caller, PresentedCertificate
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.infrastructure.api.xfcc import XfccError, parse_xfcc

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class Authentication:
    authenticator: Authenticator
    ready: Callable[[], bool]


def install_authentication(app: object, authentication: Authentication) -> None:
    app.state.authentication = authentication  # type: ignore[attr-defined]


def _presented(request: Request) -> PresentedCertificate | None:
    header = request.headers.get("x-forwarded-client-cert")
    if header is None:
        return None
    try:
        return parse_xfcc(header)
    except XfccError as error:
        logger.info("auth.refused", extra={"fields": {"reason": f"xfcc: {error}"}})
        raise HermesError(ErrorCode.AUTH, AUTH_MESSAGE) from None


async def authenticated(request: Request) -> Caller:
    """The caller, which the line of the request names once the call is authenticated."""
    gate = getattr(request.app.state, "authentication", None)
    if not isinstance(gate, Authentication) or not gate.ready():
        raise HermesError(ErrorCode.UNAVAILABLE, "hermes is not ready")
    credentials = CallCredentials(certificate=_presented(request), authorization=request.headers.get("authorization"))
    caller = await gate.authenticator.authenticate(credentials)
    request.scope.setdefault("state", {})["caller"] = str(caller.spiffe_id)
    return caller
