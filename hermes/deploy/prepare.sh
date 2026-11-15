#!/bin/sh
# Prepara el entorno de pruebas de Hermes: las contraseñas de PostgreSQL en .secrets/, fuera de Git (research R21).
set -eu
cd "$(dirname "$0")"
umask 077
mkdir -p .secrets
for name in postgres_admin_password postgres_hermes_password; do
  if [ -s ".secrets/$name" ]; then
    echo "kept .secrets/$name"
  else
    LC_ALL=C tr -dc 'A-Za-z0-9' < /dev/urandom | head -c 32 > ".secrets/$name"
    echo "created .secrets/$name"
  fi
done
