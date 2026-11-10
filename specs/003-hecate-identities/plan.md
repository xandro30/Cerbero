# Implementation Plan: Identidades de Hécate

**Branch**: `feature/003-hecate-identities` | **Date**: 2026-11-09 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-hecate-identities/spec.md`

## Summary

Hécate, ya en Rust (cambio 005), gana su CA interna y su primera API entre componentes.

- **CA en dos niveles.** Se crea en una ceremonia por el canal local en la que la clave de la CA raíz solo existe en la
  CLI y sale como una copia protegida con la frase de la CA. La copia usa un sobre propio con Argon2id y AES-256-GCM, y
  se confirma antes de activar la CA. La intermedia queda en Hécate, cifrada bajo la raíz de cifrado (R2, R7, R9).
- **API mTLS.** axum sobre hyper, con rustls (solo TLS 1.3) y aws-lc-rs. Cada llamada lleva certificado SPIFFE, que
  valida la aplicación, y un token corto EdDSA ligado al canal (R1, R3). Cubre:
  - el alta con un token de un solo uso;
  - la renovación única con detección de reutilización;
  - la CRL X.509 válida 5 minutos;
  - las claves de firma de llamadas, en una lista de permitidas y por `kid`.

  El contrato es OpenAPI en JSON en `shared/contracts` (R4, R5, R11).
- **Administración.** Las operaciones del admin van por el canal local y la CLI: ceremonias, tokens, identidades,
  revocaciones y el bloque `ca` de `status` (R9, R17).
- **Modelo.** Event Sourcing en los aggregates Key de la CA, Identity y Certificate, con el material fuera de los
  eventos (R10).
- **Garantías del cambio 005:** secretos de paso que se borran (R14) y entradas en su formato estándar.
- **Protecciones nuevas:** un freno de altas con su propio contador (R13), la destrucción de la clave de la intermedia
  anterior (R2) y la revocación urgente del certificado propio de Hécate, que se reemite solo (R8).

La pila queda en el [ADR 0009](../../docs/adr/0009-pila-api-hecate.md). Las decisiones del propietario durante el plan
están en [research R0](research.md#r0-decisiones-del-propietario).

## Technical Context

**Language/Version**: Rust 1.99.1, edition 2024, como el cambio 005 (ADR 0005 y regla de versiones del ADR 0007)

**Primary Dependencies**:
- Las del cambio 005 (ADR 0007).
- Las del [ADR 0009](../../docs/adr/0009-pila-api-hecate.md) en su última versión estable:
  - axum 0.8.9 sin features por defecto, hyper 1.12.0, hyper-util 0.1.21 y tower 0.5.3;
  - tokio-rustls 0.26.6 y rustls 0.23.46, con aws-lc-rs, sin `tls12`, con `prefer-post-quantum` y con TLS 1.3 fijada
    en la configuración;
  - rcgen 0.14.10 (`aws_lc_rs`, `pem`, `x509-parser`, `zeroize`) y x509-parser 0.18.1 (`verify-aws`);
  - jsonwebtoken 11.1.0 (`aws_lc_rs`).
- De desarrollo: hyper-rustls 0.27.10 sin features por defecto, jsonschema 0.61.0 solo con `resolve-file` y rustls con
  `tls12`, solo para el cliente TLS 1.2 de las pruebas (R1, R16, R20).

**Configuration**: las variables del cambio 005 más `HECATE_API_ADDR` (`127.0.0.1:8443`), `HECATE_CERT_LIFETIME_HOURS`
(72, de 1 a 72) y `HECATE_ENROLL_TOKEN_MINUTES` (15, de 1 a 60) (R15). Ninguna lleva secretos

**Storage**: SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)), con la migración del cambio 005
ampliada (tablas con `CREATE TABLE IF NOT EXISTS` y columnas que faltan):
- streams `ca`, `identity/<nombre>` y `certificate/<identidad>/<serie>`;
- tablas `ca_material`, `hecate_tls` y `enroll_attempts`;
- columnas `identity` y `certificate` en el registro de accesos.

Detalle en [data-model.md](data-model.md).

**Testing**: cargo test, proptest y assert_cmd (ADR 0006), con un componente de prueba sobre hyper-rustls y pruebas de
contrato con jsonschema; calidad con `cargo fmt`, `cargo clippy` con los avisos como errores y cargo-deny; macOS y un
contenedor Linux (FR-031)

**Target Platform**: Linux (servidor de casa) y macOS (desarrollo)

**Project Type**: servicio local con CLI de administración y una API mTLS interna en el mismo proceso

**Performance Goals**: un alta o una renovación en menos de 1 segundo con Hécate desbloqueada (SC-001, SC-010), y lo
mismo para las claves publicadas y la lista de revocación con un año de datos (unos 5.400 certificados, R10). Las
ceremonias tardan lo que el perfil de Argon2id (una pasada en Hécate y dos en la CLI)

**Limits**:
- cuerpos de la API de hasta 64 KiB;
- token corto de 300 s como mucho, con 30 s de margen;
- lo publicado vale 300 s;
- tokens de alta de 1 a 60 minutos y certificados de 1 a 72 horas;
- una ceremonia a la vez, que caduca a los 15 minutos;
- el freno de altas se activa a los 10 fallos seguidos.

**Constraints**:
- La clave de la CA raíz nunca entra en el proceso de Hécate (FR-002).
- La TLS solo comprueba que el cliente tiene la clave de su certificado; la aplicación valida la cadena, la caducidad y
  la revocación, y cada rechazo da `auth` con su fila (R1, SC-006).
- Los secretos de paso viven en tipos que se borran al soltarse (FR-028).
- Las entradas siguen su formato estándar (FR-029).
- 001 y 002 no cambian, salvo FR-027 (FR-030).
- `unsafe_code = "forbid"`.
- Ningún secreto en claro en disco, en eventos ni en logs, salvo la clave del certificado propio de Hécate (riesgo
  aceptado).

**Scale/Scope**: una instancia y decenas de identidades; 7 historias, 34 requisitos y 18 criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Hécate sigue siendo su propio workspace de Cargo. El lado del componente lo implementa cada uno contra el contrato; el componente de prueba vive en las pruebas de Hécate. Solo se comparte `shared/contracts` |
| II. Contrato primero | El contrato de la API está escrito antes del código ([contracts/api.md](contracts/api.md)) y se publica como OpenAPI en `shared/contracts`, con `/capabilities`, la versión en la ruta e `Idempotency-Key`. El canal local usa el sobre y los códigos comunes. `common.v1.json` recoge el sobre y los nueve códigos comunes, sin estado HTTP, que fija cada API; el código `unavailable` se añade al contrato común, la revisión que pide el principio (R11) |
| III. Hexagonal con intensidad justificada | Las capas `domain / application / infrastructure` siguen; la PKI y el token corto van detrás de ports. Event Sourcing en Key, Identity y Certificate, como fija ARCHITECTURE para Hécate, y sin CQRS (R10) |
| IV. Pruebas primero | TDD en unidad, propiedades, aplicación, integración de la API con un componente de prueba, contrato (de la API y de cada operación nueva del canal local) y CLI; la suite, también en Linux (R16) |
| V. Seguridad por diseño | mTLS con SPIFFE y token corto ligado al canal en toda la API, salvo el alta (excepción de arranque, en Complexity Tracking). La clave de la CA raíz, solo en la CLI; secretos de paso que se borran; nada secreto en eventos ni en logs. La clave del certificado propio, legible sin desbloquear, se registra como riesgo aceptado en DECISIONS al consolidar (FR-032) |
| VI. Degradación elegante | Con Hécate sellada o caída, los componentes conservan sus certificados y su última copia de la CRL y las claves. `unavailable` se puede reintentar; cada cambio es atómico ante un corte |
| VII. Referencia | step-ca y SPIFFE. Las desviaciones (renovación única con detección de reutilización, claves solo para componentes autenticados, sustituir la CA, alta sin certificado de cliente) se registran en DECISIONS al consolidar (R19) |
| Lenguajes | Rust, edition 2024 (ADR 0005) |
| Herramientas | Crates seguras y maduras del ADR 0009, todas sobre aws-lc-rs; sin C propio ni bindings a mano |
| Licencia | Los crates declaran `AGPL-3.0-or-later`; las dependencias nuevas pasan por la política del ADR 0008 (R20) |
| Nueva tecnología vía ADR | El ADR 0009, escrito en este plan antes de usarla: framework HTTP, TLS y PKI, token corto, formato del contrato y validación de contratos |
| Operación | Logs estructurados a stdout y configuración por entorno sin secretos. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `feature/003-hecate-identities`; squash `feat:` |
| Una cosa a la vez | Es el único cambio en curso. Su fila en «Cambios pausados» la retira la PR que lo fusiona (FR-033) |
| Consolidación | Los docs se actualizan al cerrar, con la lista de R19 (FR-032) |

**Resultado:** pasa, con las excepciones justificadas en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** el modelo añade tres tablas fuera de los eventos, justificadas, y reconstruye el estado leyendo streams,
  sin proyecciones.
- **Ceremonia:** el protocolo mantiene la clave de la CA raíz fuera de Hécate. Todo lo pendiente vive en memoria y caduca,
  así que una ceremonia interrumpida no deja nada (FR-004).
- **Contrato:** la API cumple el contrato común; su única ampliación, `unavailable`, se escribe en `shared/contracts`
  junto al sobre y los códigos que ya eran comunes, también los de Hermes (`provider_unavailable` y `timeout`).
- **Decisiones abiertas:** ninguna se resuelve de forma implícita. El acceso al certificado del cliente en las APIs en
  Python sigue abierto para Hermes; la firma de Policies de Orthros no se toca.

## Project Structure

### Documentation (this feature)

```text
specs/003-hecate-identities/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   ├── README.md        # Qué contratos hay y dónde se publican
│   ├── api.md           # API mTLS v1 y sobre común
│   ├── control-protocol.md  # Operaciones nuevas del canal local
│   ├── cli.md           # Órdenes nuevas
│   └── formats.md       # Token de alta, copia de la CA raíz, perfiles, CRL, claves, token corto y huella
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
docs/adr/0009-pila-api-hecate.md       # Escrito en este plan
shared/contracts/
├── README.md                          # Dónde están los OpenAPI (consolidación)
└── src/cerbero_contracts/openapi/     # Dentro del módulo, para que viajen con el paquete
    ├── common.v1.json                 # Sobre común y códigos de error
    └── hecate.v1.json                 # API de Hécate v1
