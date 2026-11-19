---

description: "Tareas del arreglo 011: Hermes nota la revocación urgente de su propio certificado"
---

# Tasks: Hermes nota la revocación urgente de su propio certificado

**Input**: Design documents from `/specs/011-hermes-own-revocation/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar. El arreglo pide en concreto las unitarias del servicio de identidad, el escenario de la
revocación en el E2E del compose y la suite de Hermes en macOS y en Linux ([research R8](research.md#r8-pruebas) y
[R9](research.md#r9-la-suite-en-linux)).

**Organization**: las tareas se agrupan por historia de usuario (US1 a US3 de la spec), en el orden de sus prioridades.
US2 se apoya en US1, y US3 documenta lo que construyen las dos; el orden está en «Dependencies & Execution Order».

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US3)
- Rutas relativas a la raíz del repositorio; las órdenes de uv van con `--package cerbero-hermes`, y las que lanzan la
  Hécate real, con `HECATE_BIN="$PWD/hecate/target/release/hecate"`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar.
- **Dónde van las pruebas:** como en la 008: `hermes/tests/unit/`, `hermes/tests/integration/`, `hermes/tests/e2e/` y
  los ayudantes en `hermes/tests/support/`. Las unitarias usan `FakeHecate`, `MemoryIdentityStore` y `FakeClock`; la
  integración, una Hécate real (marca `hecate`); el E2E, el compose de `hermes/deploy/` (marcas `docker` y `hecate`).
- **Nombres fijados en research:**
  - el método `_as_hermes(operation, call)` y el nuevo `resolve` del servicio de identidad, y el port `SecretResolver`;
  - las operaciones `renew`, `revocations`, `keys`, `key`, `callers` y `resolve` (R2);
  - el mensaje «hecate refused the identity of hermes: it needs a new enrollment token» (R4);
  - la línea `identity.refused`, de nivel warning, con `serial` y `operation` (R5);
  - la tarea de la contraseña de la base tras cada alta, `_password_after_enrollments`, con `wait_for_enrollment` y
    `_refresh_password` (R7).
- **Lo que no cambia:** ningún contrato, motivo de la salud ni salida de la CLI (FR-009); ni Hécate, ni el almacén de
  la identidad, ni el arranque, ni la base de Hermes (R6, R7 y R11).
- **Seguridad:** ningún secreto, token, clave ni certificado en los logs ni en las respuestas; la línea nueva solo
  lleva la serie y la operación.
- **Lo anterior sigue pasando:** la suite de Hermes de la 008 a la 010, con la ampliación a propósito de la prueba de la
  renovación rechazada (R8).
- **Docker:** Colima en marcha (`colima start`) para las pruebas de Docker, el E2E y el recorrido; el E2E dura unos 4
  minutos más.
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: el entorno de las pruebas y la línea base sobre el código de partida.

- [X] T001 Comprobar que la rama activa es `fix/hermes-own-revocation` (`git branch --show-current`) y que
  `.specify/feature.json` apunta a `specs/011-hermes-own-revocation`
- [X] T002 Poner en marcha Colima (`colima start`, y `docker info` responde) y compilar el binario release de Hécate
  para macOS (`cargo build --release --manifest-path hecate/Cargo.toml`)
- [X] T003 [P] Compilar el binario de Hécate para Linux en el volumen de Docker `cerbero-hecate-linux`, con la orden del
  apartado 1 de [quickstart.md](quickstart.md#1-pruebas-y-calidad), en segundo plano
- [X] T004 [P] Línea base: pasar la suite entera de Hermes en macOS sobre el código de partida (apartado 1 de
  [quickstart.md](quickstart.md#1-pruebas-y-calidad), con `HECATE_BIN` y Docker, el E2E incluido) y anotar el recuento
  para la guía de revisión

**Checkpoint**: Docker responde, los dos binarios de Hécate están compilados y la suite de partida pasa.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: el doble de Hécate hace lo que hace Hécate tras una revocación urgente (research R8). Lo usan las pruebas
de las tres historias.

- [X] T005 Pruebas del doble en `hermes/tests/unit/application/test_identity_state.py`, como la de la regla del segundo
  intento en `test_renewal.py`:
  - tras `revoke_urgently(<serie>)`, toda llamada autenticada con esa serie (`renew`, `revocations`, `keys`, `key`,
    `callers` y `resolve`) recibe `auth`, y la serie entra en su lista de revocación;
  - el alta, que no va autenticada, sigue funcionando, y las llamadas con otra serie, también;
  - `authenticated_calls` cuenta cada llamada autenticada que le llega, también las rechazadas, y no las altas;
  - con `hold_next`, la siguiente llamada autenticada espera a que se libere y se decide con la serie con la que
    empezó;
  - sin series rechazadas ni `hold_next`, se comporta como hoy
- [X] T006 Implementar en `hermes/tests/support/fake_hecate.py` lo que prueba T005:
  - `refused`, las series rechazadas, en hexadecimal como `IssuedCertificate.serial`, y `revoke_urgently(serial)`, que
    la añade a `refused` y a `revoked`;
  - `authenticated_calls` y `hold_next: asyncio.Event | None`, que gasta la siguiente llamada autenticada;
  - cada llamada autenticada cuenta, lee `caller_serial()` al empezar, espera si le toca, pasa por `_answering()` y
    responde `auth` si su serie está rechazada;
  - el docstring del módulo

**Checkpoint**: pasan las pruebas del doble y la suite de Hermes sigue pasando.

---

## Phase 3: User Story 1 - Hermes deja de estar listo tras la revocación urgente de su certificado (Priority: P1) 🎯 MVP

**Goal**: tras una revocación urgente, Hermes nota en la primera llamada que Hécate le rechaza, y como mucho en el
siguiente refresco, que su identidad no vale: no está listo, su API responde `unavailable`, su log lo dice una vez y
deja de llamar a Hécate con ella.

**Independent Test**: en el compose de pruebas, con Hermes listo, revocar con urgencia su certificado: la salud dice
`not ready: needs a new enrollment token` en 300 s como mucho y un envío responde `unavailable` sin enviar nada (T013,
los pasos 1 a 6 del apartado 3 de quickstart.md).

### Pruebas de la historia

- [X] T007 [US1] Pruebas de los caminos que marcan en `hermes/tests/unit/application/test_identity_state.py` (FR-001,
  FR-002, historia 1, escenarios 1 y 6), tras `revoke_urgently` de la serie de Hermes:
  - el refresco de las publicaciones, `resolve`, la comprobación de los secretos al aplicar la configuración
    (`HecateSecretChecker` sobre el servicio), una clave que la lista no tiene (`key`), la lista de acceso ante quien
    no está en la copia (`lookup_access`) y la renovación (`renewal_tick` a los dos tercios);
  - con cada uno, la primera llamada que Hécate rechaza deja `problem()` en `needs a new enrollment token` y
    `material()` en `None`, sin reiniciar el servicio;
  - cada camino falla como hoy ante ese rechazo: el refresco deja `publications.refresh_failed` con el motivo `auth`,
    `resolve` responde `auth`, y la comprobación de la configuración, `key` y `lookup_access`, `unavailable`;
  - `test_a_renewal_refused_with_auth_needs_a_new_token` comprueba además que el siguiente `renewal_tick` no vuelve a
    llamar a Hécate
- [X] T008 [US1] Pruebas de lo que no marca, en el mismo fichero (FR-003, FR-009, SC-004, historia 1, escenarios 4
  y 5):
  - un refresco o una resolución que falla con cualquier código que no sea `auth`, o con Hécate sellada o caída, deja
    la identidad válida;
  - una hora de Hécate sellada, y otra de Hécate caída, con un refresco cada 240 s: `problem()` sigue en `None` y
    `material()` da la última copia;
  - la revocación urgente del certificado de otro componente: Hermes sigue listo y, tras el refresco, esa serie está en
    su lista de revocación;
  - la CA sustituida (`HecateTrustChanged`) da `needs a new enrollment token` como hoy, pero no bloquea: el refresco
    siguiente llega a Hécate y, si responde con la CA de siempre, la identidad vuelve a valer
- [X] T009 [US1] Pruebas del bloqueo, en el mismo fichero (FR-008, SC-006, historia 1, escenario 7): con la identidad
  marcada, `authenticated_calls` no sube y cada camino falla como si Hécate lo rechazara:
  - el refresco deja `publications.refresh_failed` con el motivo `auth` y conserva su copia;
  - `resolve` responde `auth` con «hecate refused the identity of hermes: it needs a new enrollment token»;
  - la comprobación de la configuración, `key` y `lookup_access` responden `unavailable`;
  - `renewal_tick` devuelve `RETRY_S` sin llamar
- [X] T010 [US1] Pruebas de la línea del log y del reinicio, en el mismo fichero:
  - el primer rechazo deja una sola línea `identity.refused`, de nivel warning, con `serial`, la de la identidad, y
    `operation`, la llamada rechazada; una llamada que ya estaba en curso (retenida con `hold_next`) recibe también su
    rechazo y no deja otra línea, y las bloqueadas tampoco; ningún campo lleva un PEM, una clave ni un token (FR-005);
  - un servicio nuevo sobre el mismo almacén, como tras un reinicio, vale (`problem()` es `None`) hasta que Hécate le
    rechaza su primera llamada, la resolución de la contraseña de la base; entonces da el motivo y vuelve a dejar
    `identity.refused` (FR-007)
- [X] T011 [P] [US1] Pruebas de la caché en `hermes/tests/unit/application/test_secret_cache.py`, sobre el servicio de
  identidad con la identidad marcada (research R4): dentro del tiempo blando da el valor guardado sin llamar; pasado,
  la resolución falla con `auth`, quita el valor (`current()` responde `internal`) y no llega a `FakeHecate`
- [X] T012 [P] [US1] Pruebas contra una Hécate real en `hermes/tests/integration/hecate/test_client.py` (marca
  `hecate`), con el Hermes del fixture `hermes`:
  - tras `prepared.cli("certificates", "revoke", <serie>, "--urgent")`, una llamada del cliente recibe `auth` en el
    sobre de Hécate: un `HermesError` con el código `auth` que no es `HecateTrustChanged` (Hécate no corta el TLS);
  - el refresco de las publicaciones deja la identidad en `needs a new enrollment token`, y la resolución siguiente por
    el servicio responde `auth` con el mensaje de Hermes;
  - cada llamada rechazada deja una fila de Hermes en el registro de accesos de Hécate, y la bloqueada no deja ninguna
    (SC-006): las filas con `subject` = el SPIFFE ID de Hermes en la tabla `access_log` de `hecate.db`, en el
    directorio de datos de la Hécate de pruebas, leída en solo lectura; las órdenes del admin sobre Hermes, como la
    revocación, llevan `subject` = `admin`;
  - un Hermes nuevo sobre los mismos directorios (`hermes_for`), como tras un reinicio, vale hasta su primera llamada,
    que Hécate rechaza: el rechazo no se guarda (FR-007)
- [X] T013 [P] [US1] Escenario nuevo en `hermes/tests/e2e/test_flow.py` (marcas `docker` y `hecate`), después del
  certificado revocado de un componente y antes de PostgreSQL parado:
  - lee la serie de Hermes del campo `serial` de `/var/lib/hermes/state/identity.json` y la revoca con urgencia
    (`hecate certificates revoke <serie> --urgent`);
  - la salud llega a `(1, "not ready: needs a new enrollment token\n")` en 300 s como mucho (SC-001);
  - el log de Hermes tiene una sola línea `identity.refused`, con esa serie;
  - un envío de `caller` por Envoy responde 503 `unavailable` y la bandeja de salida del fake no cambia (SC-002);
  - al terminar, da de alta a Hermes con un token nuevo y espera a `ready`, para que sigan las pruebas siguientes; T021
    lo amplía con las comprobaciones de la historia 2;
  - el docstring del módulo nombra el escenario

### Implementación de la historia

- [X] T014 [US1] Definir `SecretResolver` en `hermes/src/hermes/application/ports.py`, y añadirlo a `__all__`: un
  Protocol con solo `async def resolve(self, references: Sequence[SecretReference]) -> tuple[ResolvedSecret, ...]`,
  que cumplen el cliente de Hécate, `FakeHecate` y el servicio de identidad
- [X] T015 [US1] Implementar el rechazo en `hermes/src/hermes/application/identity.py` (research R2 a R5):
  - la marca `self._refused: str | None`, la serie de la identidad rechazada, en lugar del booleano: `problem()` da
    `NEEDS_TOKEN` si es la de la identidad vigente, y `enroll()` la borra;
  - `_as_hermes(operation, call)`: con la serie vigente marcada, responde `HermesError(auth, <mensaje de R4>)` sin
    llamar; si no, llama y, ante un `HermesError` con el código `auth` que no es `HecateTrustChanged`, marca la serie
    de la identidad con la que llamó, leída antes de la llamada, y la primera vez deja `identity.refused`;
  - por él pasan `renew` (`renew`), las tres llamadas de `_fetch_publications` (`revocations`, `keys` y `callers`),
    `_ask_key` (`key`), `_ask_access` (`callers`) y el nuevo `resolve(references)` (`resolve`);
  - `renewal_tick` deja de poner la marca, que ya pone `_as_hermes`;
  - el docstring del módulo: la validez cuenta el rechazo de cualquier llamada
- [X] T016 [P] [US1] Hacer que `SecretCache` en `hermes/src/hermes/application/secrets.py` y `HecateSecretChecker` en
  `hermes/src/hermes/application/configuration.py` dependan de `SecretResolver` en lugar de `HecateApi`, sin cambiar
  su comportamiento
- [X] T017 [US1] Dar el servicio de identidad a la caché y a la comprobación en
  `hermes/src/hermes/infrastructure/main.py`: `SecretCache(self.identity, self.clock)` y
  `HecateSecretChecker(self.identity)`

**Checkpoint**: US1 funciona: tras una revocación urgente, Hermes deja de estar listo en la primera llamada rechazada y
deja de llamar a Hécate; pasan sus pruebas y la suite de Hermes en macOS, con T022 y T024 adelantados (véase
«Phase Dependencies»).

---

## Phase 4: User Story 2 - El admin da a Hermes una identidad nueva sin vaciar su directorio de estado (Priority: P2)

**Goal**: con la identidad rechazada, el admin da de alta a Hermes con un token nuevo sin pararlo ni vaciar su
directorio de estado, y Hermes vuelve a estar listo con su base y su configuración de siempre.

**Independent Test**: tras la revocación de US1, crear un token y dar de alta a Hermes: el alta escribe la serie nueva,
la salud vuelve a `ready` y un envío llega (T021, los pasos 7 y 8 del apartado 3 de quickstart.md).

### Pruebas de la historia

- [X] T018 [US2] Pruebas del alta tras el rechazo en `hermes/tests/unit/application/test_identity_state.py` (FR-006,
  historia 2, escenarios 1 y 5):
  - con la identidad rechazada y una renovación en curso, `enroll` con un token nuevo funciona sin `conflict` ni tocar
    el almacén a mano: la serie es otra, `problem()` es `None`, la renovación en curso se descarta, el almacén carga la
    identidad nueva, las publicaciones se refrescan con ella y las llamadas vuelven a llegar a `FakeHecate`
    (`authenticated_calls` sube y `resolve` funciona);
  - un alta que Hécate rechaza (un token de otra CA) responde `auth`, no marca nada y deja la identidad rechazada; otro
    token después funciona;
  - con una identidad válida, el alta sigue respondiendo `conflict`
- [X] T019 [US2] Prueba del rechazo atrasado, en el mismo fichero (FR-004, historia 2, escenario 4): una llamada con la
  identidad revocada, retenida con `hold_next`, recibe su `auth` después de que otra llamada marque la identidad y de
  que un alta la sustituya; la identidad nueva sigue valiendo (`problem()` es `None` y sus llamadas llegan) y no hay
  otra línea `identity.refused`
- [X] T020 [P] [US2] Prueba contra una Hécate real en `hermes/tests/integration/hecate/test_client.py`: tras la
  revocación urgente y el refresco rechazado, un token nuevo da de alta al mismo Hermes sobre los mismos directorios,
  sin vaciarlos; la serie es otra, `problem()` es `None`, el almacén carga la identidad nueva y la resolución por el
  servicio vuelve a funcionar
- [X] T021 [P] [US2] Ampliar el escenario de T013 en `hermes/tests/e2e/test_flow.py` con la historia 2 (SC-003):
  - el alta escribe `enrolled spiffe://cerbero.test/hermes serial <serie nueva> until <fecha>`, con una serie
    distinta, sin parar Hermes ni tocar su directorio de estado;
  - la salud vuelve a `ready` en 120 s como mucho, sin que los contenedores de Hermes y de Envoy se reinicien;
  - Envoy presenta el certificado de la serie nueva en una conexión TLS nueva;
  - un envío de `caller` llega a la bandeja de salida, con la configuración y el canal verificado de antes
