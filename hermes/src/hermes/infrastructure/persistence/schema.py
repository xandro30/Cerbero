"""The tables of the database of Hermes (data-model.md); migration 0001 creates the same schema."""

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    Column,
    DateTime,
    ForeignKey,
    Identity,
    Integer,
    LargeBinary,
    MetaData,
    Table,
    Text,
    UniqueConstraint,
    text,
)
from sqlalchemy.dialects.postgresql import JSONB

metadata = MetaData()

config_instances = Table(
    "config_instances",
    metadata,
    Column("name", Text, primary_key=True),
    Column("provider", Text, nullable=False),
    Column("params", JSONB, nullable=False, server_default=text("'{}'::jsonb")),
    Column("secrets", JSONB, nullable=False, server_default=text("'{}'::jsonb")),
    Column("requests_per_minute", Integer, nullable=True),
    CheckConstraint(
        "requests_per_minute IS NULL OR requests_per_minute BETWEEN 1 AND 600",
        name="ck_config_instances_requests_per_minute",
    ),
)

config_templates = Table(
    "config_templates",
    metadata,
    Column("template_id", Text, primary_key=True),
    Column("version", Integer, primary_key=True),
    Column("data_schema", JSONB, nullable=False),
    Column("message", JSONB, nullable=False),
    Column("digest", LargeBinary, nullable=False),
    CheckConstraint("version >= 1", name="ck_config_templates_version"),
)

config_variables = Table(
    "config_variables",
    metadata,
    Column("name", Text, primary_key=True),
    Column("value", Text, nullable=False),
)

config_contacts = Table(
    "config_contacts",
    metadata,
    Column("name", Text, primary_key=True),
)

channels = Table(
    "channels",
    metadata,
    Column("id", BigInteger, Identity(always=True), primary_key=True),
    Column("contact", Text, ForeignKey("config_contacts.name", ondelete="CASCADE"), nullable=False),
    Column("service", Text, nullable=False),
    Column("provider", Text, nullable=False),
    Column("instance", Text, ForeignKey("config_instances.name"), nullable=False),
    Column("address", Text, nullable=True),
    Column("verified_at", DateTime(timezone=True), nullable=True),
    Column("code_hash", LargeBinary, nullable=True),
    Column("code_expires_at", DateTime(timezone=True), nullable=True),
    UniqueConstraint(
        "contact", "service", "provider", "instance", name="uq_channels_contact_service_provider_instance"
    ),
    CheckConstraint("verified_at IS NULL OR address IS NOT NULL", name="ck_channels_verified_has_address"),
    CheckConstraint("(code_hash IS NULL) = (code_expires_at IS NULL)", name="ck_channels_code_together"),
    CheckConstraint("code_hash IS NULL OR verified_at IS NULL", name="ck_channels_pending_not_verified"),
)

idempotency_keys = Table(
    "idempotency_keys",
    metadata,
    Column("caller", Text, primary_key=True),
    Column("key", Text, primary_key=True),
    Column("fingerprint", LargeBinary, nullable=False),
    Column("state", Text, nullable=False),
    Column("http_status", Integer, nullable=True),
    Column("response", JSONB, nullable=True),
    Column("created_at", DateTime(timezone=True), nullable=False),
    Column("pending_audit", JSONB, nullable=True),
    CheckConstraint("state IN ('in_progress', 'completed')", name="ck_idempotency_keys_state"),
    CheckConstraint("(state = 'in_progress') = (pending_audit IS NOT NULL)", name="ck_idempotency_keys_pending_audit"),
)

used_call_tokens = Table(
    "used_call_tokens",
    metadata,
    Column("issuer", Text, primary_key=True),
    Column("jti", Text, primary_key=True),
    Column("expires_at", DateTime(timezone=True), nullable=False),
)

audit_log = Table(
    "audit_log",
    metadata,
    Column("id", BigInteger, Identity(always=True), primary_key=True),
    Column("at", DateTime(timezone=True), nullable=False),
    Column("kind", Text, nullable=False),
    Column("actor", Text, nullable=False),
    Column("function", Text, nullable=True),
    Column("instance", Text, nullable=True),
    Column("recipients", JSONB, nullable=True),
    Column("side_effect", Text, nullable=True),
    Column("discloses_data", Text, nullable=True),
    Column("audit_context", JSONB, nullable=True),
    Column("idempotency_key", Text, nullable=True),
    Column("outcome", JSONB, nullable=False),
    CheckConstraint(
        "kind IN ('send', 'config_applied', 'verification_code_issued', 'channel_verified')",
        name="ck_audit_log_kind",
    ),
)
