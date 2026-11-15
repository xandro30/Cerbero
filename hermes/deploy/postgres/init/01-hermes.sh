#!/bin/sh
# Crea el usuario y la base de Hermes en el PostgreSQL de pruebas, sin acceso a las demás bases (ADR 0012).
set -eu
password="$(cat /run/secrets/postgres_hermes_password)"
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname postgres --set hermes_password="$password" <<'SQL'
REVOKE CONNECT ON DATABASE postgres FROM PUBLIC;
REVOKE CONNECT ON DATABASE template1 FROM PUBLIC;
CREATE ROLE hermes LOGIN PASSWORD :'hermes_password';
CREATE DATABASE hermes OWNER hermes;
REVOKE CONNECT ON DATABASE hermes FROM PUBLIC;
GRANT CONNECT ON DATABASE hermes TO hermes;
SQL