- [X] T022 [P] [US2] Prueba de `hermes serve` en `hermes/tests/unit/test_serve_enrollment.py` (FR-006, research R7),
  con un `Hermes` como el de `test_serve_pollers.py`, sin secretos inyectados, sobre el servicio de identidad y la
  caché de una `FakeHecate`, con `runtime.engine` puesto, como tras el paso de la conexión, pero sin conectar a
  ninguna base (sin él, `_refresh_password` no hace nada), y con `_password_after_enrollments` lanzada como tarea:
  - con la contraseña de la base quitada de la caché mientras Hécate rechazaba la identidad, tras un alta correcta la
    caché vuelve a tenerla (`current()` la da), pedida con la identidad nueva y sin esperar al refresco de cada 240 s;
  - si Hécate no responde a esa petición, Hermes deja `secrets.refresh.failed` con el tipo del error y sigue esperando
    las altas siguientes

### Implementación de la historia

- [X] T023 [US2] Completar `_as_hermes` en `hermes/src/hermes/application/identity.py`: un rechazo solo marca, y solo
  deja `identity.refused`, si la identidad con la que se llamó sigue siendo la vigente (`self._identity is identity`,
  research R3)
- [X] T024 [US2] Implementar en `hermes/src/hermes/infrastructure/main.py` la contraseña tras cada alta (research R7):
  - `_password_after_enrollments`, que espera cada alta con `wait_for_enrollment` y llama a `_refresh_password`; un
    fallo deja `secrets.refresh.failed` con el tipo del error, como `every()`, y no la para;
  - una tarea de fondo más en `background()`, que la lanza;
  - el docstring del módulo, que nombra las tareas de fondo: también la contraseña tras cada alta

