---

description: "Tareas del cambio 010: Hermes documentado en un documento de alto nivel y documentos específicos"
---

# Tasks: Hermes documentado en un documento de alto nivel y documentos específicos

**Input**: Design documents from `/specs/010-hermes-reference/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[quickstart.md](quickstart.md)

**Tests**: las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas (principio IV, research R11). Las que
miran los documentos nuevos fallan sobre `main`, y cada historia termina pasando las suyas. El recorrido de los
procedimientos (apartado 9) se ejecuta con el compose de pruebas: el happy path, con el bot de Telegram de pruebas del
propietario, y el resto, con el proveedor fake.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US6 de la spec), en el orden de sus prioridades.
US4 se apoya en la referencia de la CLI de US3; el resto de historias son independientes entre sí.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US6)
- Rutas relativas a la raíz del repositorio

## Convenciones para todas las tareas

- **Solo documentación (FR-001):** no cambia ningún fichero de código, configuración, contrato ni manifiesto. Los
  scripts de comprobación y del recorrido viven fuera del repositorio.
- **Estilo:** español, con los términos técnicos consolidados en inglés, y el de `docs/hecate/` (tablas, viñetas, líneas
  de hasta 120 caracteres). Los diagramas, en Mermaid (research R14).
- **Marco:** cada documento específico sigue el de [data-model.md](data-model.md#el-marco-de-un-documento-específico):
  un párrafo de qué cubre y para quién, el desarrollo y «Relacionado»; cada procedimiento, el de
  [data-model.md](data-model.md#el-marco-de-un-procedimiento).
- **Estado (FR-013):** los documentos de diseño no dicen nada del estado de lo que describen; las referencias de uso
  (integración, CLI y operación) describen solo lo que existe.
- **Sin palabras de tiempo:** lo que no existe se dice como un hecho y remite a DECISIONS, sin «hoy», «todavía» ni «por
  ahora» (apartado 11 de quickstart.md).
- **Un solo sitio (FR-003):** cada tema se desarrolla en un documento y los demás lo enlazan.
- **Datos (FR-014):** cada dato se contrasta con el código, los OpenAPI y la CLI real. Un dato del documento actual que
  no coincide pasa a describir el código y se anota para la guía de revisión (research R12). Si el código parece
  contradecir una spec fusionada, se avisa al propietario y no se corrige aquí.
- **Secretos:** ningún documento lleva un secreto, un token ni la dirección de una persona; los ejemplos usan valores de
  muestra. El token del bot de pruebas solo se lee de `hermes/deploy/.secrets/telegram-test.env` dentro de los scripts,
  se pasa por la entrada estándar y nunca se escribe (research R0).
- **Lo que no se toca:** las specs fusionadas, los ADR, la constitución y la plantilla de los docs de otros componentes
  (FR-018, FR-019, research R10).
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Comprobar que la rama activa es `docs/hermes-reference` (`git branch --show-current`) y que `.specify/feature.json` apunta a `specs/010-hermes-reference`
- [X] T002 Ejecutar la línea base de `specs/010-hermes-reference/quickstart.md` (apartados 1 a 6, 8 y 11) y comprobar que coincide con lo esperado sobre el estado de partida: faltan las 6 órdenes, las 11 variables y las 3 operaciones; los enlaces de los documentos actuales están bien; no hay cambios fuera de Markdown; DECISIONS enlaza a tres apartados de `docs/hermes.md`; el fichero del bot de pruebas aún no existe; `docs/hermes.md` tiene unas 6.400 palabras y ninguna palabra de tiempo
- [X] T003 Preparar fuera del repositorio el recorrido del apartado 9 de `specs/010-hermes-reference/quickstart.md`: un guion sobre el compose de `hermes/deploy/` (con `compose.test.yaml` y el fake para los procedimientos con fakes, y una configuración con una instancia de Telegram para el happy path), un driver para las órdenes interactivas (`hecate init`, `hecate ca create`, `hermes enroll` por la entrada estándar), el cliente de referencia de `hermes/tests/support/caller.py` y la carga del token del bot de pruebas desde `hermes/deploy/.secrets/telegram-test.env` sin escribirlo nunca. El guion del apartado 4 de la 009 (`walk_s4.py`) sirve de base. Pedir al propietario que cree el fichero del bot de pruebas antes del recorrido (T019)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: que todos los documentos existan desde el principio, para que cada historia pueda enlazar a los demás.

- [X] T004 Crear `docs/hermes/` con los nueve documentos de research R1 (`contratos.md`, `llamadas.md`, `comunicacion.md`, `configuracion.md`, `proveedores.md`, `seguridad.md`, `integracion.md`, `cli.md` y `operacion.md`), cada uno solo con su marco de data-model.md: título, párrafo de qué cubre y para quién, y «Relacionado»; el de `cli.md`, «CLI y variables de entorno de Hermes»

**Checkpoint**: los nueve ficheros existen y los enlaces de sus «Relacionado» llevan a sitios que existen.

---

## Phase 3: User Story 1 - Entender Hermes y llegar a cada concepto (Priority: P1) 🎯 MVP

**Goal**: `docs/hermes.md` es el documento de alto nivel; los documentos de concepto están completos, nada del documento
actual se ha perdido y el estado vive solo en DECISIONS y en el README.

**Independent Test**: leer solo el documento de alto nivel y responder por qué existe Hermes, qué resuelve, qué hace y
qué no, quién lo usa y cómo se comporta ante fallos; seguir cada enlace hasta su documento; y pasar el apartado 6 de
quickstart.md y, del 7, los apartados del documento actual cuyo destino escribe esta historia.

### Tests for User Story 1

- [X] T005 [US1] Preparar la comprobación «nada se pierde» del apartado 7 de `specs/010-hermes-reference/quickstart.md`: una lista, fuera del repositorio, con cada apartado y cada dato del `docs/hermes.md` de `main` (`git show main:docs/hermes.md`) y su destino según research R3; sobre `main` todos están pendientes

### Implementation for User Story 1

- [X] T006 [P] [US1] Escribir `docs/hermes/contratos.md`: la taxonomía con su árbol y su tabla de ejemplos, las tres capas de datos, el sobre común con sus códigos de error, sus estados HTTP y qué es un error, el contrato de una función (`kind`, `mode`, `side_effect`, `compensation`, `discloses_data` y `x-cerbero-function`), `chat.send_message` con su petición, su respuesta y el orden de sus comprobaciones, y el formato y el versionado de la API (research R1 y R3). Contrastar los códigos, los estados y el orden con `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`, `common.v1.json`, `hermes/src/hermes/domain/services/` y `hermes/src/hermes/application/send_message.py`
- [X] T007 [P] [US1] Escribir `docs/hermes/llamadas.md`: la llamada síncrona con su diagrama, la task (sus estados con su diagrama, su cola y su proyección, sus rutas, la cancelación y la retención) como diseño, la idempotencia de la `Idempotency-Key` (la reserva, la respuesta guardada, el corte y la limpieza de las 24 horas), los reintentos seguros, el resultado desconocido y los timeouts (research R1 y R3). Contrastar la vida de una clave, las esperas de los reintentos y el timeout de `chat.send_message` con `hermes/src/hermes/application/idempotency.py`, `hermes/src/hermes/domain/idempotency/` y `hermes/src/hermes/infrastructure/providers/retry.py`
- [X] T008 [P] [US1] Escribir `docs/hermes/comunicacion.md`: los contactos (personas y, como diseño, grupos) y sus canales, las plantillas con sus versiones, sus esquemas y sus variables, el mensaje neutral y cómo lo dibuja cada proveedor (el orden del recorte y `truncated`), la verificación de un canal con su código, y `chat.ask` con el ejemplo de preguntar al propietario por Telegram como diseño (research R1 y R3). Contrastar el código de verificación (alfabeto, longitud, vida y hash), el orden del recorte y la respuesta «Canal verificado.» con `hermes/src/hermes/domain/messages/`, `hermes/src/hermes/domain/verification/`, `hermes/src/hermes/application/verification.py` y `hermes/src/hermes/domain/services/chat/drawing.py`
- [X] T009 [P] [US1] Escribir `docs/hermes/configuracion.md`: la configuración en la base de Hermes, su fichero (formato, lectura estricta y validación completa antes de escribir nada), el plan con su orden y su confirmación, la aplicación en una sola transacción, la exportación, y las instancias (parámetros, referencias `secret://`, límite de uso y prueba de conexión) (research R1 y R3). Contrastar las reglas de lectura (tamaño, nombres y campos), el orden del plan y el rango del límite de uso con `hermes/src/hermes/infrastructure/config_file/`, `hermes/src/hermes/domain/configuration/` y `hermes/src/hermes/application/configuration.py`
- [X] T010 [P] [US1] Escribir `docs/hermes/seguridad.md` según research R7 y FR-010: frente a qué protege, la tabla amenaza, mecanismo y documento, lo que se exige al proxy con la configuración de `hermes/deploy/envoy/` como referencia, la autenticación y el acceso, los secretos, la auditoría, los datos personales y los logs, qué datos guarda Hermes y dónde, y los riesgos aceptados con enlaces a DECISIONS, sin repetir el desarrollo de cada mecanismo. Contrastar lo que se exige al proxy y el orden de las comprobaciones con `hermes/src/hermes/infrastructure/api/xfcc.py`, `hermes/src/hermes/application/authentication.py` y `hermes/deploy/envoy/envoy.yaml`
- [X] T011 [US1] Reescribir `docs/hermes.md` como documento de alto nivel según research R2 y FR-002: por qué existe, qué hace y qué no, quién lo usa con su diagrama, cómo encaja todo con la vida de una llamada y el arranque, un párrafo y un enlace por concepto (contratos, llamadas, comunicación, configuración, proveedores y seguridad) y por uso (integración, CLI y variables de entorno, operación y `hermes/README.md`), ante fallos con los avisos, decisiones y riesgos, preguntas frecuentes y referencias. Sin la plantilla de 14 apartados ni el detalle que ya está en los documentos específicos, y sin pasar de 2.500 palabras (SC-001, apartado 11 de quickstart.md)
- [X] T012 [US1] Llevar los tres enlaces de «Abiertas» de `DECISIONS.md` que apuntan a apartados de `docs/hermes.md` (`chat.ask`, las tasks y los grupos de contactos) a sus apartados de `docs/hermes/comunicacion.md` y `docs/hermes/llamadas.md`, sin cambiar su texto (research R9, data-model.md); comprobar que no falta ninguna pieza de Hermes sin construir entre las abiertas y que la línea de Hermes de «Estado del Proyecto» de `README.md` dice lo que ya hace (FR-013)
- [X] T013 [US1] Pasar la comprobación de T005 (apartado 7 de `specs/010-hermes-reference/quickstart.md`, FR-004) para los destinos que escribe esta historia (el documento de alto nivel, contratos, llamadas, comunicación, configuración y seguridad) y el apartado 6: cada apartado y cada dato del documento de `main` con uno de esos destinos está en él, cada dato que cambia por no coincidir con el código queda anotado para la guía de revisión, y DECISIONS ya no enlaza a apartados de `docs/hermes.md`. Los apartados con destino en integración, la CLI, operación, proveedores o `hermes/README.md` quedan pendientes hasta T025

