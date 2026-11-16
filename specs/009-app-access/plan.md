# Implementation Plan: Lista de acceso por app

**Branch**: `feature/009-app-access` | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/009-app-access/spec.md`

## Summary

Hécate guarda quién puede llamar a cada app, cada app se lo pregunta, y Hermes, la primera, lo aplica.

- **En Hécate, la lista de cada app.** Un aggregate event-sourced nuevo, App Access, por app (`access/<app>`), con los
  eventos de conceder y retirar, su versión y su historial, como la Access Policy. El admin lo cambia y lo consulta por
  el canal local (`access.*`) y la CLI (`hecate access`), con órdenes sin versión esperada (R1, R2).
- **La consulta.** `GET /v1/callers` da a una app autenticada su lista y su versión, solo la suya; con Hécate sellada
  responde `unavailable`, y atendida no deja fila (R3).
- **En Hermes, la comprobación.** Tras el certificado y el token, la identidad de quien llama tiene que estar en su
  copia de la lista, o recibe `auth`. Si no está, Hermes vuelve a pedir la lista antes de responder, una petición a la
  vez, así que una concesión vale en la llamada siguiente. La copia se refresca con la lista de revocación y las claves
  cada 240 s, se guarda en el directorio de estado y, sin ninguna, Hermes no está listo (R4, R5).
- **Contratos.** `hecate.v1.json` gana `listCallers` y pasa a la 1.1.0; el sobre común no cambia (R7).

Las decisiones del propietario están en [research R0](research.md#r0-decisiones-del-propietario). No hace falta
tecnología nueva (R11).

**Alineado con la spec:** las identidades hijas están diseñadas pero no construidas, así que la regla «una hija cuenta
como su app» queda como regla de diseño, y FR-011 y la historia 3 se alinearon al revisar el plan
([research R10](research.md#r10-identidades-hijas)).

## Technical Context

**Language/Version**: Rust 1.99.1, edition 2024, en Hécate (ADR 0005 y ADR 0007); Python 3.15 con uv en Hermes
(ADR 0011)

**Primary Dependencies**: las de siempre, sin ninguna nueva: en Hécate, las del ADR 0007 y el ADR 0009; en Hermes, las
del ADR 0011 (R11)

**Configuration**: sin variables nuevas, ni en Hécate ni en Hermes

**Storage**:

- Hécate: SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)), con el event store de las features
  anteriores y el stream nuevo `access/<app>`; sin tablas nuevas ni migración ([data-model.md](data-model.md)).
- Hermes: la copia de su lista en un fichero del directorio de estado, como la lista de revocación y las claves; sin
  cambios en su base.

**Testing**:

- Hécate: cargo test, proptest y assert_cmd (ADR 0006), con el componente de prueba de la feature 003 y pruebas de
  contrato con jsonschema; `cargo fmt`, `cargo clippy` con los avisos como errores y cargo-deny; en macOS y en un
  contenedor Linux.
- Hermes: pytest e hypothesis (ADR 0004), con una Hécate real (`HECATE_BIN`), Docker y el E2E con Envoy; ruff, mypy
  en modo estricto y pip-audit.

**Target Platform**: Linux (servidor de casa, en contenedores) y macOS (desarrollo)

**Project Type**: dos componentes: un servicio local con CLI y API mTLS (Hécate) y un servicio con API detrás de un
proxy (Hermes)

**Performance Goals**: comprobar el acceso no añade una llamada a Hécate a quien está en la copia; una concesión vale
en la llamada siguiente y una retirada en 240 s como mucho (SC-003)

**Limits**:

- una lista por app, con decenas de identidades como mucho;
- el refresco de la copia en Hermes, cada 240 s, con las publicaciones de Hécate;
- una sola petición de la lista a la vez en Hermes cuando llega alguien que no está en la copia.

**Constraints**:

- Denegar por defecto: sin nadie en la lista, nadie entra (FR-010).
- El token corto no concede nada (FR-007).
- Una app solo ve su lista (FR-006).
- 001 a 008 no cambian, salvo lo que añade esta feature: en Hermes, la comprobación tras la autenticación, la copia
  entre las publicaciones y un motivo más de la salud.
- `unsafe_code = "forbid"` en Hécate y mypy estricto en Hermes.

**Scale/Scope**: una instancia de cada componente, pocas apps y decenas de identidades; 3 historias, 13 requisitos y 7
criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Hécate sigue siendo su workspace de Cargo y Hermes su miembro del workspace de uv. Hermes usa la lista por la API de Hécate, no por su código; solo se comparte `shared/contracts`. Otra app que llegue después usa la misma consulta |
| II. Contrato primero | Los contratos están escritos antes del código ([contracts/](contracts/README.md)): `listCallers` en el OpenAPI de Hécate, con el sobre común, la versión en la ruta y `/capabilities`; es una consulta sin efecto, así que no lleva `Idempotency-Key`. Las operaciones `access.*` del canal local usan el sobre y los códigos comunes |
| III. Hexagonal con intensidad justificada | App Access es un aggregate event-sourced en el dominio de Hécate, como sus otros cinco; lleva su fila en Complexity Tracking porque la tabla de ARCHITECTURE nombra los cinco. En Hermes, la comprobación vive en la aplicación (`authentication.py`) y la consulta en el adaptador de Hécate |
| IV. Pruebas primero | TDD en dominio, aplicación, canal local, CLI, API con el componente de prueba y contrato en Hécate, en macOS y en Linux; en Hermes, en aplicación, adaptador contra una Hécate real, API y el E2E con Envoy (R8) |
| V. Seguridad por diseño | Denegado por defecto y sin accesos de partida. El token corto no concede nada. Solo el admin cambia las listas, por el canal local, y cada cambio queda atribuido. La consulta es mTLS con token corto y solo da la lista de quien pregunta. Quien no está en la lista recibe `auth` sin pistas, y el motivo va solo al log |
| VI. Degradación elegante | Con Hécate sellada o caída, Hermes sigue con su última copia: nadie gana ni pierde el acceso. Sin ninguna copia, no está listo y responde `unavailable`, que vale reintentar. Si al buscar a alguien que no está Hécate no responde, `unavailable` |
| VII. Referencia | SecOps no tiene un equivalente para la autorización entre servicios. La referencia es la de las mallas de servicios: una política de autorización del servicio de destino que deja pasar a identidades SPIFFE concretas y deniega lo demás. DECISIONS la recoge al consolidar (R9) |
| Lenguajes | Rust, edition 2024, en Hécate y Python 3.15 en Hermes (ADR 0005) |
| Herramientas | Sin dependencias nuevas; en Hécate, código propio solo en Rust |
| Licencia | Sin dependencias nuevas que revisar (ADR 0008) |
| Nueva tecnología vía ADR | No hay tecnología nueva. La decisión ya está en el ADR 0013 |
| Operación | Logs estructurados a stdout y configuración por entorno sin secretos, sin variables nuevas. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `feature/009-app-access`; squash `feat:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados; la reestructuración de la documentación de Hermes espera a que se fusione |
| Consolidación | Los docs se actualizan al cerrar con la lista de R9 (FR-013) |

