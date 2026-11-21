---

description: "Tareas del cambio 012: el contrato de llamada de Hermes: repetir con la misma clave y el kid del token"
---

# Tasks: El contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

**Input**: Design documents from `/specs/012-hermes-call-contract/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar. El cambio pide las pruebas de la idempotencia y del envío, del repositorio y de la API
(también sobre PostgreSQL), del contrato y la suite de Hermes en macOS y en Linux
([research R8](research.md#r8-pruebas) y [R10](research.md#r10-la-suite-en-linux)).

**Organization**: las tareas se agrupan por historia de usuario (US1 a US5 de la spec). Dos excepciones al orden de
las prioridades, explicadas en «Dependencies & Execution Order»: US4, el contrato, va antes que las historias que
cambian el comportamiento, porque el principio II pide el contrato escrito antes que el código; y US1 y US2 comparten
fase, porque una sola regla las construye y sus pruebas tienen que fallar antes de ella.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US5)
- Rutas relativas a la raíz del repositorio; las órdenes de uv van con `--package cerbero-hermes`, y las que lanzan la
  Hécate real, con `HECATE_BIN="$PWD/hecate/target/release/hecate"`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar. Las que
  fijan un comportamiento que ya existe (la autenticación sin `kid`, lo que sigue guardándose) pasan desde el principio
  y lo dicen.
- **Dónde van las pruebas:** como en la 008: `hermes/tests/unit/`, `hermes/tests/integration/`, `hermes/tests/contract/`
  y los ayudantes en `hermes/tests/support/`. Las unitarias y la API usan los dobles de `support/doubles.py`
  (`MemoryIdempotency`, `MemoryAudit`) y el `FakeProvider`; el repositorio y la API sobre PostgreSQL, la base de usar y
  tirar (marca `docker`, fixture `migrated`).
- **Nombres fijados en research:**
  - `IdempotencyRepository.release(caller, key, audit)` y `Reservation.release(audit)` (R2);
  - `UNEXPECTED_WHILE_SENDING = "an unexpected failure while sending: the message may have arrived"` (R3);
  - los textos del contrato, los de [contracts/](contracts/README.md) (R4);
  - los textos de DECISIONS, del diseño de alto nivel y del README de Hermes, los de R6, con el apartado «La clave de
    idempotencia de Hermes frente a Stripe y al borrador de la IETF»;
  - el ADR `docs/adr/0014-clave-de-idempotencia-hermes.md`, «Lo que guarda la clave de idempotencia de Hermes» (R7);
  - el parámetro `keys` del arnés `NetworkApi` (R8).
- **Lo que no cambia:** la clasificación de los fallos de cada proveedor, el reintento interno, la vida de las claves,
  el esquema de la base, la API de Hécate, el canal local y las salidas de la CLI (FR-012); ni la primera respuesta de
  un fallo sin envío, que sigue siendo el mismo sobre (R2).
- **Seguridad:** ningún secreto, token ni dirección en los logs, las respuestas ni la auditoría.
- **Lo anterior sigue pasando:** la suite de Hermes de la 008 a la 011, salvo las tres pruebas que el cambio sustituye a
  propósito (R8): el fallo del proveedor guardado en la clave, el `internal` de un fallo inesperado al enviar y la
  versión 1.0.1 del contrato.
- **Docker:** Colima en marcha (`colima start`) para las pruebas de Docker y el E2E.
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: el entorno de las pruebas y la línea base sobre el código de partida.

- [X] T001 Comprobar que la rama activa es `fix/hermes-call-contract` (`git branch --show-current`) y que
  `.specify/feature.json` apunta a `specs/012-hermes-call-contract`
- [X] T002 Poner en marcha Colima (`colima start`, y `docker info` responde) y compilar el binario release de Hécate
  para macOS (`cargo build --release --manifest-path hecate/Cargo.toml`)
- [X] T003 [P] Comprobar que el volumen de Docker `cerbero-hecate-linux` tiene el binario de Hécate para Linux o
  compilarlo con la orden del apartado 1 de [quickstart.md](quickstart.md#1-pruebas-y-calidad), en segundo plano
- [X] T004 [P] Línea base: pasar la suite entera de Hermes en macOS sobre el código de partida (apartado 1 de
  [quickstart.md](quickstart.md#1-pruebas-y-calidad), con `HECATE_BIN` y Docker, el E2E incluido) y anotar el recuento
  para la guía de revisión

**Checkpoint**: Docker responde, los dos binarios de Hécate están listos y la suite de partida pasa.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: liberar la clave reservada, en una transacción con su auditoría (research R2). Nada la usa todavía, así
que el comportamiento no cambia; la usan US1 y US2.

- [X] T005 [P] Pruebas del repositorio en `hermes/tests/integration/persistence/test_idempotency_repository.py` (marca
  `docker`):
  - `release` borra la fila `in_progress` de la clave e inserta la entrada de auditoría en la misma transacción: la
    fila ya no está y la entrada sí;
  - si la entrada no se puede escribir (como en
    `test_an_unknown_outcome_whose_entry_cannot_be_written_leaves_the_key_in_progress`), la fila sigue `in_progress` y
    no se escribe nada;
  - solo toca la clave de su `caller`: la misma clave de otro sigue;
  - con la base caída, `unavailable`, como `test_a_database_that_does_not_answer_is_unavailable`
- [X] T006 [P] Pruebas de la reserva en `hermes/tests/unit/application/test_idempotency.py`, con `MemoryIdempotency`:
  - `Reservation.release(audit)` quita la fila, deja la entrada en la auditoría y `response` sigue en `None`;
  - con dos peticiones iguales a la vez, si la primera libera su reserva, la segunda no encuentra fila, hace su propio
    `perform` y reserva la clave, como en
    `test_requests_with_the_same_key_at_once_run_one_by_one_and_only_the_first_reaches_the_provider`
- [X] T007 Implementar la liberación (research R2), con los docstrings al día:
  - `release(caller, key, audit)` en el Protocol `IdempotencyRepository` de `hermes/src/hermes/application/ports.py`,
    con su docstring: borra la fila `in_progress` en la misma transacción que la entrada de auditoría del intento;
  - `SqlIdempotencyRepository.release` en
    `hermes/src/hermes/infrastructure/persistence/idempotency_repository.py`: el `DELETE` de la fila `in_progress` de
    la clave y `insert_entry` dentro del mismo `begin()`, y `DATABASE_DOWN` como `database_unavailable()`; el docstring
    del módulo nombra la liberación;
  - `MemoryIdempotency.release` en `hermes/tests/support/doubles.py`, que quita la fila y añade la entrada a su
    auditoría, como `complete`, y su docstring lo dice;
  - `Reservation.release(audit)` en `hermes/src/hermes/application/idempotency.py`: el docstring de la clase dice que
    la fila se completa con lo que se envió o pudo enviarse o se libera con la entrada de un fallo sin envío, y el paso
    3 del docstring del módulo, que con un fallo sin envío la fila se libera

**Checkpoint**: pasan las pruebas de la liberación y la suite de Hermes sigue pasando sin cambios de comportamiento.

---

## Phase 3: User Story 4 - El contrato dice cómo es el token (Priority: P2)

**Goal**: el contrato de Hermes dice que la cabecera del token corto lleva el `kid`, qué recibe una repetición con la
misma clave y las respuestas 200, 409 y 500 del envío; va antes que el código que cambia el comportamiento
(principio II).

**Independent Test**: el apartado 3 de [quickstart.md](quickstart.md#3-el-contrato) y las pruebas de contrato.

### Pruebas de la historia

- [X] T008 [US4] Pruebas de contrato en `hermes/tests/contract/test_openapi_schema.py` (FR-007, FR-008, historia 4,
  escenarios 1 y 2):
  - `test_both_documents_are_openapi_3_1_with_their_license_and_version` espera `hermes.v1.json` 1.0.2;
  - la descripción de `callToken` nombra la cabecera con `kid` y dice que un token sin él responde `auth`;
  - el parámetro `IdempotencyKey` tiene descripción, y dice que la clave guarda un envío completado o un `timeout` con
    `outcome_unknown`, que mientras se guarda otro cuerpo con ella es `conflict` y que tras un fallo que garantiza que
    no se envió nada la petición siguiente con ella es un intento nuevo;
  - la respuesta 200 de `sendMessage` nombra el `timeout` con `outcome_unknown` tras un fallo inesperado; la 409 dice
    que la clave guarda otra petición (`kept for another request`), y la 500, el fallo inesperado antes del proveedor;
    las 500 de las capacidades y de la prueba de conexión no cambian
- [X] T009 [P] [US4] Prueba de la autenticación en `hermes/tests/unit/application/test_authentication.py` (FR-007,
  historia 4, escenario 3): un token sin `kid` en la cabecera responde `auth` con el mensaje de siempre, sin preguntar
  a Hécate, y el log da el motivo `unknown_key`. Fija lo que ya hace: pasa desde el principio

### Implementación de la historia

- [X] T010 [US4] Editar `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json` con los textos de
  [contracts/](contracts/README.md) (research R4): `info.version` 1.0.2, `info.description`, la descripción de
  `callToken`, la descripción nueva de `IdempotencyKey` y las respuestas 200, 409 y 500 de
  `POST /chat/v1/send_message`, sin tocar esquemas, operaciones ni parámetros obligatorios

**Checkpoint**: pasan las pruebas de contrato de Hermes y las de Hécate; `git diff main -- shared/contracts` solo
cambia descripciones y la versión.

---

## Phase 4: User Story 1 y User Story 2 - Repetir con la misma clave tras un fallo sin envío (Priority: P1 y P2) 🎯 MVP

**Goal**: un fallo que garantiza que no se envió nada, reintentable o no, deja la clave libre: el reintento de Hades
(US1) y la repetición tras arreglar la causa (US2) llegan al proveedor con la misma clave.

**Independent Test**: con el `fake`, tras un fallo sin envío, la repetición con la misma clave envía una vez y deja
una entrada de auditoría por intento (apartado 2 de [quickstart.md](quickstart.md#2-las-pruebas-del-cambio-solas)).

### Pruebas de la historia

- [X] T011 [US1] Pruebas del envío en `hermes/tests/unit/application/test_send_message.py`, con `execute_once` sobre una
  `Reservation` de `MemoryIdempotency` (FR-001, FR-002, FR-005). `World` gana `keys = MemoryIdempotency(self.audit)` y
  un render de prueba que guarda el resultado que recibe:
  - con `provider_unavailable` con `retryable` (no conecta) y con el `rate_limited` del proveedor con su
    `retry_after_s`, la clave queda sin fila, la auditoría tiene una entrada con el resultado del fallo y se lanza el
    mismo `ProviderFailure`;
  - con un envío completado o con `timeout` y `outcome_unknown`, la fila queda `completed` con su respuesta, como hoy
    (FR-004): pasan desde el principio
- [X] T012 [US2] Pruebas del envío, en el mismo fichero y con el `World` de T011 (FR-002, FR-003): con las credenciales
  rechazadas (`provider_unavailable` sin `retryable`, con `credentials_rejected`), la petición rechazada por el
  proveedor (`provider_unavailable` sin `retryable`), la persona que no admite el bot (`conflict` del proveedor) y un
  `HermesError` que el proveedor lanza antes de enviar (`internal`, como la instancia sin token del bot), la clave queda
  sin fila, la auditoría tiene la entrada y se lanza el fallo
- [X] T013 [US1] Pruebas de la API en `hermes/tests/integration/api/test_idempotency.py` (historia 1, SC-001):
  - tras `fail_next(FakeFailure.UNREACHABLE, times=20)`, como en
    `test_a_failure_of_the_provider_answers_200_inside_the_envelope` de
    `hermes/tests/integration/api/test_send_route.py`, la respuesta es 200 con `provider_unavailable` y
    `retryable: true`; con los fallos quitados (`clear_failures()`), la repetición con la misma clave responde
    `completed`, el `fake` tiene un envío y la auditoría, dos entradas;
  - lo mismo con `FakeFailure.RATE_LIMITED`, cuya espera no cabe en el timeout: 200 con `rate_limited` y
    `retry_after_s`, y la repetición tras quitarlo envía;
  - tras ese envío, una petición más con la misma clave devuelve `completed` sin volver a enviar: el `fake` sigue con
    un envío (escenario 3);
  - tras un fallo sin envío, un reintento que acaba en `FakeFailure.CUT_AFTER_SEND` deja el `timeout` con
    `outcome_unknown` en la clave, y la repetición siguiente lo devuelve sin volver a enviar (caso límite);
  - si la liberación no se puede escribir (`CutKeys` corta también `release` cuando tiene `cut_next`, y su docstring
    dice que el corte cae en el registro o en la liberación del envío siguiente), la petición responde `unavailable`;
    con los fallos quitados, la repetición responde `timeout` con `outcome_unknown` y el `fake` no tiene ningún envío
    (caso límite). Pasa antes y después de T016: antes, el corte cae en `complete`
- [X] T014 [US2] Pruebas de la API, en el mismo fichero (historia 2, SC-002):
  - sustituir `test_a_failure_of_the_provider_is_the_response_of_its_key` por la repetición tras
    `FakeFailure.CREDENTIALS_REJECTED` y `FakeFailure.RECIPIENT_REFUSES`, parametrizada: la primera responde 200 con su
    código; quitado el fallo, la repetición con la misma clave envía y la auditoría tiene dos entradas;
  - con la misma parametrización y sin arreglar la causa (el fallo para dos intentos, `times=2`), la repetición responde
    el mismo fallo, el `fake` no tiene ningún envío y la auditoría tiene otra entrada, porque llegó otra vez al
    proveedor (escenario 3);
  - la misma clave con otro cuerpo tras un fallo sin envío es una petición nueva: responde según su resultado, sin
    `conflict` (caso límite)
- [X] T015 [P] [US1] Prueba de la API sobre PostgreSQL en el fichero nuevo
  `hermes/tests/integration/api/test_idempotency_postgres.py` (marcas `anyio` y `docker`, fixture `migrated`), con el
  arnés `NetworkApi` de `hermes/tests/support/network_api.py` (research R8), construido como en el fixture de
  `hermes/tests/integration/api/test_capabilities.py`:
  - `NetworkApi` gana un parámetro `keys` opcional, el repositorio de las claves; sin él, `MemoryIdempotency` sobre su
    auditoría, como hoy, y el docstring del módulo lo dice;
  - la prueba le pasa `SqlIdempotencyRepository(migrated)` y lee la auditoría con `audit_rows` de
    `hermes/tests/support/audit_log.py`: tras un fallo sin envío del `fake`, la tabla `idempotency_keys` no tiene la
    fila y la de auditoría tiene una entrada; la repetición con la misma clave envía, deja la fila `completed` y otra
    entrada;
  - el docstring del módulo nuevo dice qué recorre

### Implementación de la historia

- [X] T016 [US1] Liberar la clave en `_Reserved.after_send` de `hermes/src/hermes/application/send_message.py` (research
  R2): completa la fila con un `SendResult` o con un `HermesError` con `outcome_unknown`, y la libera con
  `Reservation.release(entry)` con cualquier otro `HermesError`. Los docstrings al día: el del módulo y el de
  `execute_once` dicen que un fallo sin envío libera la clave y se lanza, y el del módulo
  `hermes/src/hermes/infrastructure/api/routes_chat.py`, que la clave guarda lo que se envió o pudo enviarse

**Checkpoint**: US1 y US2 funcionan: tras un fallo sin envío, la repetición con la misma clave llega al proveedor; la
primera respuesta sigue siendo el mismo sobre (`test_a_failure_of_the_provider_answers_200_inside_the_envelope`, de
`test_send_route.py`, pasa sin cambios); pasan sus pruebas y la suite de Hermes en macOS.

---

## Phase 5: User Story 3 - Lo que pudo enviarse no se repite (Priority: P1)

**Goal**: un envío completado o con resultado desconocido nunca se vuelve a enviar con su clave, y un fallo inesperado
al enviar es un resultado desconocido: `timeout` con `outcome_unknown`, guardado.

**Independent Test**: un fallo inesperado al enviar y un corte del `fake` devuelven a toda repetición la misma
respuesta sin volver a enviar (apartado 2 de [quickstart.md](quickstart.md#2-las-pruebas-del-cambio-solas)).

### Pruebas de la historia

- [X] T017 [US3] Pruebas del envío en `hermes/tests/unit/application/test_send_message.py`, con el `World` de T011
  (FR-004, research R3):
  - `test_an_unexpected_failure_while_sending_is_audited_too` pasa a esperar
    `{"status": "failed", "code": "timeout", "outcome_unknown": True}`;
  - con `execute_once`, una excepción inesperada al enviar completa la fila, y el render recibe un `ProviderFailure`
    con `timeout`, `from_provider`, `retryable` falso, `outcome_unknown` y el mensaje `UNEXPECTED_WHILE_SENDING` (el
    HTTP 200 lo pone la ruta, y lo comprueba T018); el log deja `send.failed` con el tipo de la excepción, y el estado
    de la instancia no la cuenta;
  - una excepción inesperada antes de reservar la clave no deja fila
- [X] T018 [US3] Pruebas de la API en `hermes/tests/integration/api/test_idempotency.py` (historia 3, escenario 4): con
  `monkeypatch` sobre `send` del `FakeProvider` de `Api`, que lanza una excepción inesperada (`FakeFailure` no cambia,
  FR-012), la respuesta es 200 con `timeout` y `outcome_unknown: true`, y su repetición con la misma clave, lo mismo
  sin llamar al proveedor, con una sola entrada de auditoría. Las pruebas de hoy del envío completado, del corte y de
  la clave con otro cuerpo siguen sin cambios
- [X] T019 [P] [US3] Ampliar `hermes/tests/integration/api/test_idempotency_postgres.py` con la historia 3: tras
  `FakeFailure.CUT_AFTER_SEND`, la fila queda `completed` con el `timeout`, la repetición devuelve lo mismo y el `fake`
  tiene un solo envío. Pasa desde el principio: es lo que hace hoy

### Implementación de la historia

- [X] T020 [US3] Convertir el fallo inesperado al enviar en `_execute` de
  `hermes/src/hermes/application/send_message.py` (research R3): una excepción que no es `HermesError` tras reservar la
  clave pasa a `ProviderFailure(ErrorCode.TIMEOUT, UNEXPECTED_WHILE_SENDING, retryable=False, outcome_unknown=True)`,
  con la constante nueva; el log `send.failed`, la excepción que se vuelve a lanzar y la ausencia de `record_call`
  siguen como hoy; y retirar `UNEXPECTED` si queda sin uso

**Checkpoint**: US1, US2 y US3 funcionan: lo que no salió se repite con la misma clave, y lo que pudo salir, nunca;
pasan sus pruebas y la suite de Hermes en macOS.

---

## Phase 6: User Story 5 - Los documentos dicen lo mismo (Priority: P3)

**Goal**: la guía de integración, el diseño de las llamadas, la tabla de códigos, el contrato y el diseño de Hades dan
la misma regla de repetición; la decisión queda en el ADR 0014 y en DECISIONS, con su referencia frente a Stripe y al
borrador de la IETF; el diseño de alto nivel y el README de Hermes la nombran, y el README de Hécate lista la 009.

**Independent Test**: las comprobaciones del apartado 5 de [quickstart.md](quickstart.md#5-los-documentos) y la
lectura conjunta de los documentos para cada código (SC-005).

### Pruebas de la historia

- [X] T021 [US5] Ejecutar las comprobaciones del apartado 5 de [quickstart.md](quickstart.md#5-los-documentos) sobre los
  documentos de partida y comprobar que fallan:
  - `otra clave` aparece en las reglas de reintento de `docs/hermes/integracion.md`, y la guía dice que la clave «ya se
    usó con otra petición»;
  - DECISIONS tiene la fila abierta del reintento, y no el ADR 0014 ni la referencia de Stripe;
  - el README de Hécate no tiene la 009, ni el de Hermes la 012, y ni el diseño de alto nivel ni el README de Hermes
    nombran el ADR 0014;
  - el glosario no dice qué guarda la clave en Hermes;
  - las órdenes de las palabras de tiempo no dicen nada sobre los documentos de partida: sin cambios no hay líneas que
    mirar, y el ADR no existe
- [X] T022 [P] [US5] Escribir `docs/adr/0014-clave-de-idempotencia-hermes.md` con las cuatro secciones de la plantilla
  de `docs/adr/README.md` (research R7: contexto, con las referencias de Stripe y del borrador de la IETF; opciones;
  decisión; consecuencias) y añadir su fila al índice de `docs/adr/README.md`
- [X] T023 [P] [US5] Actualizar `docs/hermes/integracion.md` (research R5 y R6):
  - en «La `Idempotency-Key`», que la clave guarda solo lo que se envió o pudo enviarse y que tras un fallo sin envío
    se repite con la misma; la viñeta de la misma clave con otro cuerpo pasa al texto de R6;
  - en la tabla de errores, la regla de R5 para `rate_limited` y `provider_unavailable` del proveedor, `conflict` 200,
    `timeout` (también tras un fallo inesperado al enviar) e `internal` (antes del proveedor), y la fila de `conflict`
    409 dice «…o la `Idempotency-Key` ya guarda otra petición»
- [X] T024 [P] [US5] Actualizar `docs/hermes/llamadas.md`: en «La idempotencia», que la reserva se completa con lo que
  se envió o pudo enviarse y se libera tras un fallo sin envío, y la viñeta de la misma clave con otro cuerpo pasa al
  texto de R6; en «Los reintentos seguros», que Hades reintenta con la misma clave; en «El resultado desconocido», el
  fallo inesperado al enviar
- [X] T025 [P] [US5] Actualizar la tabla de códigos de `docs/hermes/contratos.md`: `conflict`, «…o una clave de
  idempotencia que ya guarda otra petición»; `internal`, un fallo inesperado antes del proveedor; `timeout`, también
  tras un fallo inesperado al llamar al proveedor
- [X] T026 [P] [US5] Actualizar en `docs/hades.md` «Relanzar, idempotencia y compensación»: un reintento o un
  relanzamiento con el mismo input usa la misma clave, y en Hermes la clave solo guarda lo que se envió o pudo enviarse,
  con el enlace a sus llamadas; sin extender la regla a Estigia (research R6). La viñeta sigue en una sola línea, como
  las demás del fichero
- [X] T027 [P] [US5] Actualizar la entrada Idempotency Key de `GLOSSARY.md`: en Hermes, la clave guarda la respuesta de
  lo que se envió o pudo enviarse; con otro cuerpo, mientras la guarda, `conflict`; un fallo que garantiza que no se
  envió nada no la guarda
- [X] T028 [P] [US5] Actualizar `DECISIONS.md` con los textos de research R6: el apartado «La clave de idempotencia de
  Hermes frente a Stripe y al borrador de la IETF», tras el del acceso por app frente a Istio, con sus dos desviaciones;
  la fila del ADR 0014 en Tomadas; y retirar de las abiertas «Reintento de Hades tras un fallo reintentable de Hermes»
- [X] T029 [P] [US5] Añadir a la tabla de specs de `hecate/README.md` la fila de
  [009-app-access](../009-app-access/): la lista de acceso de cada app, guardada en Hécate, gestionada con su CLI y
  leída por cada app por la API
- [X] T030 [P] [US5] Nombrar la decisión en Hermes con los textos de research R6:
  - en `hermes/README.md`, la fila de la 012 en «Las specs», el ADR 0014 en la frase de las decisiones («los ADR 0011,
    0012, 0013 y 0014», cada uno con su enlace) y, en la tabla de ayudantes, que `network_api.py` recibe las claves en
    memoria o en el PostgreSQL que le pase la prueba (T015);
  - en `docs/hermes.md`, «Decisiones y riesgos»: la viñeta de la clave de idempotencia tras «Frente a SecOps», con el
    enlace al apartado de DECISIONS, y el ADR 0014 al final de «Tomadas»
- [X] T031 [US5] Pasar las comprobaciones del apartado 5 de quickstart.md, también las de las palabras de tiempo, y leer
  juntos, para cada código de error, la guía de integración, el diseño de las llamadas, la tabla de códigos, el
  contrato y el diseño de Hades (SC-005). Corregir lo que no coincida, comprobar los enlaces nuevos y que las líneas de
  prosa que escribe el cambio no pasan de 120 caracteres (`docs/hades.md` tiene una línea por viñeta, y la viñeta que
  se edita sigue ese estilo), y anotar el resultado para la guía de revisión

**Checkpoint**: las cinco historias funcionan y los documentos describen lo construido.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T032 Pasar la calidad y las suites de los apartados 1 a 4 de [quickstart.md](quickstart.md#1-pruebas-y-calidad),
  con lo esperado de cada uno:
  - ruff, el formato, `mypy --strict` y pip-audit, sin avisos;
  - la suite entera de Hermes en macOS, con `HECATE_BIN` y Docker, el E2E incluido;
  - en el contenedor `python:3.15.2-slim-trixie`, las pruebas sin Docker, con el binario de Hécate para Linux de T003;
  - las comprobaciones del contrato con `jq`;
  - anotar los recuentos para la guía de revisión
- [X] T033 Escribir la guía de revisión `specs/012-hermes-call-contract/summary_review.md`: qué cambia y por qué, las
  decisiones de research (R0 a R11), las pruebas nuevas y las tres sustituidas, con sus recuentos en macOS y Linux
  frente a la línea base de T004, el diff del contrato, los documentos que cambian y qué mirar al revisar
- [X] T034 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `fix:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, terminada con la ruta de la guía de revisión;
  - deben recoger `hermes/`, `shared/contracts/`, `docs/`, `GLOSSARY.md`, `DECISIONS.md`, `hecate/README.md` y
    `specs/012-hermes-call-contract/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias; T003 y T004 necesitan Colima (T002).
- **Foundational (Phase 2)**: depende del Setup y bloquea US1 y US2, que liberan la clave con lo que construye.
- **US4 (Phase 3)**: depende del Setup. Va antes que US1, US2 y US3 aunque su prioridad es P2: el principio II pide el
  contrato escrito antes que el código que cambia el comportamiento.
- **US1 y US2 (Phase 4)**: dependen de la fase fundacional y de US4. Comparten fase porque T016 construye las dos: sus
  pruebas (T011 a T015) se escriben y fallan antes que él.
- **US3 (Phase 5)**: depende de US4. Va después de US1 y US2 porque T020 y T016 tocan el mismo fichero, T017 usa el
  `World` de T011 y T019 amplía el fichero de T015.
- **US5 (Phase 6)**: depende de US3, US1, US2 y US4: documenta lo que construyen.
- **Polish (Phase 7)**: depende de todas las historias. T034 se hace tras converge, como en los cambios anteriores.

### Within Each User Story

- Las pruebas de la historia, primero, y fallando (salvo las que fijan lo que ya existe, que lo dicen).
- Después, la implementación, y el checkpoint de la fase con la suite de Hermes en macOS.

### Parallel Opportunities

- T003 y T004, en el Setup.
- T005 y T006, en la fase fundacional (ficheros distintos).
- T009 con T008, en US4.
- T015 con T011 a T014, en US1 y US2; T019 con T017 y T018, en US3.
- T022 a T030, en US5: ninguna comparte fichero con otra.

---

## Parallel Example: User Story 5

```bash
# Los documentos, cada uno en sus ficheros, a la vez:
Task: "T022 [P] [US5] docs/adr/0014-clave-de-idempotencia-hermes.md y docs/adr/README.md"
Task: "T023 [P] [US5] docs/hermes/integracion.md"
Task: "T024 [P] [US5] docs/hermes/llamadas.md"
Task: "T025 [P] [US5] docs/hermes/contratos.md"
Task: "T026 [P] [US5] docs/hades.md"
Task: "T027 [P] [US5] GLOSSARY.md"
Task: "T028 [P] [US5] DECISIONS.md"
Task: "T029 [P] [US5] hecate/README.md"
Task: "T030 [P] [US5] hermes/README.md y docs/hermes.md"
```

---

## Implementation Strategy

### MVP First

1. Setup (T001-T004) y la liberación de la clave (T005-T007).
2. El contrato (US4, T008-T010), antes del comportamiento.
3. US1 y US2 (T011-T016): la repetición con la misma clave tras un fallo sin envío.
4. **Parar y validar:** el checkpoint de la fase 4 y la revisión del propietario del MVP.

### Incremental Delivery

1. US3 (T017-T020): el fallo inesperado como resultado desconocido; checkpoint.
2. US5 (T021-T031): el ADR, DECISIONS y los documentos; checkpoint.
3. Polish (T032-T033), converge y, al final, T034.
