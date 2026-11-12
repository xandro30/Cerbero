# Implementation Plan: Access Policy de Hécate y resolución de secretos

**Branch**: `feature/006-hecate-access-policy` | **Date**: 2026-11-10 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/006-hecate-access-policy/spec.md`

## Summary

Los componentes pueden por fin leer sus secretos, y la Access Policy decide qué lee cada uno.

- **Resolución por la API.** `POST /v1/secrets:resolve`, con la autenticación de la feature 003. Las reglas son las de
  la resolución interna de la 002: todo o nada, versiones y un único `not_found`. Se comprueban todas las referencias
  antes de descifrar ninguna, y todo `not_found` se responde con un tiempo mínimo de 250 ms, para que los tres casos no
  se distingan por el tiempo (R1, R2).
- **Access Policy.** Un aggregate event-sourced por identidad (`policy/<identidad>`) con las excepciones de lectura por
  secreto concreto y el permiso de estado. La regla por defecto, leer solo lo suyo, no se guarda. El admin la cambia y
  la consulta por el canal local y la CLI, con órdenes sin versión esperada (R3, R7).
- **Ámbito `status`.** `GET /v1/status` da los recuentos agregados, sin nombres, a las identidades con el permiso,
  también con Hécate sellada (R4).
- **`rotate_every` como duración ISO 8601** en días o semanas, para contar los secretos que deben rotarse. Es el único
  cambio de comportamiento de la feature 002 (R5).
- **Memoria.** El valor resuelto viaja hasta el socket en búferes que se borran (R6).

Las decisiones del propietario están en [research R0](research.md#r0-decisiones-del-propietario). No hace falta
tecnología nueva (R13).

## Technical Context

**Language/Version**: Rust 1.99.1, edition 2024, como las features anteriores (ADR 0005 y ADR 0007)

**Primary Dependencies**: las del ADR 0007 y el ADR 0009, sin ninguna nueva: `axum::body::Bytes` es `bytes` 1.12, que
ya tiene `from_owner`, y la duración la lee código propio (R13)

**Configuration**: sin variables nuevas. El tiempo mínimo de los rechazos es un ajuste de `IdentitySettings` (250 ms
por defecto) que las pruebas pueden bajar

**Storage**: SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)), con el event store de las
features anteriores: el stream nuevo `policy/<identidad>`. Sin tablas nuevas ni migración
([data-model.md](data-model.md))

**Testing**: cargo test, proptest y assert_cmd (ADR 0006), con el componente de prueba de la feature 003 y pruebas de
contrato con jsonschema; calidad con `cargo fmt`, `cargo clippy` con los avisos como errores y cargo-deny; macOS y un
contenedor Linux (FR-021)

**Target Platform**: Linux (servidor de casa) y macOS (desarrollo)

**Project Type**: servicio local con CLI de administración y una API mTLS interna en el mismo proceso

**Performance Goals**: una resolución de hasta 10 secretos en menos de 1 segundo (SC-001); la comprobación de 100
referencias, por debajo de una quinta parte del tiempo mínimo de 250 ms (50 ms)

**Limits**:
- hasta 100 referencias por petición;
- tiempo mínimo de 250 ms para un `not_found` de la resolución;
- `rotate_every` de 1 a 3650 días o de 1 a 520 semanas;
- tiempos de caché de 300 s y 3600 s, como en la feature 002.

**Constraints**:
- Los valores resueltos son secretos de paso: tipos que se borran hasta el socket (FR-016, R6).
- Los tres rechazos de la resolución son iguales en contenido y en tiempo (FR-003, FR-004).
- El estado nunca nombra secretos ni identidades (FR-011).
- 001, 002 y 003 no cambian, salvo `rotate_every` (FR-020, FR-023).
- `unsafe_code = "forbid"`.
- Ningún valor, huella, clave privada ni token en disco, en eventos ni en logs.

**Scale/Scope**: una instancia, decenas de identidades y de excepciones; 3 historias, 23 requisitos y 10 criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Hécate sigue siendo su propio workspace de Cargo. El lado del componente (caché y refresco) lo implementa cada uno contra el contrato; aquí lo cubre el componente de prueba. Solo se comparte `shared/contracts` |
| II. Contrato primero | El contrato está escrito antes del código ([contracts/api.md](contracts/api.md)) y se publica en el OpenAPI de `shared/contracts`, con el sobre común, `/capabilities` y la versión en la ruta. La resolución no lleva `Idempotency-Key` porque es una consulta sin efecto. Las operaciones nuevas del canal local usan el sobre y los códigos comunes |
| III. Hexagonal con intensidad justificada | La Access Policy es un aggregate event-sourced en el dominio, como fija ARCHITECTURE para Hécate (Secret, Key, Certificate, Identity, Access Policy), sin CQRS. Los secretos no conocen las políticas: la resolución recibe el permiso de fuera (R1) |
| IV. Pruebas primero | TDD en dominio, aplicación, API con el componente de prueba, contrato y CLI, más la prueba de tiempos de SC-003 y la suite en Linux (R10) |
| V. Seguridad por diseño | Resolución y estado con mTLS SPIFFE y token corto ligado al canal. Denegado por defecto, con excepciones explícitas y auditadas. El estado va sin nombres. Los rechazos no revelan nada ni por contenido ni por tiempo. Los secretos de paso se borran, y nada secreto queda en eventos ni en logs. Cada cambio de política queda atribuido al admin |
| VI. Degradación elegante | Sellada, la resolución responde `unavailable` (se puede reintentar) y el estado sigue respondiendo. Los componentes siguen con su caché hasta su tiempo duro. Cada cambio de política es atómico ante un corte |
| VII. Referencia | OpenBao / Vault: denegar por defecto y políticas por ruta. La diferencia, excepciones por secreto concreto sin comodines y sin políticas ligadas a tokens, se registra en DECISIONS al consolidar (R11) |
| Lenguajes | Rust, edition 2024 (ADR 0005) |
| Herramientas | Sin crates nuevos; código propio solo en Rust |
| Licencia | Sin dependencias nuevas que revisar (ADR 0008) |
| Nueva tecnología vía ADR | No hay tecnología nueva. El ADR 0010 que se escribe al cerrar recoge la decisión abierta que esta feature resuelve |
| Operación | Logs estructurados a stdout y configuración por entorno sin secretos. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `feature/006-hecate-access-policy`; squash `feat:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Consolidación | Los docs se actualizan al cerrar con la lista de R11 (FR-022) |