**Checkpoint**: US1 y US2 funcionan: Hermes nota el rechazo y vuelve a estar listo con un alta, sin reiniciarse y con
la contraseña de su base; pasan sus pruebas y la suite de Hermes en macOS.

---

## Phase 5: User Story 3 - La operación de Hermes describe lo que hace de verdad (Priority: P3)

**Goal**: los documentos describen lo que hace Hermes: el procedimiento sin vaciar el directorio de estado, el motivo
de la salud, «Ante fallos», la seguridad y lo que recorre el E2E; DECISIONS ya no lo tiene abierto.

**Independent Test**: las comprobaciones del apartado 4 de quickstart.md y el recorrido del apartado 3 siguiendo solo
la operación de Hermes.

### Pruebas de la historia

- [X] T025 [US3] Ejecutar las comprobaciones del apartado 4 de [quickstart.md](quickstart.md#4-los-documentos) sobre los
  documentos de partida y comprobar que fallan: la primera encuentra seis líneas (`docs/hermes.md`, `cli.md`,
  `seguridad.md` y tres en `operacion.md`), la segunda la fila de DECISIONS, y la tercera nada

### Implementación de la historia

- [X] T026 [P] [US3] Reescribir en `docs/hermes/operacion.md` el procedimiento «La identidad de Hermes deja de valer»,
  con la forma de los demás procedimientos del documento (research R10):
  - cuándo: sin identidad, con el certificado caducado, de otra CA o rechazado por Hécate, como tras una revocación
    urgente, que Hermes nota en la primera llamada rechazada y como mucho a los 240 s;
  - qué pasa mientras: no está listo, su API responde `unavailable`, deja de llamar a Hécate con esa identidad y su log
    dice `identity.refused`, con la serie y la llamada;
  - los pasos: `hecate tokens create hermes` y `hermes enroll`, sin parar Hermes ni vaciar su directorio de estado, y
    la comprobación con `hermes health`;
  - la identidad deshabilitada, que el admin habilita antes de crear el token, y el desfase de relojes, que se arregla
    con el reloj y un reinicio;
  - en la lista de las tareas de fondo de `hermes serve`, al principio del documento, que la contraseña de la base
    también se pide tras cada alta;
  - y retirar del compose de pruebas la fila «Vaciar el directorio de estado de Hermes», que solo usaba ese paso
- [X] T027 [P] [US3] Actualizar en `docs/hermes/cli.md` el motivo `needs a new enrollment token` de `hermes health`:
  Hécate rechazó una de sus llamadas, como tras una revocación urgente, y se arregla con un token de alta nuevo y
  `hermes enroll`, sin pararlo
- [X] T028 [P] [US3] Actualizar en `docs/hermes.md`, «Ante fallos», la fila de la identidad: Hermes nota la revocación
  urgente en la primera llamada rechazada, como mucho a los 240 s, y se recupera con un alta nueva sin pararlo
- [X] T029 [P] [US3] Actualizar `docs/hermes/seguridad.md`: en «La identidad de Hermes», que una identidad que Hécate
  rechaza deja de valer y Hermes deja de llamar con ella; en «Lo que no cubre», que la revocación urgente se nota como
  mucho a los 240 s y que hasta entonces sigue atendiendo con ese certificado
- [X] T030 [P] [US3] Añadir a lo que recorre el E2E en `hermes/README.md` la revocación urgente del certificado de
  Hermes y su alta nueva
- [X] T031 [P] [US3] Retirar de las abiertas de `DECISIONS.md` la fila «Revocación urgente de Hermes», que pasa a estar
  construida
- [X] T032 [US3] Pasar las comprobaciones del apartado 4 de quickstart.md y recorrer su apartado 3 en el compose de
  pruebas con el fake, siguiendo solo `docs/hermes/operacion.md` (SC-005):
  - con un guion fuera del repositorio que reutiliza los ayudantes de `hermes/tests/support/` (el cliente de
    referencia y `ComposeHecate`);
  - los nueve pasos, el reinicio incluido, dan lo que dicen; las copias del directorio de datos de Hécate del paso 5
    se borran al terminar;
  - corregir los pasos del documento que no funcionen y anotar el resultado para la guía de revisión

**Checkpoint**: las tres historias funcionan y los documentos describen lo construido.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T033 Pasar la calidad y las suites del apartado 1 de [quickstart.md](quickstart.md#1-pruebas-y-calidad), con lo
  esperado del apartado 2:
  - ruff, el formato, `mypy --strict` y pip-audit, sin avisos;
  - la suite entera de Hermes en macOS, con `HECATE_BIN` y Docker, el E2E incluido;
  - en el contenedor `python:3.15.2-slim-trixie`, las pruebas sin Docker, con el binario de Hécate para Linux de T003;
  - anotar los recuentos para la guía de revisión
- [X] T034 Escribir la guía de revisión `specs/011-hermes-own-revocation/summary_review.md`: qué cambia y por qué, las
  decisiones de research (R0 a R11), las pruebas nuevas con sus recuentos en macOS y Linux frente a la línea base de
  T004, el recorrido de T032, lo que solo se recorre a mano (el reinicio completo de FR-007, en el paso 9), los
  documentos que cambian y qué mirar al revisar
- [X] T035 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `fix:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, terminada con la ruta de la guía de revisión;
  - deben recoger `hermes/`, `docs/`, `DECISIONS.md` y `specs/011-hermes-own-revocation/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias; T003 y T004 necesitan Colima (T002).
- **Foundational (Phase 2)**: depende del Setup y bloquea las pruebas de las tres historias.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2 (Phase 4)**: depende de US1: el alta solo se acepta con la identidad marcada, T019 retiene una llamada que pasa
  por el `_as_hermes` de T015, T021 amplía el escenario de T013, y T022 necesita la caché sobre el servicio de T016 y
  T017.
- **T022 y T024, adelantados al checkpoint de US1** (decisión al implementar, 2026-11-19): T013 da de alta a Hermes al
  terminar, y el escenario siguiente del E2E, PostgreSQL parado, abre una conexión nueva a la base; sin la contraseña
  que T024 vuelve a pedir tras el alta, el envío responde 500 en vez de 503. Conservan sus números y su historia.
- **US3 (Phase 5)**: depende de US1 y US2, porque documenta lo que construyen; los documentos se consolidan al cerrar.
- **Polish (Phase 6)**: al final; T035 depende de todo lo demás y se hace tras converge.

### Within Each Phase

- Las pruebas primero, y deben fallar; después, el port, la aplicación y la infraestructura.
- Las tareas que tocan el mismo fichero van en orden: `test_identity_state.py` (T005, T007 a T010, T018 y T019),
  `test_client.py` (T012 y T020), `test_flow.py` (T013 y T021), `identity.py` (T015 y T023) y `main.py` (T017 y
  T024).
- T015 y T016 necesitan el port de T014; T017, el `resolve` de T015 y los tipos de T016.
- Cada fase termina en su checkpoint con la suite de Hermes en verde.

### Parallel Opportunities

- Setup: T003 y T004.
- US1: las pruebas T011, T012 y T013, junto a T007 a T010, que comparten fichero; la implementación T015 y T016.
- US2: las pruebas T020, T021 y T022, junto a T018 y T019.
- US3: los documentos T026 a T031.

### Parallel Example: User Story 1

```bash
Task: "Pruebas de la caché sobre el servicio en hermes/tests/unit/application/test_secret_cache.py"
Task: "Pruebas contra una Hécate real en hermes/tests/integration/hecate/test_client.py"
Task: "Escenario nuevo de la revocación de Hermes en hermes/tests/e2e/test_flow.py"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: el entorno, la línea base y el doble de Hécate.
2. US1: Hermes nota el rechazo y deja de llamar a Hécate.
3. **Revisión del propietario:** las pruebas de US1, con el E2E de T013, que recorre los pasos 1 a 6 del apartado 3
   de quickstart.md.

### Siguientes incrementos

1. US2: el alta sin vaciar el directorio de estado, con su revisión (el E2E ampliado de T021, los pasos 7 y 8); la
   contraseña de la base tras el alta (T022 y T024) ya va con US1.
2. US3: los documentos, con el recorrido del apartado 3 entero siguiendo solo la operación.
3. Polish: la calidad y las suites en macOS y Linux, la guía de revisión y el mensaje de commit.

El arreglo se entrega entero en una PR con un commit de squash `fix:`; los incrementos son puntos de revisión.

---

## Phase 7: Convergence

- [X] T036 Añadir al procedimiento «La identidad de Hermes deja de valer» de `docs/hermes/operacion.md` que, tras una
  revocación urgente, `hermes enroll` responde `conflict` («hermes already has a valid identity») hasta que Hermes nota
  el rechazo, como mucho a los 240 s, así que el admin espera a que `hermes health` diga
  `not ready: needs a new enrollment token` antes del alta; y pasar otra vez las comprobaciones del apartado 4 de
  [quickstart.md](quickstart.md#4-los-documentos) per SC-005 (partial)
- [X] T037 Añadir a la tabla «Las specs» de `hermes/README.md` la fila de la 011, con el enlace
  `../specs/011-hermes-own-revocation/`, como las de la 008 a la 010, y anotar la fase en
  `specs/011-hermes-own-revocation/summary_review.md` per plan: R10 (partial)