**Checkpoint**: el documento de alto nivel y los cinco documentos de concepto de esta historia están completos, los
enlaces de DECISIONS llevan a ellos, el apartado 6 de quickstart.md pasa y, del 7, los destinos de esta historia.

---

## Phase 4: User Story 2 - Integrar un componente que llama a Hermes (Priority: P2)

**Goal**: quien desarrolla un componente que llama a Hermes implementa su lado con la guía y el OpenAPI, sin abrir las
specs ni el código.

**Independent Test**: con la guía y el OpenAPI, escribir la secuencia completa desde que el admin concede el acceso
hasta enviar, repetir sin duplicar y actuar ante cada error; y pasar el apartado 3 de quickstart.md.

### Implementation for User Story 2

- [X] T014 [US2] Escribir `docs/hermes/integracion.md` según research R4 y FR-006, sin código. Debe cubrir qué necesita un componente antes de la primera llamada (con el enlace a la guía de integración de Hécate para su identidad y el acceso que concede el admin), el flujo completo con su diagrama, cada llamada a través de Envoy con el formato del token corto y lo que comprueba Hermes, la `Idempotency-Key`, el sobre y cada código de error con su estado HTTP, si se reintenta y qué hacer, las capacidades y la prueba de conexión, los límites, la tabla de las tres operaciones con enlaces al OpenAPI y cómo probar un componente contra Hermes, con el cliente de referencia. Solo lo construido: las rutas de las tasks no aparecen (research R3). Contrastar cada dato con `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`, `hermes/src/hermes/infrastructure/api/`, `hermes/src/hermes/application/authentication.py` y `hermes/tests/support/caller.py`
- [X] T015 [US2] Pasar el apartado 3 de `specs/010-hermes-reference/quickstart.md` (cada operación del OpenAPI está en la guía y ninguna referencia de uso nombra otra ruta) y repasar, con la guía y el OpenAPI y sin abrir las specs ni el código, la secuencia completa desde que el admin concede el acceso hasta enviar, repetir sin duplicar y actuar ante cada error (SC-005)

