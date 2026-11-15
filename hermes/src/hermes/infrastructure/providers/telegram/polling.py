"""The polling of the bots of Telegram (contracts/telegram.md#uso-de-la-bot-api): `getUpdates` with a long poll of 25 s,
only messages, and the offset of each instance in memory. Neither the content of a message nor its chat reach a log.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import httpx

from hermes.domain.configuration.model import Instance
from hermes.domain.services.chat.provider import IncomingMessage, ProviderFailure
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue

LONG_POLL_S = 25
SLACK_S = 10


def _answer(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError:
        return None


def _message(update: Mapping[str, Any]) -> IncomingMessage | None:
    message = update.get("message")
    if not isinstance(message, Mapping):
        return None
    chat, text = message.get("chat"), message.get("text")
    if not isinstance(chat, Mapping) or not isinstance(text, str):
        return None
    chat_id, chat_type = chat.get("id"), chat.get("type")
    if not isinstance(chat_id, int) or isinstance(chat_id, bool) or not isinstance(chat_type, str):
        return None
    return IncomingMessage(chat=str(chat_id), chat_type=chat_type, text=text)


class TelegramPoller:
    def __init__(self, client: httpx.AsyncClient, api: str) -> None:
        self._client = client
        self._api = api
        self._offsets: dict[str, int] = {}

    async def receive(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> list[IncomingMessage]:
        token = credentials.get("bot_token")
        if token is None:
            raise HermesError(ErrorCode.INTERNAL, "the instance has no bot token")
        offset = self._offsets.get(instance.name, 0)
        payload = {"offset": offset, "timeout": LONG_POLL_S, "allowed_updates": ["message"]}
        try:
            response = await self._client.post(
                f"{self._api}/bot{token.reveal()}/getUpdates", json=payload, timeout=LONG_POLL_S + SLACK_S
            )
        except httpx.HTTPError:
            raise ProviderFailure(ErrorCode.PROVIDER_UNAVAILABLE, "telegram does not answer the polling") from None
        body = _answer(response)
        if response.status_code != 200 or not isinstance(body, dict) or not isinstance(body.get("result"), list):
            raise ProviderFailure(
                ErrorCode.PROVIDER_UNAVAILABLE,
                "telegram refused the polling",
                retryable=response.status_code not in (401, 404),
                credentials_rejected=response.status_code in (401, 404),
            ) from None
        received = []
        for update in body["result"]:
            if not isinstance(update, Mapping):
                continue
            update_id = update.get("update_id")
            if isinstance(update_id, int) and not isinstance(update_id, bool):
                offset = max(offset, update_id + 1)
            message = _message(update)
            if message is not None:
                received.append(message)
        self._offsets[instance.name] = offset
        return received
