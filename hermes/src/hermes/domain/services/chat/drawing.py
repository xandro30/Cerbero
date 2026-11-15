"""The layout of a neutral message as chat text, and how it is cut to fit (contracts/telegram.md#dibujo-del-mensaje).

Each provider gives its markup; the length counts what the person sees, in UTF-16 code units, as chat providers count.
When the text does not fit, facts go from the last one, then actions from the last one, and then the summary is
shortened with «…»; the title and the link never change.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, replace

from hermes.domain.messages.neutral import NeutralMessage
from hermes.domain.services.chat.provider import Rendered

ELLIPSIS = "…"


@dataclass(frozen=True)
class Markup:
    title: Callable[[str], str]
    text: Callable[[str], str]


def _same(text: str) -> str:
    return text


PLAIN = Markup(title=_same, text=_same)


def visible_length(text: str) -> int:
    return len(text.encode("utf-16-le")) // 2


def _layout(message: NeutralMessage, markup: Markup) -> str:
    blocks = [[markup.title(message.title), "Gravedad: " + markup.text(message.severity)]]
    if message.summary:
        blocks.append([markup.text(message.summary)])
    detail = [f"• {markup.text(fact.label)}: {markup.text(fact.value)}" for fact in message.facts]
    detail += [f"Hecho: {markup.text(action)}" for action in message.actions_done]
    if message.expected:
        detail.append("Qué se espera: " + markup.text(message.expected))
    if message.link:
        detail.append(markup.text(message.link))
    if detail:
        blocks.append(detail)
    return "\n\n".join("\n".join(block) for block in blocks)


def _fits(message: NeutralMessage, max_length: int) -> bool:
    return visible_length(_layout(message, PLAIN)) <= max_length


def _shortened_summary(message: NeutralMessage, max_length: int) -> NeutralMessage:
    summary = message.summary or ""
    low, high = 0, len(summary)
    while low < high:
        middle = (low + high + 1) // 2
        if _fits(replace(message, summary=summary[:middle].rstrip() + ELLIPSIS), max_length):
            low = middle
        else:
            high = middle - 1
    return replace(message, summary=summary[:low].rstrip() + ELLIPSIS)


def draw(message: NeutralMessage, *, max_length: int, markup: Markup) -> Rendered:
    if _fits(message, max_length):
        return Rendered(_layout(message, markup), truncated=False)
    cut = message
    while cut.facts and not _fits(cut, max_length):
        cut = replace(cut, facts=cut.facts[:-1])
    while cut.actions_done and not _fits(cut, max_length):
        cut = replace(cut, actions_done=cut.actions_done[:-1])
    if cut.summary and not _fits(cut, max_length):
        cut = _shortened_summary(cut, max_length)
    return Rendered(_layout(cut, markup), truncated=True)
