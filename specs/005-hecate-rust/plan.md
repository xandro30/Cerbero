# Implementation Plan: Hécate en Rust: migración con paridad de las features 001 y 002

**Branch**: `refactor/hecate-rust` | **Date**: 2026-11-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/005-hecate-rust/spec.md`

## Summary

Hécate se reescribe en Rust en un workspace de Cargo en `hecate/`, con un crate por capa (dominio, aplicación,
infraestructura y la CLI), sin cambiar lo que hace. El canal local, la CLI, la configuración y el modelo de datos no
cambian, salvo las partes de recuperación, que pasan al formato `HEC2` (Shamir en GF(256)), y las entradas que no siguen
su formato estándar (el JSON del canal y del tipo `json`, los PEM y `expires_at`), que se rechazan (FR-018). Lo cifrado y
el esquema se conservan byte a byte, y el JSON guardado, en contenido.

La paridad se demuestra de cuatro maneras:

- trasladando las 346 funciones de prueba de Python caso a caso, con un mapeo (research R15);
- con vectores de prueba generados con la versión en Python antes de borrarla y con el esquema de SQLite comparado (R5);
- con los quickstarts de 001 y 002 traducidos a la nueva CLI ([quickstart.md](quickstart.md));
- con la suite en macOS y en un contenedor Linux (R16).

Los secretos de paso viven en tipos que se borran al soltarse (R14). Al final se borra la Hécate en Python y su miembro
del workspace de uv, y se consolidan el README, el doc de Hécate y DECISIONS (R18). Las decisiones que tomó el
propietario durante el plan están en R0.

## Technical Context

**Language/Version**: Rust 1.99.1, edition 2024, fijado en `hecate/rust-toolchain.toml` (ADR 0005 y regla de versiones
del ADR 0007)

**Primary Dependencies**: las del [ADR 0007](../../docs/adr/0007-pila-hecate-rust.md) en su última versión estable:
aws-lc-rs 1.19.0, libsodium-rs 0.3.1, vsss-rs 6.0.1, tokio 1.54.1, rusqlite 0.41.1 (`bundled`), clap 4.6.7, tracing
0.1.44 y tracing-subscriber 0.3.23, serde 1.0.229 y serde_json 1.0.154, thiserror 2.0.21, zeroize 1.9.1,
data-encoding 2.11.1 y nix 0.31.3. Fuera del ADR, justificadas en R2 y R6: time 0.3.55 y rand_core 0.10.1

**Configuration**: las variables de la versión en Python, sin cambios: `HECATE_DATA_DIR` (obligatoria),
`HECATE_SOCKET_PATH` (por defecto, `control.sock` en el directorio de datos) y `HECATE_KDF_PROFILE` (`sensitive` por
defecto o `moderate`)

**Storage**: SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)) con el mismo esquema, los mismos
disparadores append-only y los mismos pragmas (`synchronous = FULL`, `secure_delete = ON`) que la versión en Python

**Testing**: cargo test, proptest 1.12.0 y assert_cmd 2.2.2 con predicates 3.1.4 y tempfile 3.27.0
([ADR 0006](../../docs/adr/0006-pruebas-rust-cargo-proptest.md)); calidad con `cargo fmt`, `cargo clippy` con los avisos
como errores y cargo-deny 0.20.2

**Target Platform**: Linux (servidor de casa) y macOS (desarrollo); la suite pasa en los dos, con Linux en un contenedor
`rust:1.99.1-bookworm` (FR-017)

**Project Type**: servicio local con CLI de administración (el de 001 y 002), ahora un workspace de Cargo

**Performance Goals**: los de 001 y 002 (SC-003): desbloquear con la frase en menos de 1 minuto y resolver 10 secretos en
menos de 1 segundo con Hécate desbloqueada

**Limits**: los de 001 y 002, sin cambios (petición de hasta 1 MiB, valor de hasta 64 KiB, `max_versions` de 2 a 100,
`rotate_every` de 1 a 64 caracteres imprimibles, hasta 100 referencias por resolución)

**Constraints**: lo cifrado, los datos asociados y el esquema, idénticos byte a byte, y el JSON guardado, en contenido,
salvo las partes (FR-006); respuestas del canal equivalentes y códigos de salida idénticos (FR-003, FR-004); entradas en
su formato estándar: JSON en el canal y en el tipo `json`, PEM y `expires_at` (FR-018); secretos de paso en tipos que se
borran al soltarse (FR-007); `unsafe_code = "forbid"`; ningún secreto en disco en claro, en eventos ni en logs

**Scale/Scope**: unas 3.160 líneas de Python y 346 funciones de prueba en 46 ficheros que trasladar; seis tablas; cuatro
crates

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Hécate es su propio workspace de Cargo; no depende del código de ningún otro componente |
| II. Contrato primero | Los contratos del canal de 001 y 002 no cambian; el único contrato que cambia, el formato de las partes, está en [contracts/recovery-share.md](contracts/recovery-share.md) antes del código. Es un canal local de administración: sobre y códigos comunes, sin API Contracts, `/capabilities` ni `Idempotency-Key` (constitución, principio II) |
| III. Hexagonal con intensidad justificada | `domain / application / infrastructure` como crates, con la regla de dependencia comprobada por Cargo (R1); Event Sourcing en `Secret` y `Key`, como fija la tabla de intensidad de ARCHITECTURE para Hécate |
| IV. Pruebas primero | Cada prueba trasladada se escribe y se ve fallar antes de implementar; propiedades con proptest; vectores y esquema como pruebas de formato (R5, R15) |
| V. Seguridad por diseño | Ningún secreto en disco en claro, en eventos ni en logs; secretos de paso que se borran al soltarse (R14); la raíz solo en memoria, sin volcados ni swap (R12); el canal solo atiende al usuario del proceso (R9) |
| VI. Degradación elegante | Siempre arranca sellada; las mismas garantías de atomicidad ante cortes de luz que en 001 y 002 |
| VII. Referencia | Sin cambios: OpenBao / Vault, step-ca y SPIFFE, como recoge DECISIONS |
| Lenguajes | Rust (edition 2024) para Hécate (ADR 0005) |
| Herramientas | Crates seguras y maduras del ADR 0007; las dos que no están en él se justifican en R2 y R6; la tabla de Unicode es código propio en Rust (R7); sin C propio ni bindings a mano |
| Licencia | Todos los crates declaran `AGPL-3.0-or-later`; cargo-deny admite la licencia de OpenSSL solo donde la cubre el ADR 0008 (R17) |
| Nueva tecnología vía ADR | La pila está en los ADRs 0006 y 0007; no entra tecnología nueva (broker, persistencia, framework ni formato de contratos) |
| Operación (OpenTelemetry) | Excepción justificada en Complexity Tracking, la misma que en 001 y 002 |
| Orden y ramas | Ciclo completo en `refactor/hecate-rust`; squash `refactor:` |
| Una cosa a la vez | La feature 003 está pausada y registrada en DECISIONS; este es el único cambio en curso |
| Consolidación | Los docs se actualizan al final, cuando la versión en Rust cumple la paridad (R18) |

**Resultado:** pasa, con las excepciones heredadas de 001 y 002 justificadas.

**Revisión tras el diseño (fase 1):** sigue pasando. El modelo de datos y los contratos del canal no cambian: el JSON
estándar de FR-018 es el que ya pide el contrato de 001 («JSON UTF-8»), y los PEM y `expires_at` siguen sus estándares.
El único contrato nuevo, `HEC2`, conserva las reglas de 001. Las dependencias fuera del ADR 0007 son dos utilidades
maduras (R2, R6), y los vectores y el mapeo dan a `/speckit-converge` evidencia de la paridad.

## Project Structure

### Documentation (this feature)

```text
specs/005-hecate-rust/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: validación y pasos traducidos de 001 y 002
├── contracts/
│   ├── README.md        # Qué contratos se conservan y dónde están
│   ├── cli.md           # Órdenes, opciones, salidas y códigos de salida
│   └── recovery-share.md  # Formato HEC2
├── vectors/
│   └── generate.py      # Generador de vectores con la versión en Python (implementación)
├── test-mapping.md      # Mapeo de las 346 pruebas (implementación)
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
hecate/
├── Cargo.toml               # workspace: crates, dependencias, lints y metadatos comunes
├── Cargo.lock
├── rust-toolchain.toml      # Rust 1.99.1
├── deny.toml                # cargo-deny: licencias (ADR 0008), avisos y duplicados
├── README.md                # puntero al doc de Hécate (se queda)
├── crates/
│   ├── domain/              # hecate-domain
│   │   └── src/
│   │       ├── sealing/     # estado, frase, parte HEC2, intentos, registro de accesos, protección, errores
│   │       ├── secrets/     # ruta, tipo de valor, aggregate Secret
│   │       ├── keys/        # aggregate Key
│   │       ├── events.rs    # eventos y su serialización
│   │       ├── instant.rs   # isoformat y lectura en ISO 8601 extendido (R6)
│   │       ├── printable.rs # regla de str.isprintable
│   │       └── printable_table.rs  # rangos de Unicode 17, generados por el script de R5
│   ├── application/         # hecate-application
│   │   └── src/
│   │       ├── ports.rs     # CryptoProvider, repositorios, registro de accesos, reloj
│   │       ├── sealing/     # servicio, guardián de la raíz, ajustes
│   │       ├── secrets/     # servicio, sobre (cifrado por capas y huella)
│   │       └── testing.rs   # dobles en memoria (fakes.py)
│   ├── infrastructure/      # hecate-infrastructure
│   │   └── src/
│   │       ├── crypto.rs    # aws-lc-rs, libsodium y vsss-rs
│   │       ├── store/       # SQLite
│   │       ├── control/     # protocolo, servidor tokio, operaciones y cliente
│   │       ├── clock.rs, config.rs, logs.rs, process.rs, signals.rs
│   │       └── testing.rs   # ayudantes de integración (short_dir, ServerThread y SqliteStack)
│   └── cli/                 # hecate-cli: biblioteca con las órdenes y binario `hecate`
└── tests/
    └── vectors/             # JSON generados con la versión en Python (R5)
```

Se borran `hecate/src/` (Python), `hecate/tests/` (Python, salvo `tests/vectors/`, que es nuevo) y
`hecate/pyproject.toml`; `pyproject.toml` deja de listar `hecate` y `uv.lock` se regenera. Cada crate lleva sus pruebas:
unitarias junto al código y de integración y contrato en su `tests/`.

**Structure Decision**: un crate por capa, con el binario `hecate` en `hecate-cli` (R1). Los nombres de los módulos siguen
a los de la versión en Python para que el mapeo de pruebas sea directo.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | Igual que en 001 y 002: no hay llamadas entre componentes y el colector sigue abierto | Se incorpora con la API mTLS de la feature 003 |
| Material cifrado fuera del event store | Igual que en 002: el borrado criptográfico exige eliminar la clave de una versión, y el event store es append-only | Guardarlo en los eventos haría imposible destruir una versión |