**Checkpoint**: la guía de integración está completa y el apartado 3 de quickstart.md pasa.

---

## Phase 5: User Story 3 - Configurar y manejar Hermes desde la CLI (Priority: P3)

**Goal**: cómo se lanza Hermes, sus variables y cada orden de su CLI, en un solo documento, con el fichero de la
configuración en el suyo.

**Independent Test**: pasar los apartados 1 y 2 de quickstart.md.

### Implementation for User Story 3

- [X] T016 [US3] Escribir `docs/hermes/cli.md`, «CLI y variables de entorno de Hermes», según research R5 y FR-007. Debe cubrir cómo se lanza (`hermes serve`, sus sockets y sus códigos de salida), lo que necesita, las once variables con su valor por defecto, su efecto y sus límites, las convenciones (la salida, los errores, los códigos de salida 0, 1 y 2, y el token de alta sin eco o desde la entrada estándar) y las seis órdenes finales con sus argumentos, su salida, sus errores y su código de salida, enlazando a `docs/hermes/configuracion.md` para el fichero. Contrastar con `hermes/src/hermes/infrastructure/admin/cli.py`, la ayuda de la CLI, `hermes/src/hermes/infrastructure/settings.py`, `hermes/src/hermes/infrastructure/main.py` y `hermes/src/hermes/infrastructure/sockets.py`
- [X] T017 [US3] Pasar los apartados 1 y 2 de `specs/010-hermes-reference/quickstart.md` en los dos sentidos (cada orden y cada variable que existen están en la referencia, y ningún documento nombra otras) y comparar a mano los valores por defecto y los límites de las variables con `hermes/src/hermes/infrastructure/settings.py`

