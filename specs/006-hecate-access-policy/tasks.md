---

description: "Tareas de la feature 006: Access Policy de Hécate y resolución de secretos"
---

# Tasks: Access Policy de Hécate y resolución de secretos

**Input**: Design documents from `/specs/006-hecate-access-policy/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US3 de la spec), en el orden de sus prioridades.
US2 y US3 se apoyan en lo que crea US1 (la resolución por la API y su autenticación); el orden está en «Dependencies &
Execution Order».

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US3)
- Rutas relativas a la raíz del repositorio; las órdenes de cargo se ejecutan dentro de `hecate/`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar.
- **Dónde van las pruebas** (como en las features anteriores):
  - las de dominio, en un módulo `#[cfg(test)]` del propio fichero;
  - las de aplicación, en `hecate/crates/application/tests/`, con la infraestructura como dependencia de desarrollo;
  - las de integración, API y contrato, en `hecate/crates/infrastructure/tests/`, y las del contrato del canal local en
    un `contract_*_ops.rs` por grupo de operaciones;
  - las de la CLI, en `hecate/crates/cli/tests/`, con el terminal guionizado.
- **Ayudantes:** `CaRig` (`hecate/crates/application/src/testing.rs`) y `SqliteStack` y `TestComponent`
  (`hecate/crates/infrastructure/src/testing.rs`) ganan lo que necesitan las pruebas, tras la feature `test-support`.
- **Contrato:** cada prueba que recibe una respuesta de la API la valida contra `hecate.v1.json` y `common.v1.json` con
  `testing::Contract`.
- **Sin cambios en 001, 002 y 003 (FR-020):** las 754 pruebas siguen pasando en cada fase. Solo se amplían a propósito:
  - `contract_status_op.rs` y la salida de `hecate status` en `cli_secrets.rs`, que ganan `secrets_due_rotation`
    (T035);
  - la lista de operaciones de `GET /v1/capabilities` en `api_revocations.rs` (T017 y T034);
  - las pruebas de la 002 que usan `rotate_every: "90d"` (`secrets_create.rs`, `secrets_list_get.rs` y la de dominio
    de `secret.rs`), que pasan a `"P90D"` (T032), y las que usan `"30d"` (`contract_secret_ops.rs` y `cli_secrets.rs`),
    que pasan a `"P30D"` (T035), como fija research R5.

  El único cambio de comportamiento es FR-023.
- **Seguridad:**
  - `unsafe_code = "forbid"`;
  - los valores resueltos, en tipos que se borran al soltarse desde el descifrado hasta el socket (research R6);
  - ningún valor, huella ni token en eventos, logs, errores ni en el registro de accesos;
  - los tres rechazos de la resolución, iguales en contenido y en tiempo (FR-003, FR-004).
