---

description: "Tareas de la feature 009: lista de acceso por app"
---

# Tasks: Lista de acceso por app

**Input**: Design documents from `/specs/009-app-access/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US3 de la spec), en el orden de sus prioridades.
Cada historia se prueba por separado; el orden y lo que se puede adelantar están en «Dependencies & Execution Order».

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US3)
- Rutas relativas a la raíz del repositorio; las órdenes de Hécate van con `--manifest-path hecate/Cargo.toml` y las de
  uv con `--package cerbero-hermes`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar.
- **Dónde van las pruebas:**
  - en Hécate, las unitarias junto al código (`#[cfg(test)]`) y las de integración, contrato y CLI en el `tests/` de su
    crate, como en las features 003 y 006;
  - en Hermes, como en la 008: `hermes/tests/unit/`, `hermes/tests/integration/`, `hermes/tests/contract/`,
    `hermes/tests/e2e/` y los ayudantes en `hermes/tests/support/`.
- **Contrato:** cada respuesta de la API de Hécate que reciben las pruebas se valida contra `hecate.v1.json` con el
  ayudante de contrato de `hecate/crates/infrastructure/src/testing.rs`; las de Hermes, contra `hermes.v1.json` con la
  comprobación común de `hermes/tests/integration/conftest.py`.
- **Seguridad:** el acceso se deniega por defecto; el token corto no concede nada; los nombres de la app y de quien
  llama solo van al registro y a los logs una vez bien formados; ningún secreto, token ni certificado en logs, filas ni
  respuestas.
- **Lo anterior sigue pasando:** las suites de Hécate (001 a 007) y de Hermes (008), con las ampliaciones a propósito:
  la lista de operaciones de la API y las capacidades de Hécate, la versión de los OpenAPI y, en Hermes, conceder antes
  el acceso a quien llama.
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (el contrato primero)

**Purpose**: los contratos publicados, antes del código (principio II).

- [X] T001 [P] Añadir `listCallers` (`GET /v1/callers`) a
  `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` como fija [contracts/api.md](contracts/api.md):
  cabeceras `Authorization` y `X-Request-Id`, sin cuerpo ni parámetros,
  sin `Idempotency-Key`; el esquema de `data` con `app`, `callers` (lista de nombres de identidad, ordenada) y
  `version` (entero desde 0), sin campos adicionales; respuestas 200, 400, 401, 500 y 503 con `Completed` y `Failed` de
  `common.v1.json`; y el documento pasa a la 1.1.0
- [X] T002 Actualizar `hecate/crates/infrastructure/tests/contract_schema.rs`: espera la 1.1.0 de `hecate.v1.json` (y
  la 1.1.0 de `common.v1.json`), y comprueba que el ejemplo de respuesta de `listCallers` de contracts/api.md valida y
  que uno con un campo de más o de menos no
- [X] T003 [P] Actualizar las descripciones del 401 y del 503 de las tres operaciones de
  `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json` (quien no tiene acceso recibe `auth`; sin su lista,
  o si Hécate no responde al buscar a quien llama, `unavailable`), subirlo a la 1.0.1 y hacer que
  `hermes/tests/contract/test_openapi_schema.py` espere esa versión

**Checkpoint**: los contratos están publicados y sus pruebas pasan.

---

## Phase 2: Foundational (bloquea las historias de Hécate)

**Purpose**: el dominio de la lista de acceso de una app en Hécate: su aggregate, sus eventos y las operaciones del
registro de accesos.

### Pruebas de la fase

