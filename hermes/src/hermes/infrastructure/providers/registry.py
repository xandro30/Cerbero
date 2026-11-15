"""The provider libraries that Hermes loads, each with its manifest; the fake only with `HERMES_FAKE_PROVIDERS=enabled`
(research R17), so it never reaches production."""

from __future__ import annotations

import json
from collections.abc import Mapping
from importlib import resources
from pathlib import Path

import httpx

from hermes.domain.services.chat.provider import ChatProvider
from hermes.domain.services.manifest import Manifest, ManifestError
from hermes.domain.shared.clock import Clock
from hermes.infrastructure.providers.fake.adapter import FakeProvider
from hermes.infrastructure.providers.telegram.adapter import TelegramProvider

PACKAGES = {
    "fake": "hermes.infrastructure.providers.fake",
    "telegram": "hermes.infrastructure.providers.telegram",
}


def read_manifest(provider: str) -> Manifest:
    text = resources.files(PACKAGES[provider]).joinpath("manifest.json").read_text(encoding="utf-8")
    manifest = Manifest.from_json(json.loads(text))
    if manifest.provider != provider:
        raise ManifestError(f"the manifest of {provider} names another provider")
    return manifest


class ProviderRegistry:
    def __init__(self, providers: Mapping[str, ChatProvider]) -> None:
        self._providers = dict(providers)

    def chat(self, provider: str) -> ChatProvider | None:
        return self._providers.get(provider)

    def manifests(self) -> tuple[Manifest, ...]:
        """The manifests of the loaded libraries, by the name of their provider."""
        return tuple(self._providers[name].manifest for name in sorted(self._providers))


def load_registry(
    *, fake_enabled: bool, http: httpx.AsyncClient, clock: Clock, fake_dir: Path | None = None
) -> ProviderRegistry:
    providers: dict[str, ChatProvider] = {"telegram": TelegramProvider(read_manifest("telegram"), http)}
    if fake_enabled:
        providers["fake"] = FakeProvider(read_manifest("fake"), clock, directory=fake_dir)
    return ProviderRegistry(providers)
