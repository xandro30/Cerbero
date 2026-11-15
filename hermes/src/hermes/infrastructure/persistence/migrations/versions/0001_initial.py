"""The initial schema of Hermes: configuration, channels, idempotency keys, call tokens and the append-only audit log.

Revision ID: 0001
"""

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB

revision = "0001"
down_revision = None
branch_labels = None
depends_on = None

APPEND_ONLY = """
CREATE FUNCTION audit_log_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only';
END;
$$
"""


def upgrade() -> None:
    op.create_table(
        "config_instances",
        sa.Column("name", sa.Text(), primary_key=True),
        sa.Column("provider", sa.Text(), nullable=False),
        sa.Column("params", JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("secrets", JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("requests_per_minute", sa.Integer(), nullable=True),
        sa.CheckConstraint(
            "requests_per_minute IS NULL OR requests_per_minute BETWEEN 1 AND 600",
            name="ck_config_instances_requests_per_minute",
        ),
    )
    op.create_table(
        "config_templates",
        sa.Column("template_id", sa.Text(), primary_key=True),
        sa.Column("version", sa.Integer(), primary_key=True),
        sa.Column("data_schema", JSONB(), nullable=False),
        sa.Column("message", JSONB(), nullable=False),
        sa.Column("digest", sa.LargeBinary(), nullable=False),
        sa.CheckConstraint("version >= 1", name="ck_config_templates_version"),
    )
    op.create_table(
        "config_variables",
        sa.Column("name", sa.Text(), primary_key=True),
        sa.Column("value", sa.Text(), nullable=False),
    )
    op.create_table(
        "config_contacts",
        sa.Column("name", sa.Text(), primary_key=True),
    )
    op.create_table(
        "channels",
        sa.Column("id", sa.BigInteger(), sa.Identity(always=True), primary_key=True),
        sa.Column("contact", sa.Text(), sa.ForeignKey("config_contacts.name", ondelete="CASCADE"), nullable=False),
        sa.Column("service", sa.Text(), nullable=False),
        sa.Column("provider", sa.Text(), nullable=False),
        sa.Column("instance", sa.Text(), sa.ForeignKey("config_instances.name"), nullable=False),
        sa.Column("address", sa.Text(), nullable=True),
        sa.Column("verified_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("code_hash", sa.LargeBinary(), nullable=True),
        sa.Column("code_expires_at", sa.DateTime(timezone=True), nullable=True),
        sa.UniqueConstraint(
            "contact", "service", "provider", "instance", name="uq_channels_contact_service_provider_instance"
        ),
        sa.CheckConstraint("verified_at IS NULL OR address IS NOT NULL", name="ck_channels_verified_has_address"),
        sa.CheckConstraint("(code_hash IS NULL) = (code_expires_at IS NULL)", name="ck_channels_code_together"),
        sa.CheckConstraint("code_hash IS NULL OR verified_at IS NULL", name="ck_channels_pending_not_verified"),
    )
    op.create_table(
        "idempotency_keys",
        sa.Column("caller", sa.Text(), primary_key=True),
        sa.Column("key", sa.Text(), primary_key=True),
        sa.Column("fingerprint", sa.LargeBinary(), nullable=False),
        sa.Column("state", sa.Text(), nullable=False),
        sa.Column("http_status", sa.Integer(), nullable=True),
        sa.Column("response", JSONB(), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("pending_audit", JSONB(), nullable=True),
        sa.CheckConstraint("state IN ('in_progress', 'completed')", name="ck_idempotency_keys_state"),
        sa.CheckConstraint(
            "(state = 'in_progress') = (pending_audit IS NOT NULL)", name="ck_idempotency_keys_pending_audit"
        ),
    )
    op.create_table(
        "used_call_tokens",
        sa.Column("issuer", sa.Text(), primary_key=True),
        sa.Column("jti", sa.Text(), primary_key=True),
        sa.Column("expires_at", sa.DateTime(timezone=True), nullable=False),
    )
    op.create_table(
        "audit_log",
        sa.Column("id", sa.BigInteger(), sa.Identity(always=True), primary_key=True),
        sa.Column("at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("kind", sa.Text(), nullable=False),
        sa.Column("actor", sa.Text(), nullable=False),
        sa.Column("function", sa.Text(), nullable=True),
        sa.Column("instance", sa.Text(), nullable=True),
        sa.Column("recipients", JSONB(), nullable=True),
        sa.Column("side_effect", sa.Text(), nullable=True),
        sa.Column("discloses_data", sa.Text(), nullable=True),
        sa.Column("audit_context", JSONB(), nullable=True),
        sa.Column("idempotency_key", sa.Text(), nullable=True),
        sa.Column("outcome", JSONB(), nullable=False),
        sa.CheckConstraint(
            "kind IN ('send', 'config_applied', 'verification_code_issued', 'channel_verified')",
            name="ck_audit_log_kind",
        ),
    )
    op.execute(APPEND_ONLY)
    op.execute(
        "CREATE TRIGGER audit_log_append_only BEFORE UPDATE OR DELETE ON audit_log "
        "FOR EACH ROW EXECUTE FUNCTION audit_log_append_only()"
    )
    op.execute(
        "CREATE TRIGGER audit_log_no_truncate BEFORE TRUNCATE ON audit_log "
        "FOR EACH STATEMENT EXECUTE FUNCTION audit_log_append_only()"
    )


def downgrade() -> None:
    op.execute("DROP TRIGGER audit_log_no_truncate ON audit_log")
    op.execute("DROP TRIGGER audit_log_append_only ON audit_log")
    op.execute("DROP FUNCTION audit_log_append_only()")
    for table in (
        "audit_log",
        "used_call_tokens",
        "idempotency_keys",
        "channels",
        "config_contacts",
        "config_variables",
        "config_templates",
        "config_instances",
    ):
        op.drop_table(table)
