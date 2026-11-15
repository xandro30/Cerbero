"""How Telegram draws the neutral message (T029, contracts/telegram.md#dibujo-del-mensaje)."""

import html
import re
from typing import Any

from hypothesis import given, settings
from hypothesis import strategies as st

from hermes.domain.configuration.model import SEVERITIES, Fact
from hermes.domain.messages.neutral import NeutralMessage
from hermes.infrastructure.providers.telegram.rendering import MAX_LENGTH, render

LINK = "https://cerbero.home/cases/case-1234"


def message(**overrides: Any) -> NeutralMessage:
    fields: dict[str, Any] = {
        "title": "Dispositivo desconocido",
        "severity": "high",
        "summary": "Ha aparecido un dispositivo.",
        "facts": (Fact("MAC", "AA:BB:CC:DD:EE:FF"), Fact("IP", "192.168.1.57")),
        "actions_done": ("Bloqueado mientras decides",),
        "expected": "Acepta o bloquea",
        "link": LINK,
    }
    fields.update(overrides)
    return NeutralMessage(**fields)


def visible(text: str) -> str:
    """What Telegram shows: the text without its tags and with its entities decoded."""
    return html.unescape(re.sub(r"</?b>", "", text))


def shown_length(text: str) -> int:
    return len(visible(text).encode("utf-16-le")) // 2


def test_the_whole_message_follows_the_layout_of_the_contract() -> None:
    rendered = render(message())
    assert rendered.text == (
        "<b>Dispositivo desconocido</b>\n"
        "Gravedad: high\n"
        "\n"
        "Ha aparecido un dispositivo.\n"
        "\n"
        "• MAC: AA:BB:CC:DD:EE:FF\n"
        "• IP: 192.168.1.57\n"
        "Hecho: Bloqueado mientras decides\n"
        "Qué se espera: Acepta o bloquea\n"
        f"{LINK}"
    )
    assert rendered.truncated is False


def test_every_text_is_escaped() -> None:
    rendered = render(
        message(
            title="<script> & co",
            summary="a < b > c",
            facts=(Fact("<l>", "&v"),),
            actions_done=("<i>hecho</i>",),
            expected="x & y",
            link="https://cerbero.home/?a=1&b=<2>",
        )
    )
    assert rendered.text.startswith("<b>&lt;script&gt; &amp; co</b>\n")
    assert "a &lt; b &gt; c" in rendered.text
    assert "• &lt;l&gt;: &amp;v" in rendered.text
    assert "Hecho: &lt;i&gt;hecho&lt;/i&gt;" in rendered.text
    assert "Qué se espera: x &amp; y" in rendered.text
    assert rendered.text.endswith("https://cerbero.home/?a=1&amp;b=&lt;2&gt;")
    assert re.findall(r"</?[a-z]+>", rendered.text) == ["<b>", "</b>"]


def test_the_empty_parts_are_not_written() -> None:
    bare = message(summary=None, facts=(), actions_done=(), expected=None, link=None)
    assert render(bare).text == "<b>Dispositivo desconocido</b>\nGravedad: high"
    assert render(message(summary="", facts=(), actions_done=(), expected="", link=None)).text == render(bare).text
    only_link = message(summary=None, facts=(), actions_done=(), expected=None)
    assert render(only_link).text == f"<b>Dispositivo desconocido</b>\nGravedad: high\n\n{LINK}"
    no_summary = message(summary=None)
    assert "Gravedad: high\n\n• MAC" in render(no_summary).text


def test_a_long_message_loses_facts_from_the_last_one() -> None:
    facts = tuple(Fact(f"hecho {index:02d}", "v" * 250) for index in range(20))
    rendered = render(message(facts=facts))
    assert rendered.truncated is True
    assert shown_length(rendered.text) <= MAX_LENGTH
    kept = re.findall(r"• (hecho \d\d):", rendered.text)
    assert 0 < len(kept) < 20
    assert kept == [f"hecho {index:02d}" for index in range(len(kept))]
    assert "Hecho: Bloqueado mientras decides" in rendered.text
    assert rendered.text.startswith("<b>Dispositivo desconocido</b>")
    assert rendered.text.endswith(LINK)


def test_then_it_loses_actions_from_the_last_one() -> None:
    facts = tuple(Fact(f"hecho {index:02d}", "v" * 250) for index in range(20))
    actions = tuple(f"acción {index} " + "a" * 240 for index in range(10))
    rendered = render(message(facts=facts, actions_done=actions, summary="x" * 2048))
    assert rendered.truncated is True
    assert shown_length(rendered.text) <= MAX_LENGTH
    assert "• hecho" not in rendered.text
    kept = re.findall(r"Hecho: acción (\d)", rendered.text)
    assert kept == [str(index) for index in range(len(kept))]


LONG_LINK = "https://cerbero.home/" + "p" * 2027


def test_and_then_the_summary_is_shortened_with_an_ellipsis() -> None:
    long_summary = "palabra " * 256
    actions = tuple("a" * 256 for _ in range(10))
    title = "T" * 256
    rendered = render(
        message(title=title, summary=long_summary, actions_done=actions, expected="e" * 512, link=LONG_LINK)
    )
    assert rendered.truncated is True
    assert shown_length(rendered.text) <= MAX_LENGTH
    assert "Hecho:" not in rendered.text
    assert "• MAC" not in rendered.text
    summary = rendered.text.split("\n\n")[1]
    assert summary.endswith("…")
    assert long_summary.startswith(summary.removesuffix("…"))
    assert len(summary) < len(long_summary)
    assert "Qué se espera: " + "e" * 512 in rendered.text
    assert rendered.text.startswith(f"<b>{title}</b>")
    assert rendered.text.endswith(LONG_LINK)


def test_the_limit_counts_what_telegram_shows_not_the_markup() -> None:
    title = "&" * 256
    bare = message(title=title, summary="", facts=(), actions_done=(), expected=None, link=LONG_LINK)
    room = MAX_LENGTH - shown_length(render(bare).text) - 2
    rendered = render(
        message(title=title, summary="s" * room, facts=(), actions_done=(), expected=None, link=LONG_LINK)
    )
    assert len(rendered.text) > MAX_LENGTH
    assert shown_length(rendered.text) == MAX_LENGTH
    assert rendered.truncated is False


texts = st.text(st.characters(codec="utf-8", exclude_categories=("Cs",)), max_size=256)


@st.composite
def messages(draw: st.DrawFn) -> NeutralMessage:
    return NeutralMessage(
        title=draw(st.text(min_size=1, max_size=256)),
        severity=draw(st.sampled_from(sorted(SEVERITIES))),
        summary=draw(st.none() | st.text(max_size=2048)),
        facts=tuple(draw(st.lists(st.builds(Fact, texts, texts), max_size=20))),
        actions_done=tuple(draw(st.lists(texts, max_size=10))),
        expected=draw(st.none() | st.text(max_size=512)),
        link=draw(st.none() | st.just("https://cerbero.home/") | st.just("https://cerbero.home/" + "p" * 2000)),
    )


@settings(max_examples=60)
@given(messages())
def test_what_telegram_receives_always_fits_and_keeps_title_and_link(drawn: NeutralMessage) -> None:
    rendered = render(drawn)
    shown = visible(rendered.text)
    assert len(shown.encode("utf-16-le")) // 2 <= MAX_LENGTH
    assert shown.startswith(drawn.title)
    if drawn.link:
        assert shown.endswith(drawn.link)
