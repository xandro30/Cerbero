import asyncio
import copy
import tempfile
import threading
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from pathlib import Path

from hecate.application.ports import ConcurrencyConflict, EventRecord, KeyMaterial, SecretMaterial
from hecate.application.secrets.service import SecretsService, SecretsSettings
from hecate.application.sealing.service import SealingService
from hecate.application.sealing.settings import KdfLimits, SealingSettings
from hecate.domain.sealing.access_log import AccessLogEntry
from hecate.domain.sealing.attempts import AttemptPolicy
from hecate.domain.sealing.protection import SealConfig
from hecate.infrastructure.clock import SystemClock
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository, SqliteSecretStore

# Cheapest Argon2id libsodium accepts: only for tests, production uses SENSITIVE or MODERATE.
MINIMAL_KDF = KdfLimits(opslimit=1, memlimit=8192)
START = datetime(2026, 11, 7, 12, 0, tzinfo=UTC)
GOOD_PASSPHRASE = "correct horse battery"


class FakeClock:
    def __init__(self, now: datetime = START) -> None:
        self._now = now

    def now(self) -> datetime:
        return self._now

    def advance(self, delta: timedelta) -> None:
        self._now += delta


class InMemorySealRepository:
    def __init__(self) -> None:
        self.config: SealConfig | None = None
        self.attempts = AttemptPolicy()

    def load_config(self) -> SealConfig | None:
        return self.config

    def save_config(self, config: SealConfig) -> None:
        self.config = config

    def load_attempts(self) -> AttemptPolicy:
        return self.attempts

    def save_attempts(self, attempts: AttemptPolicy) -> None:
        self.attempts = attempts

    @contextmanager
    def transaction(self) -> Iterator[None]:
        snapshot = copy.deepcopy((self.config, self.attempts))
        try:
            yield
        except BaseException:
            self.config, self.attempts = snapshot
            raise


class InMemoryAccessLog:
    def __init__(self) -> None:
        self.items: list[AccessLogEntry] = []

    def append(self, entry: AccessLogEntry) -> None:
        self.items.append(entry)

    def entries(self) -> list[AccessLogEntry]:
        return list(self.items)


class InMemorySecretStore:
    """EventStore + MaterialStore in memory; transaction() undoes everything when it fails."""

    def __init__(self) -> None:
        self.events: dict[str, list[EventRecord]] = {}
        self.secret_material: dict[tuple[str, int], SecretMaterial] = {}
        self.key_material: dict[tuple[str, int], KeyMaterial] = {}

    def _state(self):
        return (self.events, self.secret_material, self.key_material)

    @contextmanager
    def transaction(self) -> Iterator[None]:
        snapshot = copy.deepcopy(self._state())
        try:
            yield
        except BaseException:
            self.events, self.secret_material, self.key_material = snapshot
            raise

    def load(self, stream: str) -> list[EventRecord]:
        return list(self.events.get(stream, []))

    def append(self, stream: str, events: list[EventRecord], expected_version: int) -> int:
        current = len(self.events.get(stream, []))
        if current != expected_version:
            raise ConcurrencyConflict(current)
        self.events.setdefault(stream, []).extend(events)
        return current + len(events)

    def streams(self, prefix: str) -> list[str]:
        return sorted(s for s in self.events if s.startswith(prefix))

    def save_secret_material(self, path: str, version: int, material: SecretMaterial) -> None:
        self.secret_material[(path, version)] = material

    def load_secret_material(self, path: str, version: int) -> SecretMaterial | None:
        return self.secret_material.get((path, version))

    def delete_secret_material(self, path: str, version: int) -> None:
        self.secret_material.pop((path, version), None)

    def save_key_material(self, identity: str, key_version: int, material: KeyMaterial) -> None:
        self.key_material[(identity, key_version)] = material

    def load_key_material(self, identity: str, key_version: int) -> KeyMaterial | None:
        return self.key_material.get((identity, key_version))


def make_service(
    repo=None, access_log=None, clock=None, kdf: KdfLimits = MINIMAL_KDF
) -> SealingService:
    return SealingService(
        crypto=SoftwareCryptoProvider(),
        repo=repo if repo is not None else InMemorySealRepository(),
        access_log=access_log if access_log is not None else InMemoryAccessLog(),
        clock=clock if clock is not None else FakeClock(),
        settings=SealingSettings(kdf=kdf),
    )


@contextmanager
def short_dir() -> Iterator[Path]:
    """A private directory with a short path: Unix socket paths are limited to ~100 bytes."""
    with tempfile.TemporaryDirectory(prefix="hc", dir="/tmp") as name:
        yield Path(name)


class ServerThread:
    """Runs a ControlServer in a background thread with its own event loop."""

    def __init__(self, server) -> None:
        self._server = server
        self._loop = asyncio.new_event_loop()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._ready = threading.Event()

    def _run(self) -> None:
        asyncio.set_event_loop(self._loop)
        self._loop.run_until_complete(self._server.start())
        self._ready.set()
        self._loop.run_forever()

    def __enter__(self) -> "ServerThread":
        self._thread.start()
        assert self._ready.wait(10), "server did not start"
        return self

    def __exit__(self, *exc) -> None:
        asyncio.run_coroutine_threadsafe(self._server.stop(), self._loop).result(10)
        self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join(10)
        self._loop.close()


def initialize(service: SealingService, passphrase: str = GOOD_PASSPHRASE) -> list[str]:
    """Run begin + confirm and return the three share texts."""
    out = service.init_begin(passphrase)
    service.init_confirm(out["init_id"], out["shares"][:2])
    return out["shares"]


class SecretsRig:
    """An unsealed Hécate with a SecretsService over in-memory fakes."""

    def __init__(self, *, unsealed: bool = True, settings: SecretsSettings | None = None) -> None:
        self.clock = FakeClock()
        self.access_log = InMemoryAccessLog()
        self.sealing = make_service(access_log=self.access_log, clock=self.clock)
        self.shares = initialize(self.sealing)
        if unsealed:
            self.sealing.unseal_with_passphrase(GOOD_PASSPHRASE)
        self.store = InMemorySecretStore()
        self.service = SecretsService(
            sealing=self.sealing,
            store=self.store,
            crypto=SoftwareCryptoProvider(),
            access_log=self.access_log,
            clock=self.clock,
            settings=settings or SecretsSettings(),
        )

    def secret_rows(self):
        return [e for e in self.access_log.items if e.operation.value.startswith("secret_")]


class SqliteStack:
    """The real adapters over one database file, like `hecate serve` builds them."""

    def __init__(self, db_path: Path, clock=None) -> None:
        self.clock = clock if clock is not None else SystemClock()
        self.repo = SqliteSealRepository(db_path)
        self.access_log = SqliteAccessLog(db_path)
        self.store = SqliteSecretStore(db_path)
        self.sealing = SealingService(
            crypto=SoftwareCryptoProvider(), repo=self.repo, access_log=self.access_log,
            clock=self.clock, settings=SealingSettings(kdf=MINIMAL_KDF),
        )
        self.service = SecretsService(
            sealing=self.sealing, store=self.store, crypto=SoftwareCryptoProvider(),
            access_log=self.access_log, clock=self.clock,
        )

    def close(self) -> None:
        self.sealing.shutdown()
        self.repo.close()
        self.access_log.close()
        self.store.close()
