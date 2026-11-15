"""The configuration of Hermes: the snapshot that the use cases read, replaced at once (research R6), and its plan,
application and export through the local channel (FR-024, FR-025, research R7)."""

from __future__ import annotations

import logging
from collections.abc import Callable, Mapping
from dataclasses import asdict, dataclass, field
from typing import Protocol

from hermes.application.ports import AuditEntry, AuditKind, ConfigRepository, HecateApi
from hermes.domain.configuration.diff import Change, diff, plan_digest
from hermes.domain.configuration.digest import configuration_digest
from hermes.domain.configuration.model import Configuration
from hermes.domain.configuration.validation import Problem, problems_error, validate
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.references import SecretReference

ADMIN = "admin"

logger = logging.getLogger(__name__)


class ConfigurationState:
    """Each request takes the snapshot of the moment it starts: the one before or after an apply, never a mix."""

    def __init__(self, initial: Configuration | None = None) -> None:
        self._current = initial

    @property
    def loaded(self) -> bool:
        return self._current is not None

    def current(self) -> Configuration:
        if self._current is None:
            raise HermesError(ErrorCode.UNAVAILABLE, "configuration not loaded")
        return self._current

    def replace(self, configuration: Configuration) -> None:
        self._current = configuration


@dataclass(frozen=True)
class Plan:
    """What an application would do, and the digest that ties it to the file and to what the database had."""

    changes: tuple[Change, ...]
    digest: str
    configuration: Configuration = field(repr=False)
    current_digest: bytes = field(repr=False)


class SecretChecker(Protocol):
    async def unresolved(self, references: Mapping[str, SecretReference]) -> list[Problem]:
        """The references, by their path in the file, that Hécate does not resolve for Hermes."""
        ...


class HecateSecretChecker:
    """Rule 9: each reference that is new or changes must resolve in Hécate; if Hécate does not answer, nothing is
    applied. Each one is asked on its own, so the file says which one fails."""

    def __init__(self, hecate: HecateApi) -> None:
        self._hecate = hecate

    async def unresolved(self, references: Mapping[str, SecretReference]) -> list[Problem]:
        problems = []
        for path, reference in references.items():
            try:
                await self._hecate.resolve([reference])
            except HermesError as error:
                if error.code is not ErrorCode.NOT_FOUND:
                    raise HermesError(
                        ErrorCode.UNAVAILABLE, "hecate cannot check the secrets of the file: nothing was applied"
                    ) from None
                problems.append(Problem(path, "does not resolve in Hécate, or Hermes cannot read it"))
        return problems


def _changed_references(current: Configuration, new: Configuration) -> dict[str, SecretReference]:
    before = {
        (instance.name, field): reference
        for instance in current.instances
        for field, reference in instance.secrets.items()
    }
    return {
        f"instances[{index}].secrets.{field}": reference
        for index, instance in enumerate(new.instances)
        for field, reference in instance.secrets.items()
        if before.get((instance.name, field)) != reference
    }


class ConfigurationAdmin:
    """Plan, apply and export the configuration (FR-024, FR-025, research R7)."""

    def __init__(
        self,
        *,
        state: ConfigurationState,
        repository: ConfigRepository,
        read: Callable[[str], Configuration],
        write: Callable[[Configuration], str],
        manifests: Mapping[str, Manifest],
        fake_enabled: bool,
        secrets: SecretChecker,
        clock: Clock,
        applied: Callable[[Configuration], None] = lambda configuration: None,
    ) -> None:
        self._state = state
        self._repository = repository
        self._read = read
        self._write = write
        self._manifests = manifests
        self._fake_enabled = fake_enabled
        self._secrets = secrets
        self._clock = clock
        self._applied = applied

    async def plan(self, text: str) -> Plan:
        """Every rule before anything is written: the file, the libraries, the templates, the channels, the versions
        that already exist and the secrets that are new or change."""
        new = self._read(text)
        problems = validate(new, manifests=self._manifests, fake_enabled=self._fake_enabled)
        current = await self._repository.load()
        changes, frozen = diff(current, new)
        problems += frozen
        if problems:
            raise problems_error(problems)
        unresolved = await self._secrets.unresolved(_changed_references(current, new))
        if unresolved:
            raise problems_error(unresolved)
        return Plan(
            changes=changes,
            digest=plan_digest(current, new),
            configuration=new,
            current_digest=configuration_digest(current),
        )

    async def apply(self, text: str, digest: str) -> tuple[Change, ...]:
        """The changes of the plan, written with their audit entry in one transaction (FR-024, FR-029): if the entry
        cannot be written, nothing is applied. The same transaction reads back what stays, so once it commits the
        snapshot and the pollers take it, with nothing left in between that could fail (research R7)."""
        plan = await self.plan(text)
        if plan.digest != digest:
            raise HermesError(ErrorCode.CONFLICT, "the configuration changed since the plan: plan it again")
        if not plan.changes:
            return ()
        entry = AuditEntry(
            at=self._clock.now(),
            kind=AuditKind.CONFIG_APPLIED,
            actor=ADMIN,
            outcome={"changes": [asdict(change) for change in plan.changes]},
        )
        loaded = await self._repository.replace_if(plan.current_digest, plan.configuration, entry)
        self._state.replace(loaded)
        self._applied(loaded)
        logger.info("config.applied", extra={"fields": {"changes": len(plan.changes)}})
        return plan.changes

    async def export(self) -> str:
        return self._write(await self._repository.load())