- [X] T004 [P] Pruebas del aggregate en `hecate/crates/domain/src/app_access/access.rs`, con las reglas de
  [data-model.md](data-model.md#reglas-al-cambiarla):
  - sin stream, `callers` vacío y versión 0, y nadie puede llamar (FR-010);
  - conceder añade a quien llama y sube la versión; retirar lo quita; conceder lo que ya está concedido, o retirar lo
    que no lo está, no produce evento;
  - «La app y quien llama siguen la regla de nombres de las identidades (minúsculas, dígitos y `-`, empezando por letra,
    hasta 63, un solo segmento)», así que un nombre con `/` se rechaza; «Ni la app ni quien llama son `hecate` ni
    `admin`»; «Quien llama no es la propia app»;
  - el historial y la lista en un instante pasado (`as_of`, SC-006);
  - proptest: tras cualquier secuencia de órdenes válidas, `callers` es el conjunto que dejan y la versión, el número de
    órdenes que cambiaron algo
- [X] T005 [P] Pruebas de los eventos en `hecate/crates/domain/src/events.rs`: `CallerGranted` y `CallerRevoked`, con
  `at`, `caller` y `by`, se codifican y se decodifican sin pérdida, con su nombre de tipo, y ninguno lleva un secreto
- [X] T006 [P] Pruebas del registro de accesos en `hecate/crates/domain/src/sealing/access_log.rs`: las operaciones
  `access_grant`, `access_revoke`, `access_list`, `access_get` y `callers_list` tienen ese texto

### Implementación de la fase

- [X] T007 Implementar `AccessEvent` (`CallerGranted` y `CallerRevoked`) y su codificación en
  `hecate/crates/domain/src/events.rs`
- [X] T008 Implementar el aggregate `AppAccess` en `hecate/crates/domain/src/app_access/mod.rs` y
  `hecate/crates/domain/src/app_access/access.rs` (stream `access/<app>`, `new`, `from_events`, `as_of`, `apply`,
  `callers`, `can_call` y las decisiones de conceder y retirar), y registrar el módulo en
  `hecate/crates/domain/src/lib.rs`
- [X] T009 Añadir las cinco operaciones al `Operation` de `hecate/crates/domain/src/sealing/access_log.rs`

**Checkpoint**: pasan las pruebas del dominio; Hécate sigue compilando y su suite pasa.

---

## Phase 3: User Story 1 - Conceder y retirar el acceso a una app (Priority: P1) 🎯 MVP

**Goal**: el admin concede y retira por el canal local y la CLI el acceso de una identidad a una app, y ve la lista
vigente de cada app y su historial, con cada cambio atribuido.

**Independent Test**: con Hécate desbloqueada, conceder, consultar, retirar y repetir órdenes con la CLI, y comprobar
las versiones, el historial y las filas del registro de accesos (quickstart, apartado 2).

### Pruebas de la historia

- [X] T010 [P] [US1] Pruebas del servicio en `hecate/crates/application/tests/app_access.rs`, como las de
  `access_policy.rs`:
  - conceder da la versión 1, un evento y una fila `access_grant` con `subject: admin`, la app como identidad y
    `caller <identidad>` en el detalle; repetirla da `changed: false`, sin evento, y deja su fila;
  - retirar, y retirar lo que no está concedido;
  - una identidad o una app mal formada (también con `/`), reservada o la propia app como quien llama dan `validation`
    con su fila `invalid_input`;
  - con Hécate sellada, `conflict` y una fila `rejected_conflict`;
  - se concede a una identidad, y para una app, sin alta (FR-005);
  - `list` da las apps con algún cambio, también vacías, por nombre; `get` da `callers` por nombre y el historial con
    versión, cambio, identidad, instante y quién; una app sin cambios, la lista vacía en la versión 0;
  - «Sin versión esperada; si otro cambio se adelanta, se vuelve a leer y a evaluar»;
  - una Hécate recién inicializada no tiene ninguna lista: `list` vacío y `get hermes` en la versión 0, sin accesos de
    partida (FR-010)
- [X] T011 [P] [US1] Pruebas del canal local en `hecate/crates/infrastructure/tests/contract_access_ops.rs`, como las de
  `contract_policy_ops.rs`: `access.grant`, `access.revoke`, `access.list` y `access.get` con los `args` y la `data` de
  [contracts/control-protocol.md](contracts/control-protocol.md), el sobre común, `validation` y `conflict`
- [X] T012 [P] [US1] Pruebas de la CLI en `hecate/crates/cli/tests/access.rs`, con assert_cmd, como las de
  `policies.rs`: las salidas de [contracts/cli.md](contracts/cli.md#hecate-access), `error (validation): ...` y
  `error (conflict): ...` con salida 1

### Implementación de la historia

- [X] T013 [US1] Implementar `AppAccessService` en `hecate/crates/application/src/app_access/mod.rs` y
  `hecate/crates/application/src/app_access/service.rs` (la lista de una app, `grant`, `revoke`, `list` y `get`, con
  sus filas), sobre los streams del almacén de identidades, y registrar el módulo en
  `hecate/crates/application/src/lib.rs`
- [X] T014 [US1] Implementar las operaciones `access.*` en `hecate/crates/infrastructure/src/control/operations.rs` y
  pasar `AppAccessService` al canal local donde se compone Hécate: `hecate/crates/cli/src/lib.rs` (`hecate serve`) y
  los arneses de `hecate/crates/application/src/testing.rs` y `hecate/crates/infrastructure/src/testing.rs`
- [X] T015 [US1] Implementar `hecate access grant|revoke|list|get` en `hecate/crates/cli/src/lib.rs`, con su ayuda y
  las salidas de contracts/cli.md

**Checkpoint**: US1 funciona: el admin gestiona las listas por la CLI; pasan sus pruebas y las de las fases anteriores.

---

## Phase 4: User Story 2 - Una app consulta su propia lista (Priority: P2)

**Goal**: una app autenticada obtiene de Hécate su lista y su versión, solo la suya.

**Independent Test**: con el componente de prueba dado de alta como una app, conceder el acceso a dos identidades y
consultar la lista; comprobar que nada permite pedir la de otra app (quickstart, apartado 3).

### Pruebas de la historia

- [X] T016 [P] [US2] Pruebas de la API en `hecate/crates/infrastructure/tests/api_callers.rs`, con el componente de
  prueba, como las de `api_status.rs`:
  - una app con identidades concedidas recibe `app`, `callers` por nombre y `version`; sin ninguna, `callers` vacío y
    versión 0, sin error;
  - la consulta no tiene parámetros: `GET /v1/callers/otra` y cualquier otra forma no son operaciones (`not_found`);
  - certificado o token que no valen, `auth` con fila `callers_list`; Hécate sellada, `unavailable` con fila; atendida,
    sin fila;
  - cada respuesta valida contra `listCallers`, y `GET /v1/capabilities` lista `callers`
- [X] T017 [P] [US2] Ampliar las pruebas de `operation_of` y `answers_while_sealed` en
  `hecate/crates/infrastructure/src/api/routes.rs`: `GET` y `HEAD /v1/callers` son la operación de la lista, y no
  responden sellada; `POST /v1/callers` y `/v1/callers/` no son operaciones

### Implementación de la historia

- [X] T018 [US2] Implementar `callers` en `hecate/crates/application/src/identity/service.rs`: autentica a quien llama,
  lee la lista de su identidad con `AppAccessService` y deja fila solo en los rechazos; `IdentityService::new` recibe
  el servicio, y se actualizan sus llamadas en `hecate/crates/cli/src/lib.rs`,
  `hecate/crates/application/src/testing.rs`, `hecate/crates/infrastructure/src/testing.rs` y las pruebas que lo
  construyen
- [X] T019 [US2] Implementar `GET /v1/callers` en `hecate/crates/infrastructure/src/api/routes.rs`: la ruta, su
  manejador, su caso en `operation_of` y `callers` en la lista de operaciones de las capacidades; y ampliar las pruebas
  que fijan esa lista: `hecate/crates/infrastructure/tests/contract_schema.rs` y
  `hecate/crates/infrastructure/tests/api_revocations.rs`
- [X] T020 [P] [US2] Añadir `callers` al componente de prueba en `hecate/crates/infrastructure/src/testing.rs` y la
  orden `callers` al ejemplo `hecate/crates/infrastructure/examples/component.rs`

**Checkpoint**: US1 y US2 funcionan; la suite de Hécate pasa en macOS.

---

## Phase 5: User Story 3 - Hermes atiende solo a quien tiene acceso (Priority: P3)

**Goal**: Hermes comprueba, tras autenticar, que quien llama está en su lista; la mantiene al día con Hécate y, sin
ninguna copia, no está listo.

**Independent Test**: con una Hécate de pruebas y Hermes detrás de Envoy, un componente con acceso envía y otro sin
acceso recibe `auth` sin que salga nada; una retirada vale en el siguiente refresco; con Hécate sellada sigue la última
lista (quickstart, apartado 4).

### Pruebas de la historia

- [X] T021 [US3] Ampliar los ayudantes de pruebas de Hermes, antes de las demás pruebas de la historia, que los usan:
  - `hermes/tests/support/doubles.py`: `StaticTrust` con la lista de quien puede llamar y su búsqueda (con su contador
    de peticiones y la opción de no responder), y `MemoryIdentityStore` con la copia;
  - `hermes/tests/support/fake_hecate.py`: la consulta de la lista;
  - `hermes/tests/support/hecate.py`: conceder y retirar el acceso en la Hécate real (`hecate access grant|revoke`);
  - `hermes/tests/support/network_api.py` y las pruebas que construyen el autenticador: quien llama tiene el acceso
    concedido, para que lo de la 008 siga pasando
- [X] T022 [P] [US3] Pruebas del almacén en `hermes/tests/unit/hecate/test_identity_store.py`:
  «`HERMES_STATE_DIR/access.json` (0600, en el directorio 0700)», escrito de forma atómica, se vuelve a leer igual, y
  sin fichero no hay copia
- [X] T023 [P] [US3] Pruebas de la copia en `hermes/tests/unit/application/test_publications.py`, con un doble de
  `HecateApi` y `FakeClock`:
  - el refresco de cada 240 s trae la lista con la de revocación y las claves, la guarda y la deja vigente;
  - si Hécate no responde o está sellada, sigue la última copia (SC-004);
  - al arrancar, con copia guardada arranca con ella; sin ninguna, Hécate tiene que darla;
  - la búsqueda ante quien no está pide la lista una sola vez aunque lleguen varias a la vez, la sustituye entera y la
    guarda; si Hécate no responde, `unavailable`
- [X] T024 [P] [US3] Pruebas de la comprobación en `hermes/tests/unit/application/test_authentication.py`:
  - quien está en la copia entra sin preguntar a Hécate;
  - quien no está recibe `auth` con el mismo mensaje tras volver a pedir la lista, y el log dice `not_allowed`;
  - con la copia vacía (versión 0, sin nada concedido), cualquier identidad autenticada recibe `auth` (historia 3,
    escenario 6);
  - una concesión vale en la llamada siguiente: la búsqueda lo encuentra y entra;
  - si Hécate no responde a la búsqueda, `unavailable`;
  - lo que diga el token sobre permisos, roles o accesos no cuenta (FR-007);
  - quien no tiene acceso no deja `jti` guardado;
  - sin copia de la lista, Hermes no tiene con qué autenticar y responde `unavailable`
- [X] T025 [P] [US3] Pruebas de la salud en `hermes/tests/integration/admin/test_health.py`: el motivo
  `access list not loaded` va después del de la identidad y antes del de la base; `hermes health` escribe
  `not ready: access list not loaded` y sale con 1
- [X] T026 [P] [US3] Pruebas contra Hécate real en `hermes/tests/integration/hecate/test_client.py` (marca `hecate`): la
  consulta da la lista tras `hecate access grant`, vacía sin concesiones, y `unavailable` con Hécate sellada
- [X] T027 [US3] Pruebas de la API en `hermes/tests/integration/api/test_auth_routes.py`: cada ruta responde `auth`
  (401) con el mismo mensaje y sin llamar al proveedor a una identidad autenticada que no está en la lista; una
  concesión hecha en el doble entra en la llamada siguiente; Hécate sin responder a la búsqueda, `unavailable` (503);
  la línea `auth.refused` con el motivo `not_allowed` lleva el `request_id` de la respuesta (FR-008); y cada respuesta
  valida contra `hermes.v1.json`

### Implementación de la historia

- [X] T028 [US3] Definir los puertos en `hermes/src/hermes/application/ports.py`: `AccessList` (app, identidades,
  versión y cuándo se obtuvo), la consulta en `HecateApi`, la copia en `IdentityStore`, la lista en `TrustMaterial` y
  la búsqueda en `TrustSource`
- [X] T029 [P] [US3] Implementar la consulta en `hermes/src/hermes/infrastructure/hecate/client.py` (`GET /v1/callers`)
  y la copia en `hermes/src/hermes/infrastructure/hecate/identity_store.py` (`access.json`)
- [X] T030 [US3] Implementar la copia en `hermes/src/hermes/application/identity.py`: se lee al arrancar, se refresca
  con las publicaciones, se exige en `ensure_publications`, `material()` no la da sin ella, la búsqueda ante quien no
  está (una petición a la vez) y su problema para la salud
- [X] T031 [US3] Implementar la comprobación en `hermes/src/hermes/application/authentication.py`, después del token y
  antes de guardar el `jti` (research R5)
- [X] T032 [US3] Implementar el motivo `access list not loaded` en `hermes/src/hermes/application/health.py` y
  conectarlo en `hermes/src/hermes/infrastructure/main.py`
- [X] T033 [US3] Ampliar el E2E en `hermes/tests/e2e/test_flow.py` (marcas `docker` y `hecate`), sin romper lo que ya
  prueba:
  - la preparación concede el acceso a Hermes a `caller` y a `caller-two`, que tiene que enviar antes de que se revoque
    su certificado, y da de alta un tercer componente, `caller-three`, sin acceso;
  - `caller-three` recibe `auth` sin que salga ningún mensaje, y tras concederle el acceso envía en la llamada
    siguiente;
  - tras retirárselo, recibe `auth` en el siguiente refresco (240 s como mucho), en la misma espera que la del
    certificado revocado;
  - nunca se retira el acceso a `caller`, que usan las pruebas siguientes, como la de PostgreSQL parado

**Checkpoint**: las tres historias funcionan; pasan las suites de Hécate y de Hermes.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T034 Consolidar los docs según FR-013 y research R9:
  - `docs/hecate/acceso.md`: el acceso por app construido (el aggregate, las órdenes, la consulta, denegar por defecto
    y la regla de las identidades hijas);
  - `docs/hecate.md`: los seis aggregates y el párrafo del acceso;
  - `docs/hecate/integracion.md`: `GET /v1/callers` en su tabla de operaciones y cómo la usa una app (copia, refresco y
    búsqueda ante quien no está);
  - `docs/hecate/cli.md` y `docs/hecate/auditoria.md`: `hecate access` y las cinco operaciones del registro;
  - `docs/hermes.md`: la comprobación en la autenticación, la lista entre las publicaciones, el motivo nuevo de la
    salud y, en la operación, que el admin concede antes los accesos;
  - `ARCHITECTURE.md`: los seis aggregates de Hécate y quién decide el acceso en las llamadas entre componentes;
  - `GLOSSARY.md`: App Access;
  - `DECISIONS.md`: «Acceso por app» construido, la regla de las identidades hijas en su fila y la referencia del
    principio VII para la autorización entre servicios;
  - `README.md`: lo que ya hacen Hécate y Hermes, en «Estado del Proyecto»;
  - `shared/contracts/README.md`: las versiones nuevas de `hecate.v1.json` y `hermes.v1.json`, si las nombra
- [X] T035 Pasar la calidad y las suites del apartado 1 de [quickstart.md](quickstart.md): en Hécate, `cargo fmt`,
  `cargo clippy` con los avisos como errores, `cargo deny` y `cargo test` en macOS y en el contenedor
  `rust:1.99.1-bookworm`; en Hermes, ruff, el formato, `mypy --strict`, pip-audit y la suite entera con `HECATE_BIN` y
  Docker, con el E2E
- [X] T036 Recorrer los apartados 2 a 4 de [quickstart.md](quickstart.md) con el binario release, el ejemplo
  `component` y el compose de `hermes/deploy/`, y dejar en la guía de revisión lo que se vio
- [X] T037 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `feat:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, terminada con la ruta de la guía de revisión;
  - deben recoger `hecate/`, `hermes/`, `shared/contracts/`, `docs/`, `ARCHITECTURE.md`, `GLOSSARY.md`, `DECISIONS.md`,
    `README.md` y `specs/009-app-access/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias; son los contratos.
- **Foundational (Phase 2)**: depende del Setup y bloquea US1 y US2.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2 (Phase 4)**: depende de US1, porque la consulta lee la lista con `AppAccessService` y sus pruebas la llenan con
  sus órdenes.
- **US3 (Phase 5)**: sus pruebas unitarias y de API usan dobles y pueden empezar tras el Setup; las de Hécate real y el
  E2E (T026 y T033) dependen de US2.
- **Polish (Phase 6)**: al final; T037 depende de todo lo demás.

### Within Each Phase

- Las pruebas primero, y deben fallar; después, dominio → aplicación → infraestructura → CLI.
- Las tareas que tocan el mismo fichero van en orden: `hecate/crates/cli/src/lib.rs` (T014, T015 y T018),
  `hecate/crates/application/src/testing.rs` e `hecate/crates/infrastructure/src/testing.rs` (T014, T018 y T020),
  `hecate/crates/infrastructure/src/api/routes.rs` (T017 y T019), y `hermes/src/hermes/application/identity.py` (T030).
- En US3, los ayudantes de T021 van antes de las pruebas T022 a T027, que los usan.
- Cada fase termina en su checkpoint con la suite completa en verde.

### Parallel Opportunities

- Setup: T001 y T003.
- Fase 2: las pruebas T004 a T006.
- US1: las pruebas T010 a T012.
- US2: las pruebas T016 y T017, y la implementación T020.
- US3: tras los ayudantes de T021, las pruebas T022 a T026, y la implementación T029.

### Parallel Example: User Story 1

```bash
Task: "Pruebas del servicio en hecate/crates/application/tests/app_access.rs"
Task: "Pruebas del canal local en hecate/crates/infrastructure/tests/contract_access_ops.rs"
Task: "Pruebas de la CLI en hecate/crates/cli/tests/access.rs"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: los contratos y el aggregate.
2. US1: el admin concede y retira el acceso por la CLI.
3. **Revisión del propietario:** las pruebas de US1 y el apartado 2 del quickstart.

### Siguientes incrementos

1. US2: la consulta de una app, con su revisión (apartado 3).
2. US3: la comprobación en Hermes, con su revisión (apartado 4 y el E2E).
3. Polish: la consolidación, la calidad y las suites, el recorrido y el mensaje de commit.

La feature se entrega entera en una PR con un commit de squash `feat:`; los incrementos son puntos de revisión.