**Checkpoint**: la referencia de la CLI está completa y los apartados 1 y 2 de quickstart.md pasan.

---

## Phase 6: User Story 4 - Actuar ante cada situación de operación (Priority: P4)

**Goal**: un procedimiento escrito, y comprobado con el compose de pruebas, para la puesta en marcha y cada situación de
operación.

**Independent Test**: cada situación tiene su procedimiento, y el recorrido del apartado 9 de quickstart.md pasa.

### Implementation for User Story 4

- [X] T018 [US4] Escribir `docs/hermes/operacion.md` según research R6 y FR-011: los diez procedimientos con «Cuándo», «Qué necesitas», «Pasos» (con las órdenes de `docs/hermes/cli.md` y de `docs/hecate/cli.md`) y «Cómo comprobarlo», y el apartado del compose de pruebas: cómo se ejecuta en él cada orden, cómo se preparan sus secretos (`prepare.sh` y `.secrets/`) y cómo se usa un bot de Telegram de pruebas, con su token en `hermes/deploy/.secrets/telegram-test.env` y en la Hécate de pruebas desde la entrada estándar. «Rotar el token de un bot» dice cómo se comprueba con lo que existe (research R12), y «Restaurar una copia», lo que se puede hacer y la remisión a DECISIONS. Ni gestor de servicios del home lab ni nada que no exista (research R3)
- [X] T019 [US4] Ejecutar el recorrido del apartado 9 de `specs/010-hermes-reference/quickstart.md` siguiendo `docs/hermes/operacion.md` tal como está escrito: el happy path con el bot de pruebas del propietario (él envía el código de verificación), los procedimientos con fakes, y para rotar el token y la instancia que falla, las pruebas con mocks: `hermes/tests/integration/telegram/`, `hermes/tests/integration/api/test_instance_test.py` y `hermes/tests/unit/application/test_secret_cache.py`; corregir los pasos que no funcionen y anotar el resultado para la guía de revisión

