"""The contract of the chat service type (T028): one recipient, the template version and the choice of the channel."""

from typing import Any

import pytest

from hermes.domain.configuration.model import ChannelSpec
from hermes.domain.services.chat.channels import choose_channel
from hermes.domain.services.chat.contract import SEND_MESSAGE, SendRequest, resolve_template
from hermes.domain.shared.errors import ErrorCode, HermesError
from support.configuration import DATA, configuration
from support.contract import load_document


def request(**overrides: Any) -> SendRequest:
    fields: dict[str, Any] = {"recipients": ("contacto_demo",), "template_id": "device_unknown", "data": DATA}
    fields.update(overrides)
    return SendRequest(**fields)


@pytest.mark.parametrize("recipients", [(), ("contacto_demo", "dos_canales")])
def test_the_request_has_exactly_one_recipient(recipients: tuple[str, ...]) -> None:
    with pytest.raises(HermesError) as raised:
        request(recipients=recipients)
    assert raised.value.code is ErrorCode.VALIDATION
    assert raised.value.details[0]["path"] == "recipients"


def test_its_only_recipient_is_the_recipient() -> None:
    assert request().recipient == "contacto_demo"


@pytest.mark.parametrize(
    "overrides",
    [{"recipients": ("Contacto",)}, {"template_id": "Device"}, {"instance": "family-bot"}, {"template_version": 0}],
)
def test_names_and_versions_follow_their_rules(overrides: dict[str, Any]) -> None:
    with pytest.raises(HermesError) as raised:
        request(**overrides)
    assert raised.value.code is ErrorCode.VALIDATION


def test_without_a_version_the_template_gives_its_latest() -> None:
    assert resolve_template(configuration(), request()).version == 2


def test_with_a_version_the_template_gives_that_one() -> None:
    assert resolve_template(configuration(), request(template_version=1)).version == 1


@pytest.mark.parametrize("overrides", [{"template_id": "missing"}, {"template_version": 3}])
def test_a_template_or_a_version_that_does_not_exist_is_not_found(overrides: dict[str, Any]) -> None:
    with pytest.raises(HermesError) as raised:
        resolve_template(configuration(), request(**overrides))
    assert raised.value.code is ErrorCode.NOT_FOUND


def contact(name: str) -> Any:
    found = configuration().contact(name)
    assert found is not None
    return found


def test_with_an_instance_the_channel_is_the_one_in_that_instance() -> None:
    assert choose_channel(contact("dos_canales"), "test_fake") == ChannelSpec("chat", "fake", "test_fake")
    assert choose_channel(contact("dos_canales"), "family_bot") == ChannelSpec("chat", "telegram", "family_bot")


def test_with_an_instance_where_the_contact_has_no_channel_it_is_not_found() -> None:
    with pytest.raises(HermesError) as raised:
        choose_channel(contact("contacto_demo"), "test_fake")
    assert raised.value.code is ErrorCode.NOT_FOUND


def test_without_an_instance_the_channel_is_the_only_chat_channel() -> None:
    assert choose_channel(contact("contacto_demo"), None) == ChannelSpec("chat", "telegram", "family_bot")


def test_without_an_instance_and_more_than_one_chat_channel_it_is_validation() -> None:
    with pytest.raises(HermesError) as raised:
        choose_channel(contact("dos_canales"), None)
    assert raised.value.code is ErrorCode.VALIDATION


def test_the_function_contract_is_the_one_published_in_the_openapi() -> None:
    published = load_document("hermes.v1.json")["paths"]["/chat/v1/send_message"]["post"]["x-cerbero-function"]
    assert published == {
        "kind": SEND_MESSAGE.kind,
        "mode": SEND_MESSAGE.mode,
        "timeout": f"{SEND_MESSAGE.timeout_s}s",
        "side_effect": SEND_MESSAGE.side_effect,
        "discloses_data": SEND_MESSAGE.discloses_data,
        "non_reversible": SEND_MESSAGE.non_reversible,
    }
    assert SEND_MESSAGE.qualified_name == "chat.send_message"


def test_side_effect_and_discloses_data_are_resolved_with_the_request() -> None:
    values = {"recipient": "contacto_demo", "title": "Aviso", "instance": "family_bot", "provider": "telegram"}
    assert (
        SEND_MESSAGE.side_effect_for(values) == "Envía a contacto_demo el mensaje «Aviso» por la instancia family_bot"
    )
    assert SEND_MESSAGE.discloses_data_for(values) == "telegram recibe el texto del mensaje y el chat de contacto_demo"
