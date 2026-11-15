"""What Hermes offers and the state of each instance (FR-013, FR-014, contracts/api.md#get-v1capabilities).

The state of an instance comes from its last calls and its last connection test (data-model.md#estado-de-una-instancia).
A failure counts against it when the provider could not be reached, did not answer or refused its credentials; a limit
of the provider or a person that refuses the bot does not. At the start, Hermes tests each instance, and a failure stays
in the state of the instance without stopping the start.
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import datetime
from typing import Any

from hermes.application.configuration import ConfigurationState
from hermes.application.ports import SecretSource
from hermes.domain.configuration.model import Instance
from hermes.domain.instances.status import InstanceRecord, InstanceStatus
from hermes.domain.services.chat.contract import CATEGORY, CONTRACT, SERVICE
from hermes.domain.services.chat.provider import ChatProvider, ChatProviders, ProviderFailure
from hermes.domain.shared.clock import Clock
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue

COUNTS = frozenset({ErrorCode.PROVIDER_UNAVAILABLE, ErrorCode.TIMEOUT})

logger = logging.getLogger(__name__)


def _rejected(failure: HermesError | None) -> bool:
    return isinstance(failure, ProviderFailure) and failure.credentials_rejected


class InstanceStatuses:
    """The state of each instance, in memory."""

    def __init__(self) -> None:
        self._records: dict[str, InstanceRecord] = {}

    def status(self, instance: str) -> InstanceStatus:
        return self._records.get(instance, InstanceRecord()).status

    def record_call(self, instance: str, failure: HermesError | None) -> None:
        record = self._records.get(instance, InstanceRecord())
        if failure is None:
            self._records[instance] = record.with_call(True)
        elif failure.from_provider and failure.code in COUNTS:
            self._records[instance] = record.with_call(False, credentials_rejected=_rejected(failure))

    def record_test(self, instance: str, failure: HermesError | None) -> None:
        record = self._records.get(instance, InstanceRecord())
        self._records[instance] = record.with_test(failure is None, credentials_rejected=_rejected(failure))


@dataclass(frozen=True)
class InstanceTest:
    instance: str
    status: InstanceStatus
    checked_at: datetime


class Capabilities:
    def __init__(
        self,
        *,
        configuration: ConfigurationState,
        providers: ChatProviders,
        statuses: InstanceStatuses,
        secrets: SecretSource,
        clock: Clock,
    ) -> None:
        self._configuration = configuration
        self._providers = providers
        self._statuses = statuses
        self._secrets = secrets
        self._clock = clock

    @property
    def ready(self) -> bool:
        return self._configuration.loaded

    def describe(self) -> dict[str, Any]:
        """Each library with its functions and its instances with their state, without contacts or addresses."""
        instances = sorted(self._configuration.current().instances, key=lambda item: item.name)
        providers = []
        for manifest in self._providers.manifests():
            implementation = next((item for item in manifest.implements if item.service == SERVICE), None)
            if implementation is None:
                continue
            providers.append(
                {
                    "provider": manifest.provider,
                    "version": manifest.version,
                    "functions": [
                        {"name": item.name, "kind": item.kind, "mode": item.mode, "timeout_s": item.timeout_s}
                        for item in implementation.functions
                    ],
                    "instances": [
                        {"name": item.name, "status": self._statuses.status(item.name).value}
                        for item in instances
                        if item.provider == manifest.provider
                    ],
                }
            )
        service = {"category": CATEGORY, "service": SERVICE, "contract": CONTRACT, "providers": providers}
        return {"component": "hermes", "services": [service]}

    def provider_of(self, name: str) -> str:
        return self._instance(name).provider

    async def test(self, name: str) -> InstanceTest:
        """The connection test of an instance; its failure stays in the state of the instance and is raised."""
        instance = self._instance(name)
        provider = self._library(instance)
        credentials = await self._credentials(instance)
        try:
            await provider.check_connection(instance, credentials)
        except HermesError as failure:
            self._statuses.record_test(name, failure)
            raise
        self._statuses.record_test(name, None)
        return InstanceTest(instance=name, status=self._statuses.status(name), checked_at=self._clock.now())

    async def test_all(self) -> None:
        """The test of each instance, at once, at the start."""
        names = [instance.name for instance in self._configuration.current().instances]
        await asyncio.gather(*(self._test_at_start(name) for name in names))

    async def _test_at_start(self, name: str) -> None:
        fields: dict[str, Any] = {"instance": name}
        try:
            await self.test(name)
        except HermesError as error:
            fields["reason"] = error.code.value
        except Exception as error:
            fields["reason"] = type(error).__name__
        fields["status"] = self._statuses.status(name).value
        logger.info("instance.tested", extra={"fields": fields})

    def _instance(self, name: str) -> Instance:
        instance = self._configuration.current().instance(name)
        if instance is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the instance does not exist")
        return instance

    def _library(self, instance: Instance) -> ChatProvider:
        provider = self._providers.chat(instance.provider)
        if provider is None:
            raise HermesError(ErrorCode.INTERNAL, "hermes has no library for the provider of the instance")
        return provider

    async def _credentials(self, instance: Instance) -> Mapping[str, SecretValue]:
        try:
            return {name: await self._secrets.resolve(reference) for name, reference in instance.secrets.items()}
        except HermesError as error:
            if error.code is ErrorCode.INTERNAL:
                raise
            raise HermesError(ErrorCode.INTERNAL, "the secrets of the instance are not available") from None