**Resultado:** pasa, con la excepción justificada en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** el modelo añade un stream de eventos y ninguna tabla. El historial de la política sale de sus eventos.
- **Contrato:** las dos operaciones nuevas siguen el contrato común. El cambio de `rotate_every` está explícito en la
  spec (FR-023) y en el contrato del canal.
- **Decisiones abiertas:** solo se resuelve la de la Access Policy y la resolución, que es el objeto de esta feature y
  pasa a un ADR al cerrar. Los permisos que salen de las declaraciones de Hermes y Hades siguen fuera.

## Project Structure

### Documentation (this feature)

```text
specs/006-hecate-access-policy/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   ├── README.md        # Qué contratos hay y dónde se publican
│   ├── api.md           # Resolución y estado en la API mTLS v1
│   ├── control-protocol.md  # Operaciones policies.*, status y rotate_every
│   ├── cli.md           # Órdenes hecate policies
│   └── formats.md       # Referencias, rotate_every y secreto resuelto
├── checklists/
│   ├── requirements.md
│   └── review.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
docs/adr/0010-access-policy-hecate.md   # Al cerrar: la Access Policy y la resolución (R11)
shared/contracts/src/cerbero_contracts/openapi/
└── hecate.v1.json                      # resolveSecrets y getStatus
hecate/crates/
├── domain/src/
│   ├── access_policy/                  # AccessPolicy: eventos, estado, permisos de lectura y de estado
│   ├── secrets/rotation.rs             # RotationInterval: rotate_every como duración
│   └── events.rs                       # Eventos de la política
├── application/src/
│   ├── access_policy/                  # AccessPolicyService: órdenes y consultas del admin, política de una identidad
│   ├── identity/service.rs             # resolve y status de la API: autenticación y filas
│   ├── secrets/service.rs              # Resolución en dos fases con el permiso de fuera; rotate_every; recuento de rotación
│   └── testing.rs                      # Dobles
├── infrastructure/
│   ├── src/api/routes.rs               # POST /v1/secrets:resolve con el tiempo mínimo, GET /v1/status y operation_of
│   ├── src/api/server.rs               # La guarda de sellada deja pasar el estado
│   ├── src/api/supervisor.rs           # Pasa al servidor la guarda por ruta
│   ├── src/api/envelope.rs             # Respuesta que se borra al enviarse
│   ├── src/control/operations.rs       # policies.* y el status ampliado
│   ├── src/testing.rs                  # TestComponent: resolve y status
│   └── examples/component.rs           # resolve y status
└── cli/src/lib.rs                      # hecate policies y la línea nueva de status
```

Cada crate lleva sus pruebas como en las features anteriores: las unitarias junto al código y las de integración y
contrato en su `tests/`.

**Structure Decision:** los cuatro crates de siempre, con un módulo nuevo por concepto del dominio (`access_policy`,
como el que ya prevé docs/hecate.md). La API sigue siendo un adaptador de infraestructura, y los casos de uso viven en
la aplicación.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
