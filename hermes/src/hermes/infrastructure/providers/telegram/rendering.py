"""The neutral message as Telegram HTML: the title in bold and every text escaped (contracts/telegram.md)."""

from __future__ import annotations

import html

from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.drawing import Markup, draw
from hermes.domain.services.chat.provider import Rendered

MAX_LENGTH = 4096


def _escaped(text: str) -> str:
    return html.escape(text, quote=False)


def _bold(text: str) -> str:
    return f"<b>{_escaped(text)}</b>"


HTML = Markup(title=_bold, text=_escaped)


def render(message: NeutralMessage, max_length: int = MAX_LENGTH) -> Rendered:
    """Telegram counts the text it shows, after parsing the HTML, so the markup does not count."""
    return draw(message, max_length=max_length, markup=HTML)
