"""uvicorn servers embedded in the process of Hermes: the API and the local channel stop together (research R1)."""

from __future__ import annotations

import contextlib
from collections.abc import Generator
from typing import Any

import uvicorn


class EmbeddedServer(uvicorn.Server):
    """A uvicorn server that leaves SIGINT and SIGTERM to Hermes, which stops both servers at once."""

    @contextlib.contextmanager
    def capture_signals(self) -> Generator[None]:
        yield


def server_config(app: Any) -> uvicorn.Config:
    """No proxy headers (the only one that counts is the certificate of Envoy), no access log and no lifespan."""
    return uvicorn.Config(
        app,
        lifespan="off",
        proxy_headers=False,
        access_log=False,
        log_config=None,
        server_header=False,
        date_header=False,
        timeout_graceful_shutdown=5,
    )
