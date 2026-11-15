"""The model of the configuration (T011), with the rules of data-model.md."""

from dataclasses import replace

import pytest

from hermes.domain.configuration.digest import configuration_digest, version_digest
from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    ConfigurationError,
    Contact,
    Fact,
    Instance,
    MessageTexts,
    Template,
    TemplateVersion,
    Variable,
)
from hermes.domain.shared.references import SecretReference

TOKEN = SecretReference.parse("secret://hermes/telegram/family_bot/bot_token")


def texts(**changes: object) -> MessageTexts:
    base = MessageTexts(
        title="Dispositivo desconocido en la red",
        severity="high",
        summary="Se ha conectado ${data.mac}.",
        facts=(Fact(label="MAC", value="${data.mac}"),),
        actions_done=(),
        expected="Aprobar o denegar el bloqueo",
        link="${vars.case_base_url}/${data.case_id}",
    )
    return replace(base, **changes)  # type: ignore[arg-type]


def version(number: int = 1, **changes: object) -> TemplateVersion:
    return TemplateVersion(version=number, data_schema={"type": "object"}, message=texts(**changes))


def configuration(**changes: object) -> Configuration:
    base = Configuration(
        instances=(
            Instance(name="family_bot", provider="telegram", params={}, secrets={"bot_token": TOKEN}),
            Instance(name="admin_bot", provider="telegram", params={"a": 1, "b": 2}, secrets={"bot_token": TOKEN}),
        ),
        templates=(Template(id="device_unknown", versions=(version(1), version(2))),),
        variables=(Variable(name="case_base_url", value="https://tenaro.cerbero.home/cases"), Variable("z", "1")),
        contacts=(
            Contact(
                name="contacto_demo",
                channels=(ChannelSpec("chat", "telegram", "family_bot"), ChannelSpec("chat", "telegram", "admin_bot")),
            ),
            Contact(name="otro", channels=(ChannelSpec("chat", "telegram", "family_bot"),)),
        ),
    )
    return replace(base, **changes)  # type: ignore[arg-type]


def test_a_valid_configuration_builds() -> None:
    assert configuration().instances[0].name == "family_bot"


def test_a_template_version_is_a_number_from_1() -> None:
    with pytest.raises(ConfigurationError, match="version"):
        version(0)


@pytest.mark.parametrize(
    "changes",
    [
        {"title": ""},
        {"title": "x" * 257},
        {"severity": "urgent"},
        {"summary": "x" * 2049},
        {"facts": tuple(Fact("l", "v") for _ in range(21))},
        {"facts": (Fact("x" * 257, "v"),)},
        {"facts": (Fact("l", "x" * 257),)},
        {"actions_done": tuple("hecho" for _ in range(11))},
        {"actions_done": ("x" * 257,)},
        {"expected": "x" * 513},
        {"link": "http://tenaro.cerbero.home/cases/1"},
        {"link": "https://" + "x" * 2050},
        {"link": "not a url"},
    ],
)
def test_each_text_of_the_message_follows_its_rule(changes: dict[str, object]) -> None:
    with pytest.raises(ConfigurationError):
        texts(**changes)


def test_a_text_with_placeholders_is_checked_after_substitution() -> None:
    assert texts(title="${data.title}" + "x" * 300).title.startswith("${data.title}")
    assert texts(severity="${data.severity}").severity == "${data.severity}"
    assert texts(link="${vars.base}/${data.id}").link == "${vars.base}/${data.id}"
    with pytest.raises(ConfigurationError):
        texts(facts=tuple(Fact("${data.l}", "${data.v}") for _ in range(21)))


def test_a_variable_holds_up_to_4096_characters() -> None:
    assert Variable("long", "x" * 4096).value
    with pytest.raises(ConfigurationError):
        Variable("long", "x" * 4097)


def test_a_contact_has_at_least_one_channel_and_one_per_instance() -> None:
    with pytest.raises(ConfigurationError, match="channel"):
        Contact(name="contacto_demo", channels=())
    with pytest.raises(ConfigurationError, match="channel"):
        Contact(
            name="contacto_demo",
            channels=(ChannelSpec("chat", "telegram", "family_bot"), ChannelSpec("chat", "telegram", "family_bot")),
        )


@pytest.mark.parametrize(
    "build",
    [
        lambda: Instance(name="Family", provider="telegram", params={}, secrets={}),
        lambda: Contact(name="a-b", channels=(ChannelSpec("chat", "telegram", "family_bot"),)),
        lambda: Variable(name="1x", value="v"),
        lambda: Template(id="Device", versions=(version(1),)),
        lambda: Template(id="device", versions=()),
        lambda: Template(id="device", versions=(version(1), version(1))),
    ],
)
def test_names_versions_and_presence_follow_their_rules(build: object) -> None:
    with pytest.raises(ConfigurationError):
        build()  # type: ignore[operator]


def test_names_do_not_repeat_within_their_list() -> None:
    with pytest.raises(ConfigurationError, match="family_bot"):
        configuration(instances=configuration().instances + configuration().instances[:1])
    with pytest.raises(ConfigurationError, match="otro"):
        configuration(contacts=configuration().contacts + configuration().contacts[1:])


def test_the_digest_does_not_depend_on_the_order_of_the_lists() -> None:
    base = configuration()
    shuffled = configuration(
        instances=tuple(reversed(base.instances)),
        templates=(Template(id="device_unknown", versions=(version(2), version(1))),),
        variables=tuple(reversed(base.variables)),
        contacts=(
            base.contacts[1],
            Contact(name="contacto_demo", channels=tuple(reversed(base.contacts[0].channels))),
        ),
    )
    assert configuration_digest(shuffled) == configuration_digest(base)
    changed = configuration(variables=(Variable(name="case_base_url", value="https://other"), Variable("z", "1")))
    assert configuration_digest(changed) != configuration_digest(base)


def test_a_version_has_its_own_digest() -> None:
    assert version_digest(version(1)) == version_digest(version(1))
    assert version_digest(version(1)) != version_digest(version(1, title="Otro título"))
