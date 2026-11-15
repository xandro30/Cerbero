"""The channel that a message to a contact goes through (contracts/api.md#post-chatv1send_message)."""

from __future__ import annotations

from hermes.domain.configuration.model import ChannelSpec, Contact
from hermes.domain.services.chat.contract import SERVICE
from hermes.domain.shared.errors import ErrorCode, HermesError


def choose_channel(contact: Contact, instance: str | None) -> ChannelSpec:
    """With an instance, the chat channel of the contact in it; without one, its only chat channel."""
    chats = [channel for channel in contact.channels if channel.service == SERVICE]
    if instance is not None:
        found = next((channel for channel in chats if channel.instance == instance), None)
        if found is None:
            raise HermesError(ErrorCode.NOT_FOUND, "the contact has no chat channel in that instance")
        return found
    if not chats:
        raise HermesError(ErrorCode.NOT_FOUND, "the contact has no chat channel")
    if len(chats) > 1:
        raise HermesError(
            ErrorCode.VALIDATION,
            "instance: the contact has more than one chat channel, so the request must name the instance",
            details=[{"path": "instance", "message": "required: the contact has more than one chat channel"}],
        )
    return chats[0]