**Resultado:** pasa, con las excepciones justificadas en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** un stream de eventos más en Hécate, sin tablas, y un fichero más en el directorio de estado de Hermes. El
  historial de cada lista sale de sus eventos.
- **Contrato:** la operación nueva y las del canal local siguen el contrato común. `hecate.v1.json` sube a la 1.1.0
  porque solo añade, y su prueba de contrato lo fija.
- **Decisiones abiertas:** solo se construye lo que decidió el ADR 0013. Las identidades hijas, los permisos por
  función y el perfilado siguen fuera.

## Project Structure

### Documentation (this feature)

```text
specs/009-app-access/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   ├── README.md        # Qué contratos hay y dónde se publican
│   ├── api.md           # GET /v1/callers en Hécate y lo que cambia en la API de Hermes
│   ├── control-protocol.md  # Operaciones access.* del canal local de Hécate
│   └── cli.md           # Órdenes hecate access y el motivo nuevo de hermes health
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
shared/contracts/src/cerbero_contracts/openapi/
└── hecate.v1.json                      # listCallers; versión 1.1.0
hecate/crates/
├── domain/src/
│   ├── app_access/                     # AppAccess: eventos, estado, reglas de conceder y retirar
│   ├── events.rs                       # AccessEvent: CallerGranted y CallerRevoked
│   └── sealing/access_log.rs           # Operaciones access_* del admin y callers_list de la API
├── application/src/
│   ├── app_access/                     # AppAccessService: órdenes y consultas del admin, lista de una app
│   ├── identity/service.rs             # callers de la API: autenticación y filas de los rechazos
│   └── testing.rs                      # Dobles
├── infrastructure/
│   ├── src/api/routes.rs               # GET /v1/callers, operation_of y la lista de operaciones
│   ├── src/control/operations.rs       # access.*
│   ├── src/testing.rs                  # TestComponent: callers
│   ├── examples/component.rs           # callers
│   └── tests/contract_schema.rs        # hecate.v1.json 1.1.0 y listCallers
└── cli/src/lib.rs                      # hecate access
hermes/src/hermes/
├── application/
│   ├── ports.py                        # AccessList y la consulta en HecateApi, IdentityStore y TrustSource
│   ├── identity.py                     # La copia: refresco con las publicaciones, búsqueda ante quien no está
│   ├── authentication.py               # La comprobación tras el token
│   └── health.py                       # El motivo access list not loaded
├── infrastructure/
│   ├── hecate/client.py                # GET /v1/callers
│   ├── hecate/identity_store.py        # access.json en el directorio de estado
│   └── main.py                         # La salud con la copia
└── deploy/                             # Sin cambios
hermes/tests/                           # Unitarias, contra Hécate real, API y E2E con el acceso concedido
```

Cada crate de Hécate lleva sus pruebas como en las features anteriores: las unitarias junto al código y las de
integración y contrato en su `tests/`. Hermes las lleva en `hermes/tests/`, con la misma organización que en la 008.

**Structure Decision:** en Hécate, los cuatro crates de siempre, con un módulo nuevo por concepto del dominio
(`app_access`, junto a `access_policy`); la API sigue siendo un adaptador de infraestructura y los casos de uso viven en
la aplicación. En Hermes, la estructura de la 008: la copia es una publicación más de Hécate en `identity.py`, y la
comprobación, un paso más de `authentication.py`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Un aggregate más en Hécate (App Access) | La lista de cada app necesita su propia versión y su historial (FR-003, FR-006), y es otra frontera de consistencia: quién entra en una app no es lo que esa app puede usar | Guardarla en la Access Policy de la app movería su versión con las lecturas y el permiso de estado, y mezclaría los dos conceptos (decisión del propietario, R0) |
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
