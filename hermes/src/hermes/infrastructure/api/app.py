"""The factory of the FastAPI apps of Hermes: the network API and the local channel share the envelope."""

from __future__ import annotations

from collections.abc import Sequence

from fastapi import APIRouter, FastAPI

from hermes.infrastructure.api.errors import install_handlers
from hermes.infrastructure.api.middleware import MAX_BODY, RequestContextMiddleware


def create_api(*, routers: Sequence[APIRouter] = (), max_body: int = MAX_BODY, log_requests: bool = False) -> FastAPI:
    """An app without documentation routes: every route it serves is one of the contract. With `log_requests`, each
    request leaves its line of log when it ends."""
    app = FastAPI(openapi_url=None, docs_url=None, redoc_url=None, swagger_ui_oauth2_redirect_url=None)
    install_handlers(app)
    app.add_middleware(RequestContextMiddleware, max_body=max_body, log_requests=log_requests)
    for router in routers:
        app.include_router(router)
    return app


def create_network_api() -> FastAPI:
    """The API that Envoy serves (hermes.v1.json): each of its routes depends on the authentication of the call, and
    each request leaves its line of log (research R19)."""
    from hermes.infrastructure.api.routes_capabilities import router as capabilities_router
    from hermes.infrastructure.api.routes_chat import router as chat_router

    return create_api(routers=[chat_router, capabilities_router], log_requests=True)