**Checkpoint**: los procedimientos están escritos y el recorrido pasa.

---

## Phase 7: User Story 5 - Añadir una librería de proveedor (Priority: P5)

**Goal**: quien desarrolla una librería sabe qué tiene que contener y qué tiene que pasar.

**Independent Test**: con `docs/hermes/proveedores.md`, enumerar lo que tiene que contener una librería nueva de `chat`
y lo que tiene que pasar, y contrastarlo con la de Telegram, la del fake y la batería de conformidad.

### Implementation for User Story 5

- [X] T020 [P] [US5] Escribir `docs/hermes/proveedores.md` según research R1 y FR-009: qué es una librería y qué tipos de servicio implementa, las oficiales y las propias, sus partes con su árbol, cada campo del manifiesto, su versión separada del contrato, el polling, la batería de conformidad de su tipo de servicio con sus casos, el proveedor fake (cuándo se carga y sus bandejas) y la librería de Telegram (sus llamadas, la traducción de sus errores, su límite y su dibujo en HTML), con la referencia de la Bot API
- [X] T021 [US5] Contrastar `docs/hermes/proveedores.md` con `hermes/src/hermes/infrastructure/providers/` (los manifiestos, el registro, Telegram y el fake), `hermes/src/hermes/domain/services/manifest.py` y `hermes/tests/contract/test_conformance_chat.py`: cada campo del manifiesto y cada caso de la batería están, y no nombra ninguno que no exista

**Checkpoint**: el documento de proveedores está completo y coincide con las librerías y la batería.

---

## Phase 8: User Story 6 - Retomar el desarrollo de Hermes (Priority: P6)

**Goal**: `hermes/README.md` explica cómo se prueba y se organiza Hermes.

**Independent Test**: siguiendo solo el README, pasar la suite con la Hécate real y Docker, el E2E y las comprobaciones
de calidad (apartado 10 de quickstart.md).

### Implementation for User Story 6

- [X] T022 [P] [US6] Escribir `hermes/README.md` según research R8 y FR-012: qué es, con el enlace a `docs/hermes.md`; las herramientas; la suite con sus marcas (`docker` y `hecate`) y lo que se salta sin ellas, y el E2E; las comprobaciones de calidad; para qué sirve cada ayudante de `hermes/tests/support/`; la organización real del paquete y sus patrones; y dónde está la spec de cada feature
- [X] T023 [US6] Seguir solo `hermes/README.md` (apartado 10 de `specs/010-hermes-reference/quickstart.md`): pasar la suite con `HECATE_BIN` y Docker, con el E2E, y las comprobaciones de calidad. El resultado es el de la feature 009: 866 pruebas