hecate/
├── Cargo.toml                         # Dependencias del ADR 0009
├── deny.toml                          # Duplicados nuevos, si hay que justificarlos
└── crates/
    ├── domain/src/
    │   ├── identity/                  # Nombres y SPIFFE ID, token de alta, Identity, Certificate, clave de firma de llamadas, freno de altas
    │   ├── ca/                        # Key de la CA (generaciones e intermedias) y formato de la copia de la clave de la CA raíz
    │   ├── secrets/path.rs            # Identidades reservadas (FR-027)
    │   └── events.rs                  # Eventos nuevos
    ├── application/src/
    │   ├── ports.rs                   # PkiProvider, CallTokenVerifier y almacenes de identidades, CA y freno
    │   ├── ca/                        # Ceremonias, certificado propio y retiro de intermedias
    │   ├── identity/                  # Tokens, alta, renovación, revocación, identidades, claves, CRL y recuentos
    │   └── testing.rs                 # Dobles nuevos
    ├── infrastructure/
    │   ├── src/
    │   │   ├── pki.rs                 # rcgen y x509-parser
    │   │   ├── call_token.rs          # jsonwebtoken
    │   │   ├── backup.rs              # Sobre de la copia: Argon2id y AES-256-GCM
    │   │   ├── api/                   # TLS con verificador de posesión, supervisor, bucle de aceptación, router, autenticación, sobre y errores
    │   │   ├── store/                 # Migración, streams y tablas nuevas
    │   │   ├── control/operations.rs  # Operaciones nuevas del canal
    │   │   ├── config.rs              # Variables nuevas
    │   │   ├── tasks.rs               # Tareas de cada minuto: certificado propio, retiro de intermedias, freno y jti
    │   │   └── testing.rs             # TestComponent
    │   └── examples/component.rs      # Componente de prueba del quickstart
    └── cli/src/                       # Órdenes ca, tokens, identities y certificates
```

Cada crate lleva sus pruebas como en el cambio 005: las unitarias junto al código y las de integración y contrato en su
`tests/`. Las de aplicación van en `crates/application/tests/`, con la infraestructura como dependencia de desarrollo.

**Structure Decision:** los cuatro crates del cambio 005, con módulos nuevos por concepto del dominio (identidad y CA).
La API es un adaptador de infraestructura, como el canal local; los casos de uso y los ports viven en la aplicación.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS (clarify 2026-11-09). El motivo de 001 y 002, que no había llamadas entre componentes, ya no vale | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
| El alta, sin mTLS ni token corto (principio V) | El componente aún no tiene certificado ni clave registrada. Lo autentica un token de alta de un solo uso, ligado a su identidad, con la huella de la CA raíz y con freno de intentos | No hay ninguna credencial previa que presentar, y un certificado de arranque compartido sería un secreto común |
| Material y estado fuera del event store | Como en 002: destruir la clave de una intermedia exige borrarla (borrado criptográfico), la clave propia de Hécate se lee sin desbloquear y el freno es un contador | En eventos append-only no se podrían destruir, y el freno crecería con cada intento |
