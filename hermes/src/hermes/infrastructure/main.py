"""`hermes serve`: one asyncio process with the API, the local channel and the background tasks (research R1).

The two sockets open first, so the local channel answers from the start, also its health. Then the start follows its
steps: identity, publications of Hécate, password of the database, connection, migrations, configuration, test of each
instance and polling. Once the database has its tables, the API can send and the local channel can configure. In the
background, the renewal of the certificate, the publications every 240 s, the password of the database every 240 s and
after each enrollment, the purge of the `jti` and of the keys of idempotency and, once ready, one polling per instance.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
import signal
import socket
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import TextIO

import httpx
from fastapi import FastAPI
from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncEngine

from hermes.application.authentication import CallAuthenticator
from hermes.application.capabilities import Capabilities, InstanceStatuses
from hermes.application.configuration import ConfigurationAdmin, ConfigurationState, HecateSecretChecker
from hermes.application.health import HealthCheck
from hermes.application.idempotency import Idempotency
from hermes.application.identity import PUBLICATIONS_INTERVAL_S, IdentityService
from hermes.application.ports import SecretSource
from hermes.application.secrets import SecretCache
from hermes.application.send_message import SendMessage
from hermes.application.startup import NeedsEnrollment, Startup, startup_steps
from hermes.application.verification import Verification
from hermes.domain.configuration.model import Configuration
from hermes.domain.configuration.validation import problems_error, validate
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.admin.app import AdminServices, create_admin_app, install_admin
from hermes.infrastructure.api.app import create_network_api
from hermes.infrastructure.api.auth import Authentication, install_authentication
from hermes.infrastructure.api.routes_capabilities import CapabilitiesServices, install_capabilities
from hermes.infrastructure.api.routes_chat import ChatServices, install_chat
from hermes.infrastructure.background import every
from hermes.infrastructure.clock import SystemClock
from hermes.infrastructure.config_file.export import write_configuration
from hermes.infrastructure.config_file.models import read_configuration
from hermes.infrastructure.hecate.client import HecateClient
from hermes.infrastructure.hecate.identity_store import FileIdentityStore
from hermes.infrastructure.hecate.keys import generate_keys
from hermes.infrastructure.logging import configure_logging, outside_requests
from hermes.infrastructure.persistence.audit_repository import SqlAuditLog
from hermes.infrastructure.persistence.channels_repository import SqlChannelRepository
from hermes.infrastructure.persistence.config_repository import SqlConfigRepository
from hermes.infrastructure.persistence.engine import DATABASE_DOWN, create_engine, database_unavailable
from hermes.infrastructure.persistence.idempotency_repository import SqlIdempotencyRepository
from hermes.infrastructure.persistence.migrations import apply_migrations
from hermes.infrastructure.persistence.tokens_repository import SqlUsedTokens
from hermes.infrastructure.providers.registry import load_registry
from hermes.infrastructure.servers import EmbeddedServer, server_config
from hermes.infrastructure.settings import ServeSettings
from hermes.infrastructure.sockets import bind_unix_socket, prepare_private_dir

logger = logging.getLogger(__name__)

PURGE_INTERVAL_S = 300
SECRETS_INTERVAL_S = 240
RENEWAL_LOOK_S = 300.0
HEALTH_DATABASE_S = 2.0


class ServeError(Exception):
    """`hermes serve` cannot go on: the CLI writes `error: <message>` and exits with 1."""


@dataclass
class Runtime:
    """What the start leaves for the requests and for the stop."""

    state: ConfigurationState = field(default_factory=ConfigurationState)
    engine: AsyncEngine | None = None
    password: SecretValue | None = None


class RuntimeUsedTokens:
    """The `jti` repository on the database of the runtime, once the start has connected to it."""

    def __init__(self, runtime: Runtime) -> None:
        self._runtime = runtime

    def _repository(self) -> SqlUsedTokens:
        if self._runtime.engine is None:
            raise database_unavailable()
        return SqlUsedTokens(self._runtime.engine)

    async def remember(self, issuer: str, jti: str, expires_at: datetime) -> bool:
        return await self._repository().remember(issuer, jti, expires_at)

    async def purge(self, now: datetime) -> int:
        return await self._repository().purge(now) if self._runtime.engine is not None else 0


def _prepare[T](path: Path, action: Callable[[Path], T]) -> T:
    try:
        return action(path)
    except OSError as error:
        raise ServeError(f"cannot prepare {path}: {error.strerror or error}") from error


@contextlib.contextmanager
def _listening(path: Path) -> Iterator[socket.socket]:
    sock = _prepare(path, bind_unix_socket)
    try:
        yield sock
    finally:
        sock.close()
        path.unlink(missing_ok=True)


async def _renewals(identity: IdentityService) -> None:
    """The renewal of the certificate when it is due; it looks again at least every five minutes."""
    while True:
        delay = await identity.renewal_tick()
        await asyncio.sleep(min(delay, RENEWAL_LOOK_S))


class Hermes:
    """The pieces of a running Hermes, wired as research R1 says."""

    def __init__(self, settings: ServeSettings, secrets: SecretSource | None) -> None:
        self.settings = settings
        self.clock = SystemClock()
        self.runtime = Runtime()
        store = FileIdentityStore(state_dir=settings.state_dir, proxy_dir=settings.proxy_dir)
        self.hecate = HecateClient(
            settings.hecate_url,
            identity=lambda: self.identity.identity,
            files=store.current_files,
            revocations=lambda: self.identity.revocation_lists(),
            clock=self.clock,
        )
        self.identity = IdentityService(store=store, hecate=self.hecate, clock=self.clock, new_keys=generate_keys)
        self.identity.load()
        self.cache = SecretCache(self.identity, self.clock)
        self.secrets: SecretSource = secrets if secrets is not None else self.cache
        self.http = httpx.AsyncClient()
        self.providers = load_registry(
            fake_enabled=settings.fake_providers, http=self.http, clock=self.clock, fake_dir=settings.fake_dir
        )
        self.manifests = {manifest.provider: manifest for manifest in self.providers.manifests()}
        self.statuses = InstanceStatuses()
        self.capabilities = Capabilities(
            configuration=self.runtime.state,
            providers=self.providers,
            statuses=self.statuses,
            secrets=self.cache,
            clock=self.clock,
        )
        self.health = HealthCheck(
            identity_problem=self.identity.problem,
            access_loaded=self.identity.access_loaded,
            database_answers=self._database_answers,
            configuration_loaded=lambda: self.runtime.state.loaded,
        )
        self.verification: Verification | None = None
        self.idempotency: Idempotency | None = None
        self.polling = False
        self.pollers: dict[str, asyncio.Task[None]] = {}
        self.api = self._network_api()
        self.admin = self._admin_app()
        self.startup = Startup(
            startup_steps(
                identity=self._identity_step,
                publications=self.identity.ensure_publications,
                database_password=self._password_step,
                database=self._database_step,
                migrations=self._migrations_step,
                configuration=self._configuration_step,
                instances=self.capabilities.test_all,
                polling=self._polling_step,
            ),
            wait_for_enrollment=self.identity.wait_for_enrollment,
        )

    def ready(self) -> bool:
        return self.startup.ready and self.identity.problem() is None

    async def _identity_step(self) -> None:
        if self.identity.problem() is not None:
            raise NeedsEnrollment

    async def _password_step(self) -> None:
        self.runtime.password = await self.secrets.resolve(self.settings.database_password_ref)

    def _password(self) -> SecretValue:
        """The current password of the database: the one in the cache, so a rotation reaches new connections."""
        if self.secrets is self.cache:
            return self.cache.current(self.settings.database_password_ref)
        if self.runtime.password is None:
            raise HermesError(ErrorCode.INTERNAL, "the password of the database is not resolved")
        return self.runtime.password

    async def _database_step(self) -> None:
        if self.runtime.engine is None:
            self.runtime.engine = create_engine(self.settings.database_url, self._password)
        async with self.runtime.engine.connect() as connection:
            await connection.execute(text("SELECT 1"))

    def _engine(self) -> AsyncEngine:
        if self.runtime.engine is None:
            raise database_unavailable()
        return self.runtime.engine

    async def _database_answers(self) -> bool:
        """The database has its tables and answers now: the health asks it each time, so a fall shows."""
        if self.verification is None or self.runtime.engine is None:
            return False
        try:
            async with asyncio.timeout(HEALTH_DATABASE_S):
                async with self.runtime.engine.connect() as connection:
                    await connection.execute(text("SELECT 1"))
        except (TimeoutError, HermesError, *DATABASE_DOWN):
            return False
        return True

    async def _migrations_step(self) -> None:
        await apply_migrations(self._engine())
        self._install_services(self._engine())

    def _install_services(self, engine: AsyncEngine) -> None:
        """The services on the database, once it has its tables: the API sends and the local channel configures, even
        while the start waits on a configuration it cannot load."""
        if self.verification is not None:
            return
        channels = SqlChannelRepository(engine)
        audit = SqlAuditLog(engine)
        send_message = SendMessage(
            configuration=self.runtime.state,
            channels=channels,
            providers=self.providers,
            secrets=self.cache,
            audit=audit,
            clock=self.clock,
            statuses=self.statuses,
        )
        self.verification = Verification(
            configuration=self.runtime.state,
            channels=channels,
            providers=self.providers,
            secrets=self.cache,
            clock=self.clock,
            code_minutes=self.settings.verification_code_minutes,
        )
        configuration = ConfigurationAdmin(
            state=self.runtime.state,
            repository=SqlConfigRepository(engine),
            read=read_configuration,
            write=write_configuration,
            manifests=self.manifests,
            fake_enabled=self.settings.fake_providers,
            secrets=HecateSecretChecker(self.identity),
            clock=self.clock,
            applied=self._sync_polling,
        )
        self.idempotency = Idempotency(SqlIdempotencyRepository(engine), self.clock)
        install_chat(self.api, ChatServices(send_message, self.idempotency))
        install_admin(
            self.admin,
            AdminServices(
                identity=self.identity, configuration=configuration, verification=self.verification, health=self.health
            ),
        )

    async def _configuration_step(self) -> None:
        """The configuration in the database, if it still fits the libraries that this Hermes loaded: one that uses the
        fake provider without `HERMES_FAKE_PROVIDERS=enabled` is rejected here too."""
        loaded = await SqlConfigRepository(self._engine()).load()
        problems = validate(loaded, manifests=self.manifests, fake_enabled=self.settings.fake_providers)
        if problems:
            logger.warning(
                "configuration.invalid",
                extra={"fields": {"problems": [f"{problem.path}: {problem.message}" for problem in problems]}},
            )
            raise problems_error(problems)
        self.runtime.state.replace(loaded)
        logger.info("configuration.loaded")

    async def _polling_step(self) -> None:
        self.polling = True
        self._sync_polling(self.runtime.state.current())

    def _sync_polling(self, configuration: Configuration) -> None:
        """One polling per instance once the start reaches it: after an apply, the new instances start theirs and the
        ones that are gone stop it. A polling outlives the request of the apply, so it starts outside it."""
        if not self.polling or self.verification is None:
            return
        names = {instance.name for instance in configuration.instances}
        for name in sorted(set(self.pollers) - names):
            self.pollers.pop(name).cancel()
        for name in sorted(names - set(self.pollers)):
            self.pollers[name] = asyncio.create_task(
                self.verification.poll_forever(name), name=f"polling.{name}", context=outside_requests()
            )

    async def _refresh_password(self) -> None:
        if self.secrets is self.cache and self.runtime.engine is not None:
            await self.cache.resolve(self.settings.database_password_ref)

    async def _password_after_enrollments(self) -> None:
        """The password of the database again as soon as each enrollment ends (research R7 of change 011): while Hécate
        refused the identity, the cache could drop it, and a new connection takes it from the cache without waiting. A
        failure is logged with its type only, as the refresh of every 240 s does, which asks for it again later."""
        while True:
            await self.identity.wait_for_enrollment()
            try:
                await self._refresh_password()
            except Exception as error:
                logger.warning("secrets.refresh.failed", extra={"fields": {"reason": type(error).__name__}})

    async def _purge_used_tokens(self) -> None:
        await RuntimeUsedTokens(self.runtime).purge(self.clock.now())

    async def _purge_idempotency_keys(self) -> None:
        if self.idempotency is not None:
            await self.idempotency.purge()

    def background(self) -> list[asyncio.Task[None]]:
        return [
            asyncio.create_task(self.startup.run(), name="startup"),
            asyncio.create_task(_renewals(self.identity), name="renewals"),
            asyncio.create_task(
                every(PUBLICATIONS_INTERVAL_S, self.identity.refresh_publications, name="publications.refresh"),
                name="publications",
            ),
            asyncio.create_task(
                every(SECRETS_INTERVAL_S, self._refresh_password, name="secrets.refresh"), name="secrets"
            ),
            asyncio.create_task(self._password_after_enrollments(), name="secrets.enrollment"),
            asyncio.create_task(every(PURGE_INTERVAL_S, self._purge_used_tokens, name="tokens.purge"), name="purge"),
            asyncio.create_task(
                every(PURGE_INTERVAL_S, self._purge_idempotency_keys, name="idempotency.purge"), name="idempotency"
            ),
        ]

    def _network_api(self) -> FastAPI:
        api = create_network_api()
        authenticator = CallAuthenticator(
            trust=self.identity, used_tokens=RuntimeUsedTokens(self.runtime), clock=self.clock
        )
        install_authentication(api, Authentication(authenticator=authenticator, ready=self.ready))
        install_capabilities(api, CapabilitiesServices(self.capabilities))
        return api

    def _admin_app(self) -> FastAPI:
        admin = create_admin_app()
        install_admin(admin, AdminServices(identity=self.identity, health=self.health))
        return admin

    async def close(self) -> None:
        for task in self.pollers.values():
            task.cancel()
        await asyncio.gather(*self.pollers.values(), return_exceptions=True)
        if self.runtime.engine is not None:
            await self.runtime.engine.dispose()
        await self.http.aclose()
        await self.hecate.aclose()


async def serve(
    settings: ServeSettings, *, secrets: SecretSource | None = None, stop: asyncio.Event | None = None
) -> None:
    """Serves until `stop` is set or SIGINT or SIGTERM arrives; then stops both servers and removes their sockets.

    `secrets` is where the password of the database comes from; by default, the cache of the secrets of Hécate.
    """
    configure_logging(settings.log_level)
    stop = stop if stop is not None else asyncio.Event()
    loop = asyncio.get_running_loop()
    async with contextlib.AsyncExitStack() as stack:
        _prepare(settings.run_dir, lambda path: path.mkdir(parents=True, exist_ok=True))
        _prepare(settings.admin_dir, prepare_private_dir)
        api_socket = stack.enter_context(_listening(settings.api_socket))
        admin_socket = stack.enter_context(_listening(settings.admin_socket))
        for signum in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(signum, stop.set)
            stack.callback(loop.remove_signal_handler, signum)
        hermes = Hermes(settings, secrets)
        stack.push_async_callback(hermes.close)
        servers = {
            "api": (EmbeddedServer(server_config(hermes.api)), api_socket),
            "admin": (EmbeddedServer(server_config(hermes.admin)), admin_socket),
        }
        serving = [
            asyncio.create_task(server.serve(sockets=[sock]), name=name) for name, (server, sock) in servers.items()
        ]
        background = hermes.background()
        stopping = asyncio.create_task(stop.wait(), name="stop")
        logger.info(
            "hermes.started",
            extra={"fields": {"api_socket": str(settings.api_socket), "admin_socket": str(settings.admin_socket)}},
        )
        done, _ = await asyncio.wait([stopping, *serving], return_when=asyncio.FIRST_COMPLETED)
        stopped_alone = [task.get_name() for task in serving if task in done]
        for server, _ in servers.values():
            server.should_exit = True
        for task in (*background, stopping):
            task.cancel()
        await asyncio.gather(*serving, *background, stopping, return_exceptions=True)
    logger.info("hermes.stopped")
    if stopped_alone:
        raise ServeError(f"the {stopped_alone[0]} server stopped on its own")


def run_serve(settings: ServeSettings, err: TextIO) -> int:
    """0 after a stop by signal; 1 if a socket cannot be prepared or a server stops on its own."""
    try:
        asyncio.run(serve(settings))
    except ServeError as error:
        err.write(f"error: {error}\n")
        return 1
    return 0
