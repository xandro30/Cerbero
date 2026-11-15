"""The Telegram library: `sendMessage` of the Bot API with httpx (research R9, contracts/telegram.md).

The token goes in the path of the URL, so no exception of httpx leaves this module: each failure becomes a
`ProviderFailure` without the URL, raised without its cause. A send that did not leave (no connection, or a 429) is
retried within the timeout of the function; one that may have left is not (research R10).
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import httpx

from hermes.domain.configuration.model import Instance
from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.contract import SEND_MESSAGE, SERVICE
from hermes.domain.services.chat.provider import Delivery, IncomingMessage, Rendered, SentMessage
from hermes.domain.services.manifest import Manifest
from hermes.domain.shared.errors import ErrorCode, HermesError
from hermes.domain.shared.secret import SecretValue
from hermes.infrastructure.providers.retry import NotSent, RetryTiming, send_with_retries
from hermes.infrastructure.providers.telegram.errors import (
    check_refusal,
    outcome_unknown,
    refusal,
    retry_after,
    unreachable_for_test,
)
from hermes.infrastructure.providers.telegram.polling import TelegramPoller
from hermes.infrastructure.providers.telegram.rendering import render

API = "https://api.telegram.org"


def _answer(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError:
        return None


def _message_id(body: Any) -> str | None:
    if not isinstance(body, dict) or body.get("ok") is not True or not isinstance(body.get("result"), dict):
        return None
    message_id = body["result"].get("message_id")
    if not isinstance(message_id, int) or isinstance(message_id, bool):
        return None
    return str(message_id)


class TelegramProvider:
    def __init__(
        self, manifest: Manifest, client: httpx.AsyncClient, *, api: str = API, timing: RetryTiming | None = None
    ) -> None:
        self.manifest = manifest
        self._client = client
        self._api = api.rstrip("/")
        function = manifest.function(SERVICE, SEND_MESSAGE.name)
        self._timeout = float(function.timeout_s if function is not None else SEND_MESSAGE.timeout_s)
        self._timing = timing if timing is not None else RetryTiming()
        self._poller = TelegramPoller(client, self._api)

    async def receive(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> list[IncomingMessage]:
        return await self._poller.receive(instance, credentials)

    async def check_connection(self, instance: Instance, credentials: Mapping[str, SecretValue]) -> None:
        """`getMe`: the token of the bot works (contracts/telegram.md#errores)."""
        token = credentials.get("bot_token")
        if token is None:
            raise HermesError(ErrorCode.INTERNAL, "the instance has no bot token")
        try:
            response = await self._client.post(f"{self._api}/bot{token.reveal()}/getMe", timeout=self._timeout)
        except httpx.HTTPError:
            raise unreachable_for_test() from None
        body = _answer(response)
        if response.status_code == 200 and isinstance(body, dict) and body.get("ok") is True:
            return
        raise check_refusal(response.status_code, body) from None

    def render(self, message: NeutralMessage) -> Rendered:
        return render(message, max_length=self.manifest.max_length)

    async def send(self, delivery: Delivery) -> SentMessage:
        token = delivery.credentials.get("bot_token")
        if token is None:
            raise HermesError(ErrorCode.INTERNAL, "the instance has no bot token")
        payload = {
            "chat_id": delivery.address,
            "text": delivery.text,
            "parse_mode": "HTML",
            "link_preview_options": {"is_disabled": True},
        }
        url = f"{self._api}/bot{token.reveal()}/sendMessage"
        return await send_with_retries(
            lambda timeout_s: self._send_once(url, payload, timeout_s),
            timeout_s=self._timeout,
            provider=self.manifest.provider,
            timing=self._timing,
        )

    async def _send_once(self, url: str, payload: dict[str, Any], timeout_s: float) -> SentMessage:
        try:
            response = await self._client.post(url, json=payload, timeout=timeout_s)
        except httpx.ConnectError, httpx.ConnectTimeout, httpx.PoolTimeout:
            raise NotSent from None
        except httpx.HTTPError:
            raise outcome_unknown() from None
        body = _answer(response)
        if response.status_code == 429:
            raise NotSent(retry_after_s=retry_after(body)) from None
        if response.status_code != 200:
            raise refusal(response.status_code, body) from None
        reference = _message_id(body)
        if reference is None:
            raise outcome_unknown() from None
        return SentMessage(reference)
