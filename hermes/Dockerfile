# Imagen de pruebas de Hermes (research R21): la del E2E y el quickstart, no fija cómo corre en el home lab.
FROM python:3.15.2-slim-trixie

COPY --from=ghcr.io/astral-sh/uv:0.13.1 /uv /uvx /bin/

ENV UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/opt/hermes \
    PATH=/opt/hermes/bin:$PATH

WORKDIR /src
COPY . .
RUN uv sync --frozen --no-dev --no-editable --package cerbero-hermes

RUN useradd --uid 10001 --create-home hermes \
    && mkdir -p /run/hermes /run/hermes-admin /var/lib/hermes/state /var/lib/hermes/proxy \
    && chown -R 10001:10001 /run/hermes /run/hermes-admin /var/lib/hermes

USER 10001
ENTRYPOINT ["hermes"]
CMD ["serve"]
