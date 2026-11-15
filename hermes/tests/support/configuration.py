"""A configuration for the tests: a Telegram instance, a fake one, a template with two versions and three contacts."""

from __future__ import annotations

from typing import Any

from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    Contact,
    Fact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
    Variable,
)
from hermes.domain.shared.references import SecretReference

BOT_TOKEN_REF = "secret://hermes/telegram/family_bot/bot_token"
BOT_TOKEN = "123456789:TEST-only-not-a-real-bot-token"
CHAT = "987654321"
FAKE_CHAT = "fake-chat-1"

DEVICE_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": ["mac", "ip", "case_id"],
    "additionalProperties": False,
    "properties": {
        "mac": {"type": "string", "pattern": "^([0-9A-F]{2}:){5}[0-9A-F]{2}$"},
        "ip": {"type": "string", "format": "ipv4"},
        "case_id": {"type": "string", "minLength": 1, "maxLength": 64},
    },
}
DEVICE_TEXTS_V1 = MessageTexts(
    title="Dispositivo desconocido en la red",
    severity="high",
    summary="Ha aparecido ${data.mac} con la IP ${data.ip}.",
    facts=(Fact("MAC", "${data.mac}"), Fact("IP", "${data.ip}")),
    actions_done=("Bloqueado mientras decides",),
    expected="Acepta o bloquea el dispositivo",
    link="${vars.console_url}/cases/${data.case_id}",
)
DEVICE_TEXTS_V2 = MessageTexts(
    title="Dispositivo nuevo: ${data.mac}",
    severity="medium",
    link="${vars.console_url}/cases/${data.case_id}",
)
DATA: dict[str, Any] = {"mac": "AA:BB:CC:DD:EE:FF", "ip": "192.168.1.57", "case_id": "case-1234"}
TITLE_V1 = "Dispositivo desconocido en la red"
LINK = "https://cerbero.home/cases/case-1234"


def configuration() -> Configuration:
    return Configuration(
        instances=(
            Instance("family_bot", "telegram", {}, {"bot_token": SecretReference.parse(BOT_TOKEN_REF)}),
            Instance("test_fake", "fake", {}, {}),
        ),
        templates=(
            Template(
                "device_unknown",
                (
                    TemplateVersion(1, DEVICE_SCHEMA, DEVICE_TEXTS_V1),
                    TemplateVersion(2, DEVICE_SCHEMA, DEVICE_TEXTS_V2),
                ),
            ),
        ),
        variables=(Variable("console_url", "https://cerbero.home"),),
        contacts=(
            Contact("contacto_demo", (ChannelSpec("chat", "telegram", "family_bot"),)),
            Contact("contacto_fake", (ChannelSpec("chat", "fake", "test_fake"),)),
            Contact(
                "dos_canales",
                (ChannelSpec("chat", "telegram", "family_bot"), ChannelSpec("chat", "fake", "test_fake")),
            ),
        ),
    )
