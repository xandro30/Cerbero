# Una Hécate de pruebas para el entorno de Hermes (research R21), compilada desde el repositorio.
FROM rust:1.99.1-bookworm AS build
WORKDIR /src
COPY hecate hecate
COPY shared shared
RUN cargo build --release --manifest-path hecate/Cargo.toml --bin hecate

FROM debian:bookworm-20261104-slim
RUN useradd --uid 10002 --create-home hecate \
    && mkdir -p /var/lib/hecate \
    && chown 10002:10002 /var/lib/hecate
COPY --from=build /src/hecate/target/release/hecate /usr/local/bin/hecate
USER 10002
ENTRYPOINT ["hecate"]
CMD ["serve"]
