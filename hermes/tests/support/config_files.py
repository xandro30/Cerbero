"""Configuration files for the tests (contracts/config-file.md): the one of support/configuration.py, as YAML."""

from __future__ import annotations

SCHEMA = """\
          type: object
          required: [mac, ip, case_id]
          additionalProperties: false
          properties:
            mac: { type: string, pattern: "^([0-9A-F]{2}:){5}[0-9A-F]{2}$" }
            ip: { type: string, format: ipv4 }
            case_id: { type: string, minLength: 1, maxLength: 64 }
"""

VALID_YAML = f"""\
version: 1

variables:
  console_url: https://cerbero.home

instances:
  - name: family_bot
    provider: telegram
    secrets:
      bot_token: secret://hermes/telegram/family_bot/bot_token
  - name: test_fake
    provider: fake

templates:
  - id: device_unknown
    versions:
      - version: 1
        data_schema:
{SCHEMA}\
        message:
          title: Dispositivo desconocido en la red
          severity: high
          summary: "Ha aparecido ${{data.mac}} con la IP ${{data.ip}}."
          facts:
            - {{ label: MAC, value: "${{data.mac}}" }}
            - {{ label: IP, value: "${{data.ip}}" }}
          actions_done: [Bloqueado mientras decides]
          expected: Acepta o bloquea el dispositivo
          link: "${{vars.console_url}}/cases/${{data.case_id}}"
      - version: 2
        data_schema:
{SCHEMA}\
        message:
          title: "Dispositivo nuevo: ${{data.mac}}"
          severity: medium
          link: "${{vars.console_url}}/cases/${{data.case_id}}"

contacts:
  - name: contacto_demo
    channels:
      - {{ service: chat, provider: telegram, instance: family_bot }}
  - name: contacto_fake
    channels:
      - {{ service: chat, provider: fake, instance: test_fake }}
  - name: dos_canales
    channels:
      - {{ service: chat, provider: telegram, instance: family_bot }}
      - {{ service: chat, provider: fake, instance: test_fake }}
"""

FAKE_ONLY_YAML = f"""\
version: 1

variables:
  console_url: https://cerbero.home

instances:
  - name: test_fake
    provider: fake

templates:
  - id: device_unknown
    versions:
      - version: 1
        data_schema:
{SCHEMA}\
        message:
          title: Dispositivo desconocido en la red
          severity: high
          summary: "Ha aparecido ${{data.mac}} con la IP ${{data.ip}}."
          facts:
            - {{ label: MAC, value: "${{data.mac}}" }}
            - {{ label: IP, value: "${{data.ip}}" }}
          actions_done: [Bloqueado mientras decides]
          expected: Acepta o bloquea el dispositivo
          link: "${{vars.console_url}}/cases/${{data.case_id}}"

contacts:
  - name: contacto_fake
    channels:
      - {{ service: chat, provider: fake, instance: test_fake }}
"""
