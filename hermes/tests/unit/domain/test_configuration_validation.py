"""The validation of a configuration before it is applied (T072, contracts/config-file.md#validación-al-aplicar).

Rule 1 (format and fields) and rule 6 (fixed severity and link) are checked when the file is read; rule 8 (versions that
do not change) in the plan; rule 9 (secrets in Hécate) when it is applied.
"""

from dataclasses import replace

import pytest

from hermes.domain.configuration.model import (
    ChannelSpec,
    Configuration,
    Contact,
    Instance,
    MessageTexts,
    TemplateVersion,
)
from hermes.domain.configuration.validation import Problem, validate
from hermes.domain.shared.references import SecretReference
from hermes.infrastructure.providers.registry import read_manifest
from support.configuration import BOT_TOKEN_REF, configuration

MANIFESTS = {"telegram": read_manifest("telegram"), "fake": read_manifest("fake")}


def problems(config: Configuration, *, fake_enabled: bool = True) -> list[Problem]:
    return validate(config, manifests=MANIFESTS, fake_enabled=fake_enabled)


def with_instance(index: int, instance: Instance) -> Configuration:
    config = configuration()
    instances = list(config.instances)
    instances[index] = instance
    return replace(config, instances=tuple(instances))


def with_message(texts: MessageTexts) -> Configuration:
    config = configuration()
    template = config.templates[0]
    version = replace(template.versions[0], message=texts)
    return replace(config, templates=(replace(template, versions=(version, *template.versions[1:])),))


def test_the_configuration_of_the_tests_is_valid() -> None:
    assert problems(configuration()) == []


def test_rule_2_the_provider_of_each_instance_exists() -> None:
    config = with_instance(0, replace(configuration().instances[0], provider="whatsapp"))
    found = problems(config)
    assert found[0].path == "instances[0].provider"
    assert {problem.path for problem in found[1:]} == {
        "contacts[0].channels[0].provider",
        "contacts[2].channels[0].provider",
    }


def test_rule_2_the_fake_only_with_fake_providers_enabled() -> None:
    [problem] = problems(configuration(), fake_enabled=False)
    assert problem.path == "instances[1].provider"
    assert "HERMES_FAKE_PROVIDERS" in problem.message


def test_rule_2_the_fake_names_its_variable_when_hermes_did_not_load_its_library() -> None:
    """Without HERMES_FAKE_PROVIDERS, Hermes does not load the fake library; the error still says what is missing."""
    [problem] = validate(configuration(), manifests={"telegram": MANIFESTS["telegram"]}, fake_enabled=False)
    assert problem == Problem("instances[1].provider", "the fake provider needs HERMES_FAKE_PROVIDERS=enabled")


def test_rule_3_an_instance_has_every_secret_field_of_its_manifest() -> None:
    config = with_instance(0, replace(configuration().instances[0], secrets={}))
    [problem] = problems(config)
    assert problem.path == "instances[0].secrets.bot_token"


def test_rule_3_an_instance_has_no_secret_field_that_its_manifest_does_not_declare() -> None:
    secrets = {"bot_token": SecretReference.parse(BOT_TOKEN_REF), "api_key": SecretReference.parse(BOT_TOKEN_REF)}
    config = with_instance(0, replace(configuration().instances[0], secrets=secrets))
    assert [problem.path for problem in problems(config)] == ["instances[0].secrets.api_key"]


@pytest.mark.parametrize(
    "reference",
    [
        "secret://hades/telegram/family_bot/bot_token",
        "secret://hermes/telegram/otro_bot/bot_token",
        "secret://hermes/fake/family_bot/bot_token",
        "secret://hermes/telegram/family_bot/token",
        "secret://hermes/telegram/family_bot",
    ],
)
def test_rule_3_each_secret_of_an_instance_is_the_one_of_its_provider_instance_and_field(reference: str) -> None:
    """FR-019: `secret://hermes/<provider>/<instance>/<field>`, so no instance reads the secret of another."""
    config = with_instance(
        0, replace(configuration().instances[0], secrets={"bot_token": SecretReference.parse(reference)})
    )
    [problem] = problems(config)
    assert problem == Problem("instances[0].secrets.bot_token", "must be secret://hermes/telegram/family_bot/bot_token")


def test_rule_3_a_secret_of_its_instance_may_name_a_version() -> None:
    reference = SecretReference.parse(f"{BOT_TOKEN_REF}@2")
    config = with_instance(0, replace(configuration().instances[0], secrets={"bot_token": reference}))
    assert problems(config) == []


def test_rule_3_params_only_carry_the_fields_that_the_manifest_declares_as_not_secret() -> None:
    config = with_instance(0, replace(configuration().instances[0], params={"bot_token": "x", "colour": "red"}))
    assert sorted(problem.path for problem in problems(config)) == [
        "instances[0].params.bot_token",
        "instances[0].params.colour",
    ]


def test_rule_4_the_data_schema_of_each_version_is_a_json_schema() -> None:
    config = configuration()
    template = config.templates[0]
    broken = TemplateVersion(1, {"type": "nonsense"}, template.versions[0].message)
    config = replace(config, templates=(replace(template, versions=(broken, template.versions[1])),))
    [problem] = problems(config)
    assert problem.path == "templates[0].versions[0].data_schema"
    assert "nonsense" not in problem.message


@pytest.mark.parametrize(
    ("texts", "path"),
    [
        (
            MessageTexts(title="Aviso", severity="info", link="${vars.missing}/x"),
            "templates[0].versions[0].message.link",
        ),
        (MessageTexts(title="Aviso ${input.url}", severity="info"), "templates[0].versions[0].message.title"),
        (MessageTexts(title="Aviso", severity="info", summary="${data}"), "templates[0].versions[0].message.summary"),
        (
            MessageTexts(title="Aviso", severity="info", actions_done=("${vars.nope}",)),
            "templates[0].versions[0].message.actions_done[0]",
        ),
    ],
)
def test_rule_5_each_placeholder_is_data_or_an_existing_variable(texts: MessageTexts, path: str) -> None:
    assert [problem.path for problem in problems(with_message(texts))] == [path]


def test_rule_7_each_channel_uses_an_instance_that_exists() -> None:
    config = configuration()
    contact = Contact("contacto_demo", (ChannelSpec("chat", "telegram", "missing_bot"),))
    config = replace(config, contacts=(contact, *config.contacts[1:]))
    assert [problem.path for problem in problems(config)] == ["contacts[0].channels[0].instance"]


def test_rule_7_each_channel_has_the_provider_of_its_instance() -> None:
    config = configuration()
    contact = Contact("contacto_demo", (ChannelSpec("chat", "fake", "family_bot"),))
    config = replace(config, contacts=(contact, *config.contacts[1:]))
    assert [problem.path for problem in problems(config)] == ["contacts[0].channels[0].provider"]


def test_an_instance_used_by_a_channel_cannot_go() -> None:
    config = configuration()
    config = replace(config, instances=config.instances[1:])
    assert {problem.path for problem in problems(config)} == {
        "contacts[0].channels[0].instance",
        "contacts[2].channels[0].instance",
    }


def test_every_problem_comes_together() -> None:
    config = with_instance(0, replace(configuration().instances[0], provider="whatsapp", params={"x": 1}))
    assert len(problems(config, fake_enabled=False)) >= 2
