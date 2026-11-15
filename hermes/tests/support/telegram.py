"""Recorded answers of the Telegram Bot API, in the format of its documentation, for respx."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import httpx

RECORDINGS = Path(__file__).parent / "telegram_recordings"
API = "https://api.telegram.org"


def recorded(name: str) -> dict[str, Any]:
    data: dict[str, Any] = json.loads((RECORDINGS / f"{name}.json").read_text(encoding="utf-8"))
    return data


def recording(name: str) -> httpx.Response:
    data = recorded(name)
    return httpx.Response(data["status"], json=data["body"])


def description(name: str) -> str:
    """The text that Telegram answered, which no error of Hermes may carry."""
    text: str = recorded(name)["body"].get("description", "")
    return text
