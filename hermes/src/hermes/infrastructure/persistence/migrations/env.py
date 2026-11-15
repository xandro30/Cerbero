"""Alembic runs the migrations on the connection that `apply_migrations` hands it."""

from alembic import context

from hermes.infrastructure.persistence.schema import metadata

context.configure(connection=context.config.attributes["connection"], target_metadata=metadata)
with context.begin_transaction():
    context.run_migrations()