**Checkpoint**: el README de desarrollo está completo y lo que explica funciona.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [X] T024 Revisar los enlaces de `ARCHITECTURE.md`, `GLOSSARY.md` y los docs de otros componentes que remiten a un detalle de Hermes y llevarlos a su documento específico (research R13, FR-016), sin tocar nada más de ellos (FR-018)
- [X] T025 Pasar los apartados 4 a 8 y 11 de `specs/010-hermes-reference/quickstart.md` sobre todos los documentos del cambio: enlaces y anclas (FR-017), ningún cambio fuera de Markdown ni en las specs fusionadas (FR-001, FR-019), los enlaces de DECISIONS, la comprobación completa de «nada se pierde» de T005, ya con todos sus destinos escritos (FR-004, SC-002), ningún documento con el token del bot de pruebas, y el documento de alto nivel sin pasar de 2.500 palabras y sin palabras de tiempo en ningún documento (SC-001, FR-015)
- [X] T026 Leer de una vez `docs/hermes.md`, `docs/hermes/`, `hermes/README.md` y las líneas cambiadas de `DECISIONS.md`, buscando contradicciones entre ellos y con ARCHITECTURE, GLOSSARY, DECISIONS y `docs/hecate/` (FR-016), frases de estado en los documentos de diseño o de futuro en cualquiera (FR-013, FR-015) y temas desarrollados en dos sitios (FR-003)
- [X] T027 Proponer al propietario el mensaje del commit de squash y la descripción de la PR del cambio (`specs/010-hermes-reference/`):
  - **commit:** `docs:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, con las diferencias con el código que se encontraron (research R12) y el resultado del recorrido, terminada con la ruta de la guía de revisión;
  - deben recoger `docs/hermes.md`, `docs/hermes/`, `hermes/README.md`, `DECISIONS.md`, `ARCHITECTURE.md` y `GLOSSARY.md` si cambian y `specs/010-hermes-reference/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias.
- **Foundational (Phase 2)**: depende del Setup; crea los ficheros que enlazan las historias.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2, US3, US5 y US6 (Phases 4, 5, 7 y 8)**: dependen de la fase fundacional y no entre sí; conviene hacerlas tras
  US1, para que los documentos de concepto ya existan cuando las referencias de uso los enlacen.
- **US3 (Phase 5)**: su escenario 3, el fichero de la configuración, lo cubre `docs/hermes/configuracion.md`, que
  escribe T009 en US1.
- **US4 (Phase 6)**: depende de US3, porque cada paso usa una orden de `docs/hermes/cli.md`; su recorrido (T019)
  necesita el fichero del bot de pruebas del propietario.
- **Polish (Phase 9)**: al final.

### Within Each User Story

- La comprobación se prepara o se ejecuta primero sobre el estado de partida; después se escribe; al terminar, pasa.
- En US1, los cinco documentos de concepto (T006 a T010) van antes del de alto nivel (T011), que resume lo que ellos
  desarrollan, y los enlaces de DECISIONS (T012) se mueven en cuanto `docs/hermes.md` pierde sus apartados.
- Las tareas que tocan el mismo fichero van en orden.

### Parallel Opportunities

- US1: los cinco documentos de concepto, T006 a T010.
- Entre historias: T020 (proveedores) y T022 (README de desarrollo) con las tareas de US2 y US3.

---

## Parallel Example: User Story 1