- **Registro de accesos:** cada operación deja las filas de [data-model.md](data-model.md#registro-de-accesos).
- **Atomicidad (FR-019):** cada orden de política escribe en una transacción con la versión que lee y, si otra se
  adelanta, vuelve a leer y a evaluar (FR-008).
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (el contrato primero)

**Purpose**: publicar el contrato de las operaciones nuevas antes de escribir código (principio II).

- [X] T001 Añadir a `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`:
  - `resolveSecrets` (`POST /v1/secrets:resolve`): cuerpo `{references: [string]}` de 1 a 100 elementos, y respuestas
    200, 400, 401, 404, 500 y 503;
  - `getStatus` (`GET /v1/status`): respuestas 200, 400, 401 y 500;
  - los esquemas `ResolveRequest`, `ResolvedSecret` y `HecateStatus` de [contracts/api.md](contracts/api.md) y
    [contracts/formats.md](contracts/formats.md), sin campos adicionales;
  - `secrets:resolve` y `status` en la lista de operaciones de `getCapabilities`
- [X] T002 Ampliar `hecate/crates/infrastructure/tests/contract_schema.rs`: los esquemas nuevos resuelven sus `$ref`, un
  ejemplo de cada respuesta de contracts/api.md valida, y una con campos de más o de menos no valida

**Checkpoint**: el contrato está publicado y sus pruebas pasan, junto con las 754 anteriores.

---

## Phase 2: Foundational (bloquea todas las historias)

**Purpose**: la Access Policy en el dominio, sus eventos, las operaciones nuevas del registro de accesos y lo que
necesitan del esqueleto de la API la resolución y el estado.

### Pruebas de la fase

- [X] T003 [P] Pruebas del aggregate en `hecate/crates/domain/src/access_policy/policy.rs`:
  - conceder una lectura escribe `ReadGranted` y retirarla, `ReadRevoked`; con el permiso de estado, igual;
  - repetir una orden que no cambia nada no escribe eventos;
  - `can_read`: lo suyo siempre; una ruta ajena solo con una excepción para esa ruta exacta, nunca por prefijo;
  - la versión y el historial salen de los eventos, y aplicarlos hasta un instante da la política de ese momento
- [X] T004 [P] Pruebas de `PolicyEvent` en `hecate/crates/domain/src/events.rs`: ida y vuelta de los cuatro eventos con
  `at` y `by`, y rechazo de uno mal formado
- [X] T005 [P] Pruebas de las operaciones nuevas del registro en `hecate/crates/domain/src/sealing/access_log.rs` y
  `hecate/crates/infrastructure/tests/store_access_log.rs`: `policy_grant`, `policy_revoke`, `policy_list`,
  `policy_get` y `status_get` se escriben y se leen
- [X] T006 [P] Pruebas del esqueleto de la API en `hecate/crates/infrastructure/tests/api_server.rs`:
  - la guarda de sellada pregunta por cada petición (método y ruta) y deja pasar las rutas que responden selladas;
  - una respuesta con secretos se escribe en un búfer de tamaño exacto que se borra cuando hyper suelta el cuerpo (un
    dueño de prueba comprueba el borrado);
  - esa respuesta lleva el mismo sobre que las demás (`request_id` y `duration_ms`, calculada al construirla) y valida
    contra `common.v1.json`
- [X] T007 Pruebas de `AccessPolicyService::policy_of` en `hecate/crates/application/tests/access_policy.rs`: una
  identidad sin stream tiene la política vacía (versión 0) y, con eventos, la que dan

### Implementación de la fase

- [X] T008 [P] Implementar `PolicyEvent` en `hecate/crates/domain/src/events.rs`, para el stream `policy/<identidad>`
- [X] T009 Implementar `AccessPolicy` en `hecate/crates/domain/src/access_policy/` sobre los eventos de T008 (estado,
  transiciones, `can_read`, `has_status` e historial) y exportarlo en `hecate/crates/domain/src/lib.rs`
- [X] T010 [P] Añadir las operaciones del registro en `hecate/crates/domain/src/sealing/access_log.rs`
- [X] T011 Implementar en el esqueleto de la API:
  - la guarda por ruta en `hecate/crates/infrastructure/src/api/server.rs` (lo que hoy es `SealedCheck` recibe el
    método y la ruta) y sus usos: `hecate/crates/infrastructure/src/api/supervisor.rs`, `hecate serve`
    (`hecate/crates/cli/src/lib.rs`) y, en `hecate/crates/infrastructure/tests/`, `api_server.rs`, `api_identity.rs`,
    `identity_no_leak.rs` y `common/mod.rs`, que solo cambian cómo construyen la guarda, no lo que comprueban;
  - la respuesta que se borra en `hecate/crates/infrastructure/src/api/envelope.rs`: serialización a un
    `Zeroizing<Vec<u8>>` de tamaño exacto, en dos pasadas, enviado con `axum::body::Bytes::from_owner`
- [X] T012 Crear `AccessPolicyService` en `hecate/crates/application/src/access_policy/service.rs`, con `policy_of`,
  y añadirlo a `CaRig` y a `SqliteStack`:
  - lee y escribe el stream `policy/<identidad>` por el `IdentityStore`, sin tablas nuevas;
  - depende, como `SecretsService`, de `SealingService`, `AccessLog` y `Clock`, y del `SecretStore` para comprobar
    que un secreto existe

**Checkpoint**: pasan las pruebas de la fase y las 754 anteriores.

---

## Phase 3: User Story 1 - Resolver sus secretos (Priority: P1) 🎯 MVP

**Goal**: un componente autenticado resuelve sus secretos por la API, todo o nada, con versiones, tiempos de caché y
rechazos que no se distinguen ni por contenido ni por tiempo.

**Independent Test**: guardar secretos para un componente de prueba dado de alta, resolverlos y comprobar valores,
versiones y tiempos de caché; que una referencia ajena o inexistente rechaza la petición entera con la misma respuesta
y sin diferencia de tiempo; las filas; y que el valor no queda en la memoria de Hécate.

### Pruebas de la historia

- [X] T013 [P] [US1] Pruebas de la resolución en dos fases en `hecate/crates/application/tests/secrets_resolve.rs`:
  - el método nuevo recibe el permiso y el contexto de la llamada (SPIFFE ID, identidad y serie) para las filas;
  - comprueba todas las referencias antes de descifrar ninguna: un rechazo no abre ninguna clave (un doble lo cuenta);
  - la fila de un rechazo lleva la primera referencia que falla;
  - datos inconsistentes (versión activa sin material) dan `not_found`, y una referencia repetida se resuelve otra vez;
  - `resolve(identity, references)` de la feature 002 sigue igual (FR-020)
- [X] T014 [P] [US1] Pruebas del caso de uso de la API en `hecate/crates/application/tests/identity_resolve.rs`:
  - secretos propios: la `active`, una concreta con `@N`, una `deprecated` en su gracia y un secreto caducado; una
    `deprecated` pasada su gracia, `revoked` y `destroyed` no;
  - de otra identidad, inexistentes y sin versión: el mismo `not_found`, todo o nada;
  - filas `secret_resolve` con el SPIFFE ID, la identidad y la serie: una por referencia, o la primera que falla;
  - sellada, `unavailable` con su fila; `auth` con fila; `validation` para una referencia mal formada, ninguna o más
    de 100, `@0`, `@01` o `@1000000000`;
  - Hécate sellada entre las dos fases (un `SecretStore` de prueba que sella al leer el material, como el envoltorio
    de `SqliteStack::open_with`): `unavailable` con una sola fila, nunca `conflict`;
  - la política se lee una sola vez por petición, sea cual sea el número de referencias (un doble cuenta las lecturas
    del stream `policy/<identidad>`): un cambio a la vez da la de antes o la de después, nunca una mezcla;
  - los valores, en `Zeroizing`
- [X] T015 [P] [US1] Pruebas de la API y de sus tiempos:
  - en `hecate/crates/infrastructure/tests/api_resolve.rs`, con el componente de prueba:
    - los campos del contrato, validados contra el OpenAPI, con `cache_ttl_soft_s` 300 y `cache_ttl_hard_s` 3600;
    - los tres `not_found` con el mismo cuerpo, ninguno antes del tiempo mínimo, y `meta.duration_ms` que lo incluye;
    - sellada, `auth` y `validation`, con sus filas; un cuerpo que no es JSON estándar, `validation` sin fila; un
      `X-Request-Id` mal formado, `validation` con su fila, como en la feature 003;
    - SC-001: 10 secretos de 64 KiB en menos de 1 segundo;
  - SC-003, en su propio fichero `hecate/crates/infrastructure/tests/api_resolve_timing.rs`, para que ninguna otra
    prueba corra a la vez: cinco grupos de 1.000 rechazos (los tres tipos con una referencia, y peticiones de 10
    referencias en las que falla la primera o la última, siempre por un secreto sin versión resoluble), intercalados
    en orden aleatorio con una semilla fija, con 8 peticiones a la vez y un tiempo mínimo de prueba de 50 ms, por
    encima de lo que tardan antes de la espera (pocas referencias, sobre el almacén en memoria de `CaRig`); la
    diferencia entre las medianas de dos tipos, o entre falla la primera y falla la última, no supera la mayor de las
    diferencias entre las dos mitades, en orden de envío, de los dos grupos comparados, más 1 ms;
  - la primera fase, en `hecate/crates/infrastructure/tests/performance.rs`, sobre SQLite: un rechazo de 100
    referencias en el que falla la última, por un secreto sin versión resoluble; lo que va antes de la espera
    (`IdentityService::resolve`, con la autenticación y la política) tiene una mediana de 20 mediciones por debajo de
    50 ms, una quinta parte de los 250 ms de producción (research R2)
- [X] T016 [P] [US1] Ampliar `hecate/crates/infrastructure/tests/identity_no_leak.rs`: tras resoluciones atendidas y
  rechazadas, ni la base, ni el registro de accesos, ni los logs, ni las respuestas de error contienen un valor
  resuelto ni su huella, en ninguna codificación (SC-004)
- [X] T017 [P] [US1] Pruebas de las operaciones de la API:
  - `operation_of`, en el módulo de pruebas de `hecate/crates/infrastructure/src/api/routes.rs`:
    `POST /v1/secrets:resolve` es `secret_resolve` con método `identity`;
  - la lista de operaciones, en `hecate/crates/infrastructure/tests/api_revocations.rs`: `GET /v1/capabilities` lista
    `secrets:resolve`

### Implementación de la historia

- [X] T018 [US1] Implementar en `hecate/crates/application/src/secrets/service.rs` la resolución en dos fases: un
  método que recibe el permiso (`Fn(&SecretPath) -> bool`) y el contexto de la llamada, que comprueba todas las
  referencias antes de descifrar y escribe las filas; `resolve(identity, references)` pasa a usarlo con la regla por
  defecto
- [X] T019 [US1] Implementar `IdentityService::resolve` en `hecate/crates/application/src/identity/service.rs`: la
  autenticación de la feature 003, `unsealed_for`, la política de `AccessPolicyService` y la delegación en
  `SecretsService`. Esas dos dependencias nuevas van en el constructor (también en `CaRig` y `SqliteStack`), y
  `refusal_floor` (250 ms por defecto) en `hecate/crates/application/src/identity/settings.rs`. Si Hécate se sella
  entre la autenticación y el descifrado, el `conflict` de `SecretsService` (el de la feature 002) se responde como
  `unavailable`, con una sola fila
- [X] T020 [US1] Implementar `POST /v1/secrets:resolve` en `hecate/crates/infrastructure/src/api/routes.rs`: el caso de
  uso en un hilo bloqueante; la espera asíncrona hasta el tiempo mínimo (`tokio::time::sleep_until`) para un
  `not_found`, antes de construir la respuesta; la respuesta que se borra (T011); `operation_of` y `OPERATIONS`
- [X] T021 [US1] Dar al componente de prueba `resolve` en `hecate/crates/infrastructure/src/testing.rs` y la orden
  `resolve <referencias...>` en `hecate/crates/infrastructure/examples/component.rs`
- [X] T022 [US1] Conectarlo en `hecate serve` (`hecate/crates/cli/src/lib.rs`): `AccessPolicyService` y las
  dependencias nuevas de `IdentityService`

**Checkpoint**: pasan las pruebas de US1 y las 754 anteriores; el apartado 5 de quickstart.md funciona.

---

## Phase 4: User Story 2 - Compartir un secreto con otra identidad (Priority: P2)

**Goal**: el admin concede y retira excepciones de lectura por secreto concreto, con una política versionada y su
historial.

**Independent Test**: conceder a un componente de prueba un secreto de otra identidad, resolverlo, retirar la excepción
y comprobar que la siguiente petición se rechaza, las versiones de la política con quién y cuándo, y el historial.

### Pruebas de la historia

- [X] T023 [P] [US2] Pruebas de las órdenes en `hecate/crates/application/tests/access_policy.rs`:
  - `grant_read`, `revoke_read`, `grant_status` y `revoke_status`;
  - `validation` para una identidad reservada o mal formada, una ruta mal formada o con `@N`, o un secreto de la propia
    identidad; `not_found` para un secreto que no existe; una identidad sin alta, aceptada;
  - repetir una orden no escribe eventos y da `changed: false`; las filas `policy_grant` y `policy_revoke` llevan
    `detail` también cuando no cambian nada;
  - `list` (solo identidades con cambios, por nombre) y `get` (vacía en la versión 0; historial con `at` y `by`);
  - sellada, `conflict` con fila;
  - un corte con `fail_commits` deja la política anterior o la nueva;
  - dos órdenes a la vez sobre la misma identidad: la que pierde vuelve a leer y a evaluar
- [X] T024 [P] [US2] Ampliar `hecate/crates/application/tests/identity_resolve.rs` con excepciones:
  - un secreto compartido se resuelve con las reglas de versiones de su dueño, y los demás de esa identidad siguen
    denegados;
  - retirarla afecta a la siguiente petición;
  - un secreto compartido sin versiones resolubles da `not_found`;
  - deshabilitar al dueño no retira la excepción;
  - deshabilitar la identidad que la recibió, o sustituir la CA, conserva su política: al darse de alta de nuevo,
    resuelve otra vez el secreto compartido;
  - una petición con secretos propios y compartidos es todo o nada
- [X] T025 [P] [US2] Pruebas de contrato de las seis operaciones en
  `hecate/crates/infrastructure/tests/contract_policy_ops.rs`: `args`, `data` y errores de
  [contracts/control-protocol.md](contracts/control-protocol.md), con la fila de cada una
- [X] T026 [P] [US2] Pruebas de la CLI en `hecate/crates/cli/tests/policies.rs`: las salidas de
  [contracts/cli.md](contracts/cli.md), también `unchanged`, `no policies` y los errores
- [X] T027 [P] [US2] Ampliar `hecate/crates/infrastructure/tests/api_resolve.rs`: conceder, resolver, retirar y volver a
  resolver con el componente de prueba (historia 2, escenarios 1 a 3)

### Implementación de la historia

- [X] T028 [US2] Implementar las órdenes y las consultas en `hecate/crates/application/src/access_policy/service.rs`:
  Hécate desbloqueada (si no, `conflict`), las validaciones de T023, la transacción con la versión leída y una nueva
  lectura si otra orden se adelanta (`ServiceError::Concurrency`), y sus filas
- [X] T029 [US2] Registrar `policies.*` en `hecate/crates/infrastructure/src/control/operations.rs`, con los argumentos
  mal formados registrados como en el resto de operaciones del canal
- [X] T030 [US2] Implementar `hecate policies grant-read|revoke-read|grant-status|revoke-status|list|get` en
  `hecate/crates/cli/src/lib.rs`

**Checkpoint**: pasan las pruebas de US2 y las anteriores; el apartado 6 de quickstart.md funciona.

---

## Phase 5: User Story 3 - Consultar el estado agregado (Priority: P3)

**Goal**: una identidad con el permiso de estado consulta los recuentos agregados, sin nombres, también con Hécate
sellada, y `rotate_every` pasa a ser una duración con la que contar los secretos que deben rotarse.

**Independent Test**: conceder el permiso de estado a un componente de prueba, consultar el estado con Hécate
desbloqueada y sellada, comprobar los recuentos, que ninguna respuesta lleva rutas ni nombres y que sin el permiso se
rechaza.

### Pruebas de la historia

- [X] T031 [P] [US3] Pruebas de `RotationInterval` en `hecate/crates/domain/src/secrets/rotation.rs`:
  - acepta `P<n>D` de 1 a 3650 y `P<n>W` de 1 a 520;
  - rechaza `P0D`, `P0W`, `P090D`, `P012W`, `P+90D`, `PD`, `P3651D`, `P521W`, `P99999999999D`, `p90d`, `90d`,
    `P1W2D`, `P3M`, `P1Y`, `PT36H`, el texto vacío y `P90D` con espacios delante o detrás;
  - vence en el instante exacto en que se cumple la duración
- [X] T032 [P] [US3] Pruebas de `rotate_every` y del recuento en `hecate/crates/application/tests/secrets_create.rs` y
  `hecate/crates/application/tests/secrets_list_get.rs`:
  - `secrets.create` acepta `P90D` y `P12W` y rechaza otro texto con `validation`;
  - las pruebas existentes pasan de `"90d"` a `"P90D"`, también la de dominio de
    `hecate/crates/domain/src/secrets/secret.rs`;
  - un valor antiguo guardado (un evento con texto libre) se lee y no cuenta;
  - el recuento cuenta los secretos que cumplen la duración, también en el instante exacto, y no los que no tienen
    versión activa
- [X] T033 [P] [US3] Pruebas del caso de uso en `hecate/crates/application/tests/identity_status.rs`:
  - con el permiso, los campos de [data-model.md](data-model.md#estado-agregado);
  - sin el permiso, `auth` con fila `status_get`;
  - conceder el permiso, consultar, retirarlo y consultar otra vez: la segunda consulta da `auth` con fila (FR-010);
  - sellada, responde con `sealed: true` y los mismos recuentos; un token que no vale o una identidad sin el permiso
    dan `auth` con fila, igual que desbloqueada;
  - ninguna respuesta lleva rutas ni nombres, y una consulta atendida no deja fila
- [X] T034 [P] [US3] Pruebas de la API:
  - en `hecate/crates/infrastructure/tests/api_status.rs`, con el componente de prueba: `GET /v1/status` validado
    contra el contrato, desbloqueada y sellada; sin el permiso o con un token que no vale, 401 con fila, también
    sellada; un `X-Request-Id` mal formado, 400 con su fila, como en la feature 003; y `HEAD /v1/status` como la
    misma operación;
  - `operation_of` en `hecate/crates/infrastructure/src/api/routes.rs`: `GET` y `HEAD /v1/status` son `status_get`;
  - `status` en la lista de operaciones de `hecate/crates/infrastructure/tests/api_revocations.rs`
- [X] T035 [P] [US3] Ampliar las pruebas del canal y de la CLI:
  - `hecate/crates/infrastructure/tests/contract_status_op.rs`, con `secrets_due_rotation`, también sellada;
  - la salida de `hecate status` en `hecate/crates/cli/tests/cli_secrets.rs`, con la línea
    `secrets due rotation: <n>` junto a los recuentos de caducidad;
  - `hecate/crates/infrastructure/tests/contract_secret_ops.rs`, con el error de `rotate_every`

### Implementación de la historia

- [X] T036 [US3] Implementar `RotationInterval` en `hecate/crates/domain/src/secrets/rotation.rs`, su validación en
  `secrets.create` y el recuento de los secretos que deben rotarse en
  `hecate/crates/application/src/secrets/service.rs`. Antes de convertir n, la validación comprueba que solo tiene
  dígitos ASCII, sin cero a la izquierda y con una longitud acotada, como `version_number` en
  `hecate/crates/domain/src/secrets/path.rs`: `u32::from_str` por sí sola admite un `+` delante
- [X] T037 [US3] Implementar `IdentityService::status` en `hecate/crates/application/src/identity/service.rs`: la
  autenticación sin exigir Hécate desbloqueada, el permiso de la política y los recuentos de las mismas fuentes que el
  `status` del canal local (`expiry_counts` de `SecretsService`, `counts` y `enrollment_throttled` de
  `IdentityService` y `status` de `CaService`)
- [X] T038 [US3] Implementar `GET /v1/status` en `hecate/crates/infrastructure/src/api/routes.rs`, que la guarda de
  sellada deja pasar, con `operation_of` y `OPERATIONS`, y dar al componente de prueba
  (`hecate/crates/infrastructure/src/testing.rs`) y al ejemplo (`hecate/crates/infrastructure/examples/component.rs`)
  la orden `status`
- [X] T039 [US3] Añadir `secrets_due_rotation` al `status` del canal en
  `hecate/crates/infrastructure/src/control/operations.rs` y la línea `secrets due rotation: <n>` a `hecate status` en
  `hecate/crates/cli/src/lib.rs`

**Checkpoint**: pasan las pruebas de US3 y las anteriores; el apartado 7 de quickstart.md funciona.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T040 Revisar la memoria (FR-016, research R6) como en las features anteriores y ampliar
  `hecate/crates/application/tests/memory.rs` y las pruebas de memoria de la infraestructura con lo que aparezca:
  - el valor resuelto vive en tipos que se borran desde el descifrado hasta el socket, también en los caminos de error;
  - corregir lo que aparezca y apuntar el resultado para la descripción de la PR;
  - pasar el apartado 8 de `specs/006-hecate-access-policy/quickstart.md`
- [X] T041 Ejecutar `cargo test --workspace` en `hecate/` y comprobar que pasan las 754 pruebas anteriores, con las
  ampliaciones a propósito de las convenciones, y todas las nuevas (FR-020)
- [X] T042 Pasar las comprobaciones de calidad del apartado 2 de `specs/006-hecate-access-policy/quickstart.md`
  (FR-021)
- [X] T043 Ejecutar la suite en el contenedor Linux del apartado 3 de `specs/006-hecate-access-policy/quickstart.md`
  (FR-021)
- [X] T044 Comprobar con el apartado 4 de `specs/006-hecate-access-policy/quickstart.md` que todas las respuestas de las
  operaciones nuevas, también las de error, validan contra los OpenAPI
- [X] T045 Recorrer los apartados 5 a 7 de `specs/006-hecate-access-policy/quickstart.md` con el binario release y el
  ejemplo `component`
- [X] T046 Consolidar los docs según FR-022 y research R11:
  - `docs/hecate.md`: apartados 3, 4, 5, 6, 9 y 10, y la ruta `/v1/secrets:resolve` del diagrama de la resolución;
  - `GLOSSARY.md`: la entrada Access Policy;
  - `ARCHITECTURE.md`, si algo de Identidad y seguridad lo contradice;
  - `shared/contracts`: el OpenAPI de T001, revisado contra lo implementado;
  - `DECISIONS.md`: sacar la decisión de «Abiertas» y añadir a las desviaciones frente a OpenBao / Vault las
    excepciones por secreto concreto, sin comodines ni políticas ligadas a tokens;
  - el ADR 0010, `docs/adr/0010-access-policy-hecate.md` (el modelo de la política, la regla por defecto, las
    excepciones por secreto, el permiso de estado y la igualdad de los rechazos), con su fila en `docs/adr/README.md`;
  - la orden del apartado 9 de quickstart.md no imprime nada
- [X] T047 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `feat:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, con el resultado de T040;
  - deben recoger `hecate/`, `shared/contracts/`, `docs/`, `GLOSSARY.md`, `DECISIONS.md` (y `ARCHITECTURE.md` si
    cambia) y `specs/006-hecate-access-policy/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias; es el contrato.
- **Foundational (Phase 2)**: depende del Setup y bloquea las historias.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2 (Phase 4)**: depende de US1, porque la resolución por la API es la que comprueba las excepciones.
- **US3 (Phase 5)**: depende de US2, porque sus pruebas conceden el permiso de estado con las órdenes del admin, y de
  la API de US1.
- **Polish (Phase 6)**: al final.

### Within Each Phase

- Las pruebas primero, y deben fallar; después, dominio → aplicación → infraestructura → CLI.
- Las tareas que tocan el mismo fichero van en orden.
- Cada fase termina en su checkpoint con la suite completa en verde.

### Parallel Opportunities

- Fase 2: las pruebas T003 a T006 y las implementaciones T008 y T010.
- US1: las pruebas T013 a T017.
- US2: las pruebas T023 a T027 (T024 y T027 amplían ficheros que crea US1).
- US3: las pruebas T031 a T035.

### Parallel Example: Phase 2

```bash
Task: "Pruebas del aggregate en hecate/crates/domain/src/access_policy/policy.rs"
Task: "Pruebas de PolicyEvent en hecate/crates/domain/src/events.rs"
Task: "Pruebas de las operaciones nuevas del registro en hecate/crates/domain/src/sealing/access_log.rs"
Task: "Pruebas del esqueleto de la API en hecate/crates/infrastructure/tests/api_server.rs"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: el contrato, la Access Policy en el dominio y el esqueleto de la API.
2. US1: resolver los propios secretos por la API, con los rechazos iguales en contenido y tiempo.
3. **Revisión del propietario:** las pruebas de US1 y el apartado 5 de quickstart.md.

### Siguientes incrementos

1. US2: compartir secretos, con su revisión (apartado 6 de quickstart.md).
2. US3: el estado y `rotate_every` como duración, con su revisión (apartado 7 de quickstart.md).
3. Polish: memoria, Linux, contrato, quickstart, consolidación y mensaje de commit.

La feature se entrega entera en una PR con un commit de squash `feat:`; los incrementos son puntos de revisión.

## Phase 7: Convergence

- [X] T048 Probar por la API el estado agregado con los campos que hoy solo se ven a `0`, `null` o `false`: en `hecate/crates/infrastructure/tests/api_status.rs`, con el componente de prueba, un intento de renovación sospechoso de otra identidad en las últimas 24 horas, las altas frenadas, una identidad cuyo certificado caduca en menos de 24 horas y el componente dado de alta cuando a la intermedia le quedan menos de 30 días, de modo que `identities_expiring_24h`, `suspicious_renewals_24h`, `intermediate_days_left` y `enrollment_throttled` lleguen con valores distintos de `0`, `null` y `false`, validados contra el OpenAPI. Hoy `identity_status.rs` y `api_status.rs` solo comprueban esos valores por defecto, así que un error al llevarlos de `aggregate_status` (`hecate/crates/application/src/identity/service.rs`) a la respuesta de `routes.rs` no lo detectaría ninguna prueba, per US3-AC1 / FR-011 (partial)
- [X] T049 Añadir una prueba de que un secreto guardado antes de esta feature, con un `rotate_every` de texto libre (`90d`) y su material, se sigue resolviendo, en `hecate/crates/application/tests/secrets_resolve.rs` o `hecate/crates/application/tests/identity_resolve.rs`. Hoy `store_legacy` (`hecate/crates/application/tests/secrets_list_get.rs`) solo escribe sus eventos, y las pruebas comprueban que se lee y que no cuenta entre los que deben rotarse, pero no que se resuelve, como dice el caso límite de la spec, per Edge Cases / FR-023 (partial)
- [X] T050 Añadir a `hecate/crates/cli/tests/cli_secrets.rs` el caso de `hecate secrets create ... --rotate-every 90d`: sale con 1 y escribe `error (validation): rotate_every must be an ISO 8601 duration in days or weeks, such as P90D or P12W`, la salida que fija `specs/006-hecate-access-policy/contracts/cli.md`. Hoy lo comprueban el canal (el código) y la aplicación (el mensaje), pero ninguna prueba de la CLI, per contracts/cli.md / Constitution IV (partial)
- [X] T051 Comprobar en `hecate/crates/infrastructure/tests/identity_no_leak.rs` que las resoluciones no escriben en la base nada más que sus filas: los eventos son los mismos antes y después de ellas. La huella de un valor solo está en la base en los eventos de su secreto, como desde la feature 002, y así la prueba cubre lo que pide T016 sin buscarla en los ficheros. Ajustar el resultado esperado del apartado 8 de `specs/006-hecate-access-policy/quickstart.md`, que hoy dice que la huella no aparece en la base, per T016 / SC-004 (partial)
- [X] T052 Tras T048 a T051, volver a pasar las comprobaciones de calidad del apartado 2 y la suite completa en macOS y en el contenedor Linux del apartado 3 de `specs/006-hecate-access-policy/quickstart.md`: las pasadas de T041 a T043 son anteriores a las pruebas de esta fase, per FR-021 / SC-010 (partial)

## Phase 8: Convergence

- [X] T053 Añadir a la fila de `hecate.v1.json` de `shared/contracts/README.md` la resolución de secretos y el estado agregado, las dos operaciones que esta feature añade al OpenAPI. Hoy la fila solo nombra las de la feature 003 (alta, renovación, lista de revocación, claves de firma de llamadas publicadas y capacidades), per FR-022 (partial)
- [X] T054 Nombrar en los avisos por cronjobs de `docs/hades.md` el permiso de estado de Hécate y `GET /v1/status`, como hacen `docs/hecate.md` y GLOSSARY desde T046, en lugar del «ámbito `status`»: ese nombre ya no está en el doc de Hécate, y en GLOSSARY el ámbito es el de cifrado (Scope), per FR-022 (partial)

## Phase 9: Convergence

- [X] T055 Corregir la ayuda de `--rotate-every` de `hecate secrets create` en `hecate/crates/cli/src/lib.rs`, que hoy dice «Informative rotation period, e.g. 90d»: pone de ejemplo un valor que la CLI rechaza y llama informativo a un periodo que ya cuenta para los secretos que deben rotarse (FR-013). Debe describirlo como una duración ISO 8601 en días o en semanas (`P90D`, `P12W`), comprobarlo con `hecate secrets create --help` y volver a pasar fmt, clippy y la suite completa en macOS y en el contenedor Linux, que T052 pasó antes de este cambio, per FR-023 / contracts/cli.md / FR-021 (contradicts)

## Phase 10: Convergence

- [X] T056 Corregir dos comentarios de pruebas anteriores que dicen lo contrario de lo que hace Hécate desde esta feature: en `hecate/crates/infrastructure/tests/secret_no_leak.rs`, «The resolution has no channel yet», cuando la resolución ya tiene su canal y su prueba de fugas en `identity_no_leak.rs`; y en el comentario del módulo de `hecate/crates/infrastructure/tests/api_identity.rs`, que con Hécate sellada «every route answers `unavailable`», cuando `GET /v1/status` responde (`api_status.rs`). Sin cambiar lo que comprueban ni el nombre de las pruebas (FR-020); basta `cargo fmt --check`, porque los comentarios no cambian el código compilado, per FR-001 / FR-012 (contradicts)
