"""The plan of an application (T074, contracts/admin-channel.md#cambios): what is deleted, changed and created."""

import re
from dataclasses import replace

from hermes.domain.configuration.diff import Change, diff, plan_digest
from hermes.domain.configuration.model import ChannelSpec, Configuration, Contact, MessageTexts, Variable
from hermes.domain.shared.references import SecretReference
from support.configuration import configuration


def test_the_same_configuration_has_no_changes() -> None:
    assert diff(configuration(), configuration()) == ((), [])


def test_from_nothing_everything_is_created_from_the_variables_to_the_channels() -> None:
    changes, problems = diff(Configuration.empty(), configuration())
    assert problems == []
    assert {change.action for change in changes} == {"create"}
    kinds = [change.kind for change in changes]
    assert kinds == sorted(kinds, key=["variable", "instance", "template_version", "contact", "channel"].index)
    assert Change("create", "template_version", "device_unknown v2") in changes
    assert Change("create", "channel", "contacto_demo telegram/family_bot") in changes


def test_to_nothing_everything_is_deleted_from_the_channels_and_they_lose_their_verification() -> None:
    changes, _ = diff(configuration(), Configuration.empty())
    assert {change.action for change in changes} == {"delete"}
    kinds = [change.kind for change in changes]
    assert kinds == sorted(kinds, key=["channel", "contact", "template_version", "instance", "variable"].index)
    channels = [change for change in changes if change.kind == "channel"]
    assert channels
    assert all(change.detail == "loses its verification" for change in channels)


def test_the_changes_go_deleted_then_changed_then_created() -> None:
    config = configuration()
    new = replace(
        config,
        variables=(Variable("console_url", "https://tenaro.cerbero.home"), Variable("extra", "x")),
        contacts=config.contacts[:2],
    )
    changes, _ = diff(config, new)
    actions = [change.action for change in changes]
    assert actions == sorted(actions, key=["delete", "update", "create"].index)
    assert Change("update", "variable", "console_url", "value") in changes
    assert Change("create", "variable", "extra") in changes
    assert Change("delete", "contact", "dos_canales") in changes


def test_a_channel_that_changes_instance_is_deleted_and_created_again() -> None:
    config = configuration()
    moved = Contact("contacto_fake", (ChannelSpec("chat", "telegram", "family_bot"),))
    new = replace(config, contacts=(config.contacts[0], moved, config.contacts[2]))
    changes, _ = diff(config, new)
    assert changes == (
        Change("delete", "channel", "contacto_fake fake/test_fake", "loses its verification"),
        Change("create", "channel", "contacto_fake telegram/family_bot"),
    )


def test_an_instance_that_changes_says_what_without_the_values() -> None:
    config = configuration()
    instance = replace(
        config.instances[0],
        secrets={"bot_token": SecretReference.parse("secret://hermes/telegram/family_bot/bot_token@2")},
        requests_per_minute=20,
    )
    new = replace(config, instances=(instance, config.instances[1]))
    [change] = diff(config, new)[0]
    assert change == Change("update", "instance", "family_bot", "limits, secrets: bot_token")
    assert "@2" not in (change.detail or "")


def test_changing_a_template_version_that_exists_is_a_problem() -> None:
    config = configuration()
    template = config.templates[0]
    changed = replace(template.versions[0], message=MessageTexts(title="Otro título", severity="high"))
    new = replace(config, templates=(replace(template, versions=(changed, template.versions[1])),))
    _, problems = diff(config, new)
    assert [problem.path for problem in problems] == ["templates[0].versions[0]"]


def test_a_version_that_goes_away_is_deleted() -> None:
    config = configuration()
    template = config.templates[0]
    new = replace(config, templates=(replace(template, versions=template.versions[:1]),))
    assert diff(config, new) == ((Change("delete", "template_version", "device_unknown v2"),), [])


def test_the_plan_digest_covers_the_file_and_what_was_in_the_database() -> None:
    config, empty = configuration(), Configuration.empty()
    digest = plan_digest(empty, config)
    assert re.fullmatch(r"sha256:[0-9a-f]{64}", digest)
    assert plan_digest(empty, config) == digest
    assert plan_digest(config, config) != digest
    assert plan_digest(empty, empty) != digest