```bash
Task: "Escribir docs/hermes/contratos.md (T006)"
Task: "Escribir docs/hermes/llamadas.md (T007)"
Task: "Escribir docs/hermes/comunicacion.md (T008)"
Task: "Escribir docs/hermes/configuracion.md (T009)"
Task: "Escribir docs/hermes/seguridad.md (T010)"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: la línea base, el guion del recorrido y los nueve ficheros con su marco.
2. US1: los documentos de concepto, el de alto nivel y los enlaces de DECISIONS.
3. **Revisión del propietario:** el documento de alto nivel y los de concepto.

### Siguientes incrementos

1. US2: la guía de integración, la que más necesitará Hades.
2. US3 y US4: la referencia de la CLI y los procedimientos, con el recorrido y el bot de pruebas.
3. US5 y US6: el documento de proveedores y el README de desarrollo.
4. Polish: enlaces, comprobaciones finales, lectura completa y la propuesta de commit y PR.

El cambio se entrega entero en una PR con un commit de squash `docs:`; los incrementos son puntos de revisión.

---

## Phase 10: Convergence

- [X] T028 Añadir a `docs/hermes/cli.md` («Lo que necesita») y a la tabla «Qué datos guarda y dónde» de `docs/hermes/seguridad.md` los permisos del directorio del proxy, que Hermes crea como el de estado: 0700, con `gen-<n>/` 0700 y ficheros 0600 (`hermes/src/hermes/infrastructure/hecate/identity_store.py`) per FR-007 (partial)
- [X] T029 Sustituir en `docs/hermes/operacion.md` (paso 1 de «Una instancia que falla») la repetición de qué dice cada estado de una instancia por el enlace a la tabla de `docs/hermes/integracion.md` per FR-003 (partial)
- [X] T030 Enlazar la pregunta frecuente «¿Por qué la salud no está en la API?» de `docs/hermes.md` a la salud de `docs/hermes/cli.md`, como las demás preguntas que tocan un concepto per plan: R2 (partial)
- [X] T031 Pasar de nuevo los apartados 1 a 8 y 11 de `specs/010-hermes-reference/quickstart.md` y la comprobación de «nada se pierde» tras T028 a T030, y añadir el resultado de esta convergencia a `specs/010-hermes-reference/summary_review.md` per plan: R11 (partial)

---

## Phase 11: Convergence

- [X] T032 Corregir en `docs/hermes/configuracion.md` («La lectura») la regla de los nombres: siguen `^[a-z][a-z0-9_]{0,62}$`, y no se repiten las instancias, las plantillas, los contactos, las variables ni las versiones de una plantilla, ni dos canales de un contacto en la misma instancia; los proveedores sí se repiten, porque puede haber varias instancias por proveedor (`hermes/src/hermes/infrastructure/config_file/models.py`, `_repeated`) per FR-014 (contradicts)
- [X] T033 Cambiar en la introducción de `docs/hermes/integracion.md` «el cliente de referencia de las pruebas hace todo lo de esta guía» por lo que hace (`hermes/tests/support/caller.py`): el alta, el token corto de cada llamada y el envío con su clave per FR-014 (partial)
- [X] T034 Pasar de nuevo los apartados 1 a 8 y 11 de `specs/010-hermes-reference/quickstart.md` y la comprobación de «nada se pierde» tras T032 y T033, y añadir el resultado de este converge a `specs/010-hermes-reference/summary_review.md` per plan: R11 (partial)

---

## Phase 12: Convergence

- [X] T035 Añadir a «Lo que no cubre y los riesgos aceptados» de `docs/hermes/seguridad.md`, como un hecho y sin presentarlo como riesgo aceptado, que tras revocar con urgencia el certificado de Hermes, Hermes no lo nota hasta que Hécate rechaza su renovación, con el enlace a «La identidad de Hermes deja de valer» de `docs/hermes/operacion.md`, como ya dicen ese procedimiento y «Ante fallos» de `docs/hermes.md` per FR-010 (partial)
- [X] T036 Pasar de nuevo los apartados 1 a 8 y 11 de `specs/010-hermes-reference/quickstart.md` y la comprobación de «nada se pierde» tras T035, y añadir el resultado de este converge a `specs/010-hermes-reference/summary_review.md` per plan: R11 (partial)

---

## Phase 13: Convergence

- [X] T037 Añadir a «Abiertas» de `DECISIONS.md`, junto a las demás filas de Hermes y con su formato, la revocación urgente de Hermes: «Diseñado, no implementado» que Hermes deje de estar listo y pida un token de alta nuevo en cuanto Hécate revoca con urgencia su certificado (FR-010 de la 008); solo lo nota cuando Hécate rechaza su renovación, porque Hécate le rechaza el refresco de la lista de revocación, y mientras `hermes enroll` responde `conflict`; con el enlace a «La identidad de Hermes deja de valer» de `docs/hermes/operacion.md` per FR-013 / plan: R9 (missing)
- [X] T038 Añadir a «Abiertas» de `DECISIONS.md` la decisión del reintento de Hades tras un fallo reintentable de Hermes: Hades deriva la clave de idempotencia del paso y su input y reintenta si el sobre dice `retryable`, y Hermes guarda en la clave toda respuesta que llegó al proveedor, también un fallo reintentable; falta decidir si Hermes no guarda los fallos que garantizan que no se envió nada o si Hades cambia la clave en cada intento; con el enlace a «La `Idempotency-Key`» de `docs/hermes/integracion.md` per Constitution: Flujo de desarrollo y documentación (missing)
- [X] T039 Pasar de nuevo los apartados 1 a 8 y 11 de `specs/010-hermes-reference/quickstart.md` y la comprobación de «nada se pierde» tras T037 y T038, y añadir el resultado de este converge a `specs/010-hermes-reference/summary_review.md` per plan: R11 (partial)
