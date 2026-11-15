---

description: "Tareas de la feature 008: base de Hermes y envío de mensajes por Telegram"
---

# Tasks: Base de Hermes y envío de mensajes por Telegram

**Input**: Design documents from `/specs/008-hermes-send-message/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US6 de la spec), en el orden de sus prioridades.
Cada historia se prueba por separado con dobles de lo que aún no existe; el orden y lo que se puede adelantar están en
«Dependencies & Execution Order».

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US6)
- Rutas relativas a la raíz del repositorio; las órdenes de uv se ejecutan con `--package cerbero-hermes`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar.
- **Dónde van las pruebas:**
  - las de dominio y aplicación, en `hermes/tests/unit/domain/` y `hermes/tests/unit/application/`, con dobles de los
    puertos y el reloj inyectado (`FakeClock`), sin esperas reales;
  - las de integración (PostgreSQL, Telegram con respx, Hécate real, canal local y API con el transporte ASGI de httpx),
    en `hermes/tests/integration/`;
  - las de contrato y la batería de conformidad, en `hermes/tests/contract/`;
  - las de extremo a extremo con docker compose y Envoy, en `hermes/tests/e2e/`;
  - los ayudantes, en `hermes/tests/support/`.
- **Marcas de pytest:** `docker` (testcontainers o compose) y `hecate` (necesita `HECATE_BIN`). Sirven para elegir qué
  correr mientras se trabaja; T109 las corre todas, y una tarea no está terminada si alguna falla.
- **Contrato:** cada prueba que recibe una respuesta de la API la valida contra `hermes.v1.json` y `common.v1.json` con
  el ayudante de T005; las del canal local, contra el sobre común.
- **Seguridad:**
  - ningún secreto, token, código de verificación, dirección de contacto, certificado ni URL de Telegram en logs,
    respuestas, auditoría ni lo exportado (FR-030, SC-007);
  - los secretos resueltos van en el tipo `SecretValue`, cuyo `repr` los oculta;
  - mypy en modo estricto en todo `hermes/src`.
- **Hécate no cambia,** salvo la prueba de contrato de T003 (la versión del sobre común); su suite sigue pasando.
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (el contrato primero)

**Purpose**: el proyecto, sus herramientas, el contrato publicado y el entorno de pruebas, antes de escribir código
(principio II).

- [X] T001 Preparar el paquete en `hermes/pyproject.toml`:
  - dependencias con las versiones de plan.md (FastAPI 0.162.7, Pydantic 2.14.3, uvicorn 0.54.0, httpx 0.28.1,
    cryptography 50.0.2, PyJWT 2.15.1, SQLAlchemy 2.1.8 con `asyncio`, psycopg 3.3.9 con `binary` y `pool`, Alembic
    1.20.1, PyYAML 6.0.3 y jsonschema 4.26.0);
  - el grupo `dev` (pytest, hypothesis, anyio, respx, testcontainers con `postgres`, ruff, mypy, pip-audit y
    types-PyYAML), en su última versión estable;
  - la entrada `hermes = "hermes.infrastructure.admin.cli:main"`;
  - `[tool.ruff]`, `[tool.mypy]` con `strict = true` y `[tool.pytest.ini_options]` con las marcas `docker` y `hecate`;
  - los paquetes vacíos de la estructura de plan.md bajo `hermes/src/hermes/` y los directorios de `hermes/tests/`;
  - `uv lock` y `uv sync --package cerbero-hermes` terminan sin errores
- [X] T002 [P] Subir `shared/contracts/src/cerbero_contracts/openapi/common.v1.json` a la 1.1.0: `Meta` gana `service` y
  `provider` (nombres con `^[a-z][a-z0-9_]{0,62}$`) e `idempotency_key` (de 1 a 128 caracteres ASCII imprimibles), los
  tres opcionales, sin tocar nada más
- [X] T003 Actualizar `hecate/crates/infrastructure/tests/contract_schema.rs` para que espere la 1.1.0 de
  `common.v1.json` (y la 1.0.0 de `hecate.v1.json`) y compruebe que un `Meta` con los tres campos nuevos valida;
  después, `cargo test --manifest-path hecate/Cargo.toml --workspace` pasa entero
- [X] T004 [P] Crear `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json` (OpenAPI 3.1 en JSON, licencia
  `AGPL-3.0-or-later`) con lo que fija [contracts/api.md](contracts/api.md):
  - `sendMessage` (`POST /chat/v1/send_message`): cabeceras `Authorization`, `Idempotency-Key` y `X-Request-Id`, cuerpo
    `SendMessageRequest` sin campos adicionales, respuestas 200, 400, 401, 404, 409, 429, 500 y 503, y la extensión
    `x-cerbero-function`;
  - `getCapabilities` (`GET /v1/capabilities`) y `testInstance` (`POST /v1/instances/{name}/test`), sin rutas de salud,
    que van por el canal local;
  - los esquemas de `data` de cada respuesta, con la capa de Telegram (`provider_message_ref`), referenciando
    `Completed`, `Failed`, `Error` y `RequestId` de `common.v1.json`
- [X] T005 Pruebas del contrato en `hermes/tests/contract/test_openapi_schema.py`, con el ayudante
  `hermes/tests/support/contract.py` (valida una respuesta por `operationId` y estado HTTP con jsonschema y
  referencing):
  - los dos documentos son OpenAPI 3.1 en JSON con su licencia y su versión;
  - cada `$ref` resuelve, en el propio documento o en `common.v1.json`;
  - cada ejemplo de respuesta de contracts/api.md valida, y uno con campos de más o de menos no valida;
  - `hermes.v1.json` solo tiene las tres operaciones de contracts/api.md: ninguna gestiona la configuración ni la salud
    (FR-024, FR-015)
- [X] T006 [P] Ayudantes de pruebas en `hermes/tests/conftest.py` y `hermes/tests/support/`:
  - `clock.py`: `FakeClock`, que avanza a mano;
  - `postgres.py`: un PostgreSQL de testcontainers con la base `hermes` y el usuario `hermes`, y una segunda base de
    otra app a la que ese usuario no tiene acceso;
  - `pki.py`: una CA de pruebas (raíz e intermedia P-256) que emite certificados con un SPIFFE ID, CRL X.509 y claves
    Ed25519 de firma de llamadas con su JWK de RFC 8037, y firma tokens cortos con los claims de la guía de Hécate
- [X] T007 [P] Entorno de pruebas en `hermes/` (research R21):
  - `Dockerfile`: `python:3.15-slim` con uv, `uv sync --frozen --no-dev` y el usuario UID 10001;
  - `deploy/compose.yaml`: PostgreSQL, Hécate, Hermes y Envoy, cada imagen con su etiqueta exacta; Hermes sin puertos
    publicados y con el healthcheck `hermes health --live`; Envoy en `127.0.0.1:9443` y Hécate en `127.0.0.1:8443`, con
    `HECATE_API_ADDR=0.0.0.0:8443`, `HECATE_KDF_PROFILE=moderate` y `ulimits: memlock: -1`; volúmenes para los
    directorios de estado, del proxy, de la API y de administración, este último sin montar en Envoy;
  - `deploy/prepare.sh`: escribe una contraseña aleatoria en `deploy/.secrets/postgres_hermes_password` con permisos
    0600, y `deploy/.gitignore` con `.secrets/`;
  - `deploy/postgres/`: el script de inicio que crea el usuario `hermes` con esa contraseña y su base `hermes`, sin
    acceso a las demás;
  - `deploy/hecate/Dockerfile`: Hécate compilada en release desde el repositorio, con `rust:1.99.1-bookworm`

**Checkpoint**: el contrato está publicado y sus pruebas pasan; la suite de Hécate pasa con el sobre común 1.1.0.

---

## Phase 2: Foundational (bloquea todas las historias)

**Purpose**: lo que usan todas las historias: el dominio común, la configuración en memoria, el esqueleto de la API y
del canal local, la persistencia con sus tablas y los logs.

### Pruebas de la fase

- [X] T008 [P] Pruebas del dominio común en `hermes/tests/unit/domain/test_shared.py`:
  - los nombres siguen `^[a-z][a-z0-9_]{0,62}$`;
  - una referencia `secret://<identidad>/<ruta>` con `@N` opcional se lee con las reglas de Hécate, y un valor en claro
    se rechaza;
  - un SPIFFE ID se lee y da su trust domain y su identidad;
  - `ErrorCode` tiene los nueve códigos comunes, cada uno con su `retryable` por defecto;
  - `SecretValue` no muestra su valor en `repr`, `str` ni al formatearlo
- [X] T009 [P] Pruebas de la configuración por entorno en `hermes/tests/unit/test_settings.py`, con las variables de
  [contracts/cli.md](contracts/cli.md#variables-de-entorno): las obligatorias de cada orden, los valores por defecto,
  `HERMES_VERIFICATION_CODE_MINUTES` de 5 a 1440, `HERMES_FAKE_PROVIDERS` `disabled` o `enabled`,
  `HERMES_LOG_LEVEL`, y una `HERMES_DATABASE_URL` con contraseña rechazada; un valor que no vale da `error: <mensaje>`
- [X] T010 [P] Pruebas de los logs en `hermes/tests/unit/test_logging.py`: una línea JSON por evento con `ts`, `level`,
  `logger` y `event`; un `SecretValue`, un token, un código, una dirección, una URL de Telegram o un certificado nunca
  salen; httpx y httpcore quedan en `WARNING`
- [X] T011 [P] Pruebas del modelo de la configuración en `hermes/tests/unit/domain/test_configuration_model.py`, con las
  reglas de [data-model.md](data-model.md):
  - versión de plantilla «Número desde 1»; textos del mensaje: `title` «De 1 a 256 caracteres», `severity` «`info`,
    `low`, `medium`, `high` o `critical`», `summary` «Hasta 2048 caracteres», `facts` «Hasta 20 pares; cada texto, hasta
    256 caracteres», `actions_done` «Hasta 10; cada uno, hasta 256 caracteres», `expected` «Hasta 512 caracteres» y
    `link` «Una URL `https` de hasta 2048 caracteres», comprobados cuando el texto no lleva `${...}`;
  - variable «Valor de texto, hasta 4096 caracteres»; contacto «Al menos un canal»; canal «Uno por contacto, tipo de
    servicio, proveedor e instancia»;
  - la huella canónica de una configuración no depende del orden de sus listas
- [X] T012 Pruebas del esqueleto de la API en `hermes/tests/integration/api/test_envelope.py`, con el transporte ASGI de
  httpx:
  - `completed` y `failed` validan contra `common.v1.json`;
  - `X-Request-Id` de 1 a 128 caracteres ASCII imprimibles se repite en `meta.request_id`; sin ella se genera uno; con
    otro valor, `validation`;
  - un cuerpo de más de 64 KiB, o un JSON que no es estándar, responde `validation` (400);
  - una ruta o un método desconocido responde `not_found` (404) en el sobre;
  - un fallo inesperado responde `internal` (500) sin detalles
- [X] T013 Pruebas de la persistencia en `hermes/tests/integration/persistence/test_schema.py`, con el PostgreSQL de
  T006:
  - la migración `0001` crea las ocho tablas de [data-model.md](data-model.md#tablas) con sus restricciones:
    `config_templates` con PK `(template_id, version)` y CHECK `version >= 1`; `config_instances` con
    `requests_per_minute` de 1 a 600 o nulo; `channels` con UNIQUE `(contact, service, provider, instance)`, FK a
    `config_contacts` con ON DELETE CASCADE y los CHECK «con `verified_at`, hay `address`», «`code_hash` y
    `code_expires_at` van juntos» y «con código pendiente, no está verificado»; `idempotency_keys` con PK
    `(caller, key)` y `state` `in_progress` o `completed`; `used_call_tokens` con PK `(issuer, jti)`; `audit_log` con
    `kind` `send`, `config_applied`, `verification_code_issued` o `channel_verified`;
  - el disparador `audit_log_append_only` rechaza `UPDATE` y `DELETE`, y el usuario `hermes` no puede conectarse a la
    segunda base de T006 (FR-023);
  - aplicar las migraciones dos veces, o a la vez desde dos conexiones, no falla (bloqueo consultivo);
  - el repositorio de la configuración carga y sustituye una configuración entera, y conserva la dirección, la
    verificación y el código de los canales que no cambian
- [X] T014 Pruebas del esqueleto del canal local en `hermes/tests/integration/admin/test_admin_skeleton.py`: el
  directorio queda en 0700 y el socket en 0600; las respuestas usan el sobre común; la CLI escribe
  `error: cannot reach hermes at <socket>: <motivo>` y sale con 1 sin socket, sale con 2 ante un uso incorrecto, y con
  `failed` escribe `error: <código>: <mensaje>` y sale con 1
- [X] T015 [P] Pruebas del estado de la configuración en `hermes/tests/unit/application/test_configuration_state.py`: la
  instantánea se sustituye de una vez, y quien la tomó antes sigue con la suya
- [X] T016 Pruebas de `hermes serve` en `hermes/tests/integration/test_serve.py` (marca `docker`), con directorios
  temporales, el PostgreSQL de T006 y una `SecretSource` doble con su contraseña: abre `api.sock` en 0600 y `admin.sock`
  en un directorio 0700; no abre ningún puerto TCP; uvicorn no toma las cabeceras de proxy; aplica las migraciones al
  arrancar; y con SIGTERM para y cierra los dos sockets

### Implementación de la fase

- [X] T017 [P] Implementar el dominio común en `hermes/src/hermes/domain/shared/`: `errors.py` (`ErrorCode` y errores de
  dominio), `names.py`, `references.py` (referencia `secret://` y SPIFFE ID), `clock.py` (puerto `Clock`) y `secret.py`
  (`SecretValue`)
- [X] T018 [P] Implementar la configuración por entorno en `hermes/src/hermes/infrastructure/settings.py`
- [X] T019 [P] Implementar los logs en `hermes/src/hermes/infrastructure/logging.py`: formateador JSON y enmascarado
- [X] T020 Implementar el modelo de la configuración en `hermes/src/hermes/domain/configuration/model.py` y su huella
  canónica en `hermes/src/hermes/domain/configuration/digest.py`
- [X] T021 Definir los puertos en `hermes/src/hermes/application/ports.py`: repositorios de configuración, canales,
  auditoría, idempotencia y `jti`; `SecretSource`; `HecateApi`; `IdentityStore`; registro de proveedores y
  `ChatProvider`; `Authenticator`; `Clock`
- [X] T022 Implementar el esqueleto de la API en `hermes/src/hermes/infrastructure/api/`: `app.py` (la fábrica),
  `envelope.py` (sobre y `meta`), `errors.py` (manejadores y estado HTTP de cada código, con la regla de
  [contracts/api.md](contracts/api.md#estado-http)) y el middleware de `X-Request-Id` y del límite de 64 KiB
- [X] T023 Implementar la persistencia en `hermes/src/hermes/infrastructure/persistence/`:
  - `schema.py`, con las ocho tablas y sus restricciones en un `MetaData`;
  - `engine.py`, con el motor asíncrono `postgresql+psycopg` que pide la contraseña a `SecretSource` al abrir cada
    conexión;
  - `migrations/`, con Alembic asíncrono y `versions/0001_initial.py` (tablas y disparador), y la función que las aplica
    al arrancar con `pg_advisory_xact_lock`;
  - `config_repository.py`, que carga y sustituye la configuración entera en una transacción
- [X] T024 Implementar el esqueleto del canal local en `hermes/src/hermes/infrastructure/admin/`: `app.py` (FastAPI en
  `HERMES_ADMIN_DIR/admin.sock` con sus permisos), `client.py` (httpx con transporte por socket Unix) y `cli.py`
  (argparse, subórdenes, códigos de salida y errores de [contracts/cli.md](contracts/cli.md))
- [X] T025 Implementar el estado de la configuración en `hermes/src/hermes/application/configuration.py` y el esqueleto
  de `hermes serve` en `hermes/src/hermes/infrastructure/main.py`: dos `uvicorn.Server` en `api.sock` y `admin.sock`,
  con `proxy_headers` desactivado, las tareas de fondo y la parada con SIGINT o SIGTERM (research R1)

**Checkpoint**: pasan las pruebas de la fase; Hermes arranca, abre sus dos sockets y aplica sus migraciones.

---

## Phase 3: User Story 1 - Enviar un mensaje a un contacto por Telegram (Priority: P1) 🎯 MVP

**Goal**: quien llama pide `chat.send_message` con una plantilla, sus datos y un contacto con su canal verificado, y
Hermes compone el mensaje, lo envía por Telegram, devuelve el resultado en el sobre y lo deja en la auditoría.

**Independent Test**: con la configuración y un canal verificado cargados por los repositorios, un autenticador doble y
Telegram con respuestas grabadas, pedir el envío y comprobar el texto que recibe Telegram, la respuesta y la entrada de
auditoría; y los rechazos de los escenarios 2 a 7.

### Pruebas de la historia

- [X] T026 [P] [US1] Pruebas de la sustitución en `hermes/tests/unit/domain/test_templating.py`: `${data.<ruta>}` con la
  ruta separada por puntos y `${vars.<nombre>}`; un dato que falta es un error; un valor que no es texto se escribe con
  su JSON; cualquier otra forma de `${...}` es un error; hypothesis: un texto sin `${` no cambia
- [X] T027 [P] [US1] Pruebas del mensaje neutral en `hermes/tests/unit/domain/test_neutral_message.py`: tras sustituir,
  cada campo cumple su regla de [data-model.md](data-model.md#textos-de-una-versión-de-plantilla), y uno que no la
  cumple es `validation`
- [X] T028 [P] [US1] Pruebas del contrato del tipo `chat` en `hermes/tests/unit/domain/test_chat_contract.py`:
  `recipients` con exactamente un contacto (con cero o con dos, `validation`, FR-021); la versión de la plantilla es
  opcional (sin ella, la última); con `instance`, el canal en esa instancia o `not_found`; sin ella, el único canal de
  chat del contacto, y con más de uno, `validation`
- [X] T029 [P] [US1] Pruebas del dibujo en `hermes/tests/unit/providers/test_telegram_rendering.py`, con
  [contracts/telegram.md](contracts/telegram.md#dibujo-del-mensaje): HTML escapado (`&`, `<` y `>`); las partes vacías
  no se escriben; con más de 4096 caracteres se quitan hechos desde el último, después acciones, y se acorta el resumen
  con «…»; el título y el enlace no cambian; `truncated` lo dice
- [X] T030 [P] [US1] Pruebas del registro y del proveedor fake en `hermes/tests/unit/providers/test_registry_fake.py`:
  el registro lee los manifiestos, el de Telegram con lo que fija
  [contracts/telegram.md](contracts/telegram.md#manifiesto) (FR-026), y solo carga el fake con
  `HERMES_FAKE_PROVIDERS=enabled`; el fake guarda lo que envía y simula cada fallo de la batería cuando se le pide; con
  `HERMES_FAKE_DIR`, deja cada envío en `outbox.jsonl` y entrega una sola vez cada mensaje que se deja en `inbox/`
  ([contracts/fake.md](contracts/fake.md))
- [X] T031 [P] [US1] Pruebas de `sendMessage` en `hermes/tests/integration/telegram/test_send.py`, con respuestas
  grabadas en `hermes/tests/support/telegram_recordings/`: éxito con `message_id` como `provider_message_ref`,
  `parse_mode: "HTML"` y la vista previa desactivada; 403 da `conflict`; 400 da `provider_unavailable` sin reintento;
  401 y 404 dan `provider_unavailable` sin reintento y marcan las credenciales rechazadas; ningún error lleva la URL, el
  token, el `chat_id` ni el texto de Telegram
- [X] T032 [US1] Pruebas del caso de uso en `hermes/tests/unit/application/test_send_message.py`, con dobles:
  - el escenario 1: el proveedor recibe el mensaje dibujado, y el resultado lleva `recipient`, `instance`,
    `provider_message_ref`, `sent_at` y `truncated`;
  - los escenarios 2 a 4: `validation` si los datos no cumplen el esquema, `not_found` si la plantilla, la versión, la
    instancia o el contacto no existen, `conflict` si el canal no está verificado, y en ninguno se llama al proveedor;
  - el escenario 7: una entrada `send` con quién llama, `chat.send_message`, la instancia, el destinatario por
    referencia, `side_effect` y `discloses_data` resueltos como en [contracts/api.md](contracts/api.md), el contexto de
    auditoría y el resultado, sin el token del bot;
  - el token del bot sale de `SecretSource`
- [X] T033 [P] [US1] Pruebas del repositorio de auditoría en
  `hermes/tests/integration/persistence/test_audit_repository.py`: escribe cada tipo de entrada y no se puede cambiar ni
  borrar
- [X] T034 [US1] Pruebas de la ruta en `hermes/tests/integration/api/test_send_route.py`, con un autenticador doble y el
  fake: `Idempotency-Key` obligatoria y de 1 a 128 caracteres ASCII imprimibles; un cuerpo con campos de más es
  `validation`; un fallo del proveedor responde 200 y uno anterior, su estado; `meta` lleva `service`, `provider` e
  `idempotency_key`; cada respuesta valida contra `hermes.v1.json`
- [X] T035 [US1] Crear la batería de conformidad en `hermes/tests/contract/test_conformance_chat.py`, parametrizada por
  proveedor (el fake y Telegram con respx), con los casos C1, C2, C3 y C10 de
  [contracts/conformance.md](contracts/conformance.md)

### Implementación de la historia

- [X] T036 [P] [US1] Implementar el mensaje neutral y la sustitución en `hermes/src/hermes/domain/messages/neutral.py` y
  `hermes/src/hermes/domain/messages/templating.py`
- [X] T037 [P] [US1] Implementar el contrato del tipo `chat` y la elección del canal en
  `hermes/src/hermes/domain/services/chat/contract.py` y `hermes/src/hermes/domain/services/chat/channels.py`
- [X] T038 [P] [US1] Implementar el registro de proveedores en `hermes/src/hermes/infrastructure/providers/registry.py`
  (lee los manifiestos y solo carga el fake con `HERMES_FAKE_PROVIDERS=enabled`) y el fake en
  `hermes/src/hermes/infrastructure/providers/fake/` (`manifest.json`, `adapter.py` y las bandejas de `HERMES_FAKE_DIR`)
- [X] T039 [P] [US1] Implementar la librería de Telegram en `hermes/src/hermes/infrastructure/providers/telegram/`:
  `manifest.json` como en [contracts/telegram.md](contracts/telegram.md#manifiesto), `rendering.py`, `adapter.py`
  (`sendMessage`) y `errors.py` (la traducción de 400, 401, 403 y 404)
- [X] T040 [US1] Implementar el repositorio de auditoría en
  `hermes/src/hermes/infrastructure/persistence/audit_repository.py`
- [X] T041 [US1] Implementar el caso de uso en `hermes/src/hermes/application/send_message.py`
- [X] T042 [US1] Implementar la ruta en `hermes/src/hermes/infrastructure/api/routes_chat.py` y conectarla en la fábrica
  y en `hermes/src/hermes/infrastructure/main.py`

**Checkpoint**: US1 funciona con un autenticador doble; pasan sus pruebas y las de las fases anteriores.

---

## Phase 4: User Story 2 - Atender solo a componentes autenticados (Priority: P2)

**Goal**: Hermes solo atiende llamadas que llegan por Envoy con el certificado de un componente de Cerbero y un token
corto válido, y responde `auth` a todo lo demás sin decir qué falló.

**Independent Test**: con la CA de pruebas, una llamada por cada credencial inválida recibe `auth` sin efectos, y una
correcta se atiende; Envoy sustituye la cabecera del cliente y Hermes no tiene puerto.

### Pruebas de la historia

- [X] T043 [P] [US2] Pruebas de la cabecera en `hermes/tests/unit/test_xfcc.py`, con
  [contracts/proxy.md](contracts/proxy.md#la-cabecera-que-lee-hermes): un elemento con `Hash`, `Cert`, `Chain` y un solo
  `URI`, valores entre comillas y con codificación URL; `Hash` coincide con `Cert`; `By`, si viene, es el SPIFFE ID de
  Hermes; sin cabecera, con dos elementos o con un campo que falta, error; hypothesis: una cabecera arbitraria nunca
  rompe el lector
- [X] T044 [P] [US2] Pruebas de la autenticación en `hermes/tests/unit/application/test_authentication.py`, con la CA de
  T006 (FR-003):
  - el certificado: válido; caducado; de otra CA; con su serie en la CRL; con más de un nombre, sin SPIFFE ID o de otro
    trust domain;
  - el token: que falta; caducado; de más de 300 s; con otro `aud`; con un `iss` distinto del certificado; con un `kid`
    desconocido, de otra identidad o que no está activo; con la firma mal; con un `jti` ya usado; los 30 s de margen
    hacia los dos lados;
  - todos los fallos dan `auth` con el mismo mensaje, y no se toma ningún permiso del token (FR-004);
  - `cerbero_audit`: un objeto de hasta 16 claves de 1 a 64 caracteres, con valores de texto de hasta 256 y 2 KiB en
    total, se acepta; con otra forma, `validation`
- [X] T045 [P] [US2] Pruebas del repositorio de `jti` en `hermes/tests/integration/persistence/test_used_tokens.py`: un
  `(issuer, jti)` se guarda una vez, el segundo se rechaza, también tras reabrir la conexión, y se borra pasado
  `expires_at`
- [X] T046 [US2] Pruebas de la autenticación en la API en `hermes/tests/integration/api/test_auth_routes.py`: cada ruta
  responde `auth` (401) con cada credencial inválida de la historia 2, con el mismo mensaje y sin llamar al proveedor, y
  la API no tiene ninguna ruta sin autenticar (SC-002 en la API)
- [X] T047 [US2] Pruebas de Envoy en `hermes/tests/e2e/test_proxy.py` (marca `docker`), con la configuración de T052, un
  servidor de eco en el socket de la API (`hermes/tests/e2e/stubs/echo_upstream.py`) y certificados de la CA de pruebas:
  - una cabecera `X-Forwarded-Client-Cert` enviada por el cliente no llega: llega la del certificado de la conexión, con
    su cadena;
  - sin certificado de cliente, la petición llega sin cabecera, y con uno de otra CA, llega con su cabecera (Envoy no lo
    verifica);
  - el contenedor de Hermes no publica ningún puerto, y Hermes no escucha en TCP

### Implementación de la historia

- [X] T048 [P] [US2] Implementar el lector de la cabecera en `hermes/src/hermes/infrastructure/api/xfcc.py`
- [X] T049 [US2] Implementar `Authenticator` en `hermes/src/hermes/application/authentication.py` (research R3), con los
  puertos de la CA raíz, las listas de revocación, las claves publicadas, los `jti` usados y el reloj
- [X] T050 [US2] Implementar el repositorio de `jti` en
  `hermes/src/hermes/infrastructure/persistence/tokens_repository.py` y su limpieza como tarea de fondo
- [X] T051 [US2] Implementar la dependencia de autenticación en `hermes/src/hermes/infrastructure/api/auth.py` y usarla
  en todas las rutas, en lugar del doble de US1
- [X] T052 [US2] Escribir `hermes/deploy/envoy/envoy.yaml` y sus recursos SDS (`certificate.yaml` y `validation.yaml`)
  como fija [contracts/proxy.md](contracts/proxy.md), y el servicio de Envoy en `hermes/deploy/compose.yaml` (imagen con
  su etiqueta exacta, UID 10001, directorio del proxy en solo lectura y directorio de la API)

**Checkpoint**: US1 y US2 funcionan; ninguna ruta atiende sin autenticar.

---

## Phase 5: User Story 3 - Mantener su identidad y sus secretos con Hécate (Priority: P3)

**Goal**: Hermes se da de alta con el token del admin, renueva solo su certificado, mantiene al día lo que publica
Hécate y resuelve sus secretos con caché, también con Hécate sellada o caída.

**Independent Test**: contra una Hécate real lanzada con `HECATE_BIN`, darse de alta, renovar repitiendo la misma
petición y resolver; con el reloj inyectado, encadenar renovaciones y comprobar la caché con Hécate sellada.

### Pruebas de la historia

- [X] T053 [P] [US3] Pruebas de las claves en `hermes/tests/unit/hecate/test_keys.py`: clave TLS P-256, clave de firma
  Ed25519 distinta, solicitud PKCS#10 sin nombre y clave pública en PEM `PUBLIC KEY`, como pide la guía de Hécate
- [X] T054 [P] [US3] Pruebas del almacén de identidad en `hermes/tests/unit/hecate/test_identity_store.py`: el
  directorio de estado en 0700 y sus ficheros en 0600; cada generación en `gen-<n>/` del directorio del proxy y el
  enlace `current` sustituido de forma atómica; la renovación en curso y las últimas publicaciones se guardan y se
  vuelven a leer
- [X] T055 [P] [US3] Pruebas de la verificación TLS hacia Hécate en `hermes/tests/integration/hecate/test_tls.py`, con
  un servidor TLS de prueba y la CA de T006:
  - con el certificado `spiffe://<td>/hecate`, la petición sale;
  - con el certificado de otra identidad, o con uno revocado, la conexión se corta tras el saludo y el servidor no
    recibe ningún byte HTTP;
  - en el alta, una CA raíz que no tiene la huella del token corta antes de enviar nada
- [X] T056 [US3] Pruebas de la renovación en `hermes/tests/unit/application/test_renewal.py`, con un doble de
  `HecateApi` y `FakeClock`: a los dos tercios de la vida y una vez por certificado; sin respuesta, la misma petición
  (mismas claves, solicitud e `Idempotency-Key`) con un token nuevo; con `unavailable` o sin conexión, conserva el
  certificado y reintenta cada 60 s; con el certificado nuevo, escribe la generación y firma con la clave nueva; 15
  renovaciones seguidas en 30 días con certificados de 3 días, sin un segundo intento (SC-005); el doble aplica la regla
  de Hécate: otra petición con un certificado ya renovado es un segundo intento
- [X] T057 [P] [US3] Pruebas de las publicaciones en `hermes/tests/unit/application/test_publications.py`: se refrescan
  cada 240 s; si Hécate no responde, sigue la última copia; al arrancar, se usa la guardada (FR-008)
- [X] T058 [P] [US3] Pruebas de `SecretCache` en `hermes/tests/unit/application/test_secret_cache.py`, con `FakeClock`:
  sin preguntar durante 300 s; refresco después; con Hécate sellada o caída, el valor sigue hasta los 3600 s y después
  el envío falla con `internal` sin llegar al proveedor (SC-004, con el caso de uso de US1); la contraseña de la base
  sale del valor vigente (FR-009)
- [X] T059 [P] [US3] Pruebas de la validez de la identidad en `hermes/tests/unit/application/test_identity_state.py`:
  con el certificado caducado, revocado en la CRL, rechazado con `auth` al renovar, o con Hécate presentando otra CA
  raíz, Hermes no está listo y dice `needs a new enrollment token` (FR-010)
- [X] T060 [US3] Pruebas contra Hécate real en `hermes/tests/integration/hecate/test_client.py` (marca `hecate`), con el
  ayudante `hermes/tests/support/hecate.py`, que lanza `hecate serve` con un directorio temporal, el perfil `moderate` y
  certificados de 1 hora, y prepara por la entrada estándar la inicialización, el desbloqueo, la CA, los tokens y los
  secretos:
  - el alta da el certificado, la cadena y el `call_key_id`;
  - una Hécate con otra CA raíz se rechaza y el token sigue sin usar;
  - la renovación da un certificado nuevo, y repetir la misma petición tras perder la respuesta da el mismo, sin un
    segundo intento;
  - la resolución es todo o nada, y la lista de revocación y las claves se leen
- [X] T061 [US3] Pruebas del alta por el canal local y la CLI en `hermes/tests/integration/admin/test_enroll.py`:
  `hermes enroll` pide el token sin eco en un terminal y lo lee de la entrada estándar si no lo es, nunca como
  argumento; escribe `enrolled <spiffe id> serial <serie> until <fecha>`; responde `conflict` con una identidad válida,
  `auth` si Hécate no coincide con la huella y `unavailable` si Hécate no responde
- [X] T062 [US3] Prueba de la recarga en `hermes/tests/e2e/test_reload.py` (marcas `docker` y `hecate`): tras una
  renovación real escrita como generación nueva, Envoy presenta el certificado nuevo sin reiniciarse ni cortar una
  conexión abierta (escenario 2)
- [X] T063 [P] [US3] Pruebas del arranque en `hermes/tests/unit/test_startup.py`, con dobles de Hécate, de la base y de
  los proveedores: sigue el orden de research R1 (identidad, publicaciones, contraseña de la base, conexión,
  migraciones, configuración, prueba de las instancias y sondeos); no pasa a listo si falla un paso, y dice cuál; sin
  identidad, espera al alta

### Implementación de la historia

- [X] T064 [P] [US3] Implementar las claves y la solicitud en `hermes/src/hermes/infrastructure/hecate/keys.py`
- [X] T065 [P] [US3] Implementar el almacén de identidad en `hermes/src/hermes/infrastructure/hecate/identity_store.py`
- [X] T066 [US3] Implementar el cliente de Hécate en `hermes/src/hermes/infrastructure/hecate/tls.py` (el `SSLContext` y
  el `network_backend` de httpcore que exige el SPIFFE ID de Hécate antes de enviar) y
  `hermes/src/hermes/infrastructure/hecate/client.py` (alta con la comprobación previa, renovación, revocaciones,
  claves, una clave, resolución y el token corto de cada llamada)
- [X] T067 [US3] Implementar `hermes/src/hermes/application/identity.py` (alta, supervisor de la renovación,
  publicaciones y validez) y `hermes/src/hermes/application/secrets.py` (`SecretCache`)
- [X] T068 [US3] Implementar `identity.enroll` en el canal local y `hermes enroll` en
  `hermes/src/hermes/infrastructure/admin/`
- [X] T069 [US3] Completar el arranque en `hermes/src/hermes/infrastructure/main.py` (research R1): identidad,
  publicaciones, contraseña de la base, conexión, migraciones, configuración, prueba de las instancias y sondeos; y
  `SecretCache` como `SecretSource` de los proveedores y de la base

**Checkpoint**: US1 a US3 funcionan con una Hécate real: Hermes tiene identidad, la renueva y resuelve sus secretos.

---

## Phase 6: User Story 4 - Configurar Hermes y verificar el canal de un contacto (Priority: P4)

**Goal**: el admin aplica con la CLI la configuración entera, la exporta, pide el código de cada canal, y la persona lo
verifica enviándoselo al bot.

**Independent Test**: aplicar un fichero, comprobar que un envío al contacto se rechaza, verificar su canal con un
código que llega por el sondeo, comprobar que el envío llega, y aplicar ficheros con errores sin que cambie nada.

### Pruebas de la historia

- [X] T070 [P] [US4] Pruebas del cargador en `hermes/tests/unit/config_file/test_loader.py`: claves repetidas, alias,
  anclas, ficheros de más de 1 MiB y texto que no es UTF-8 se rechazan con su ruta
- [X] T071 [P] [US4] Pruebas de los modelos en `hermes/tests/unit/config_file/test_models.py`, con
  [contracts/config-file.md](contracts/config-file.md): campos desconocidos, conversiones implícitas y nombres que no
  siguen `^[a-z][a-z0-9_]{0,62}$` se rechazan; `version` es `1`
- [X] T072 [P] [US4] Pruebas de la validación en `hermes/tests/unit/domain/test_configuration_validation.py`: cada regla
  de 1 a 8 de [contracts/config-file.md](contracts/config-file.md#validación-al-aplicar) da su error con su ruta, sin
  valores; entre ellas, unos `params` con un campo que el manifiesto no declara como no secreto
- [X] T073 [P] [US4] Pruebas del código en `hermes/tests/unit/domain/test_verification_code.py`: 8 caracteres del
  alfabeto de Crockford sin `I`, `L`, `O` ni `U`; se enseña como `XXXX-XXXX`; se compara sin mayúsculas, espacios ni
  guiones (hypothesis); se guarda su SHA-256; caduca a los `HERMES_VERIFICATION_CODE_MINUTES`, de 5 a 1440
- [X] T074 [P] [US4] Pruebas del plan en `hermes/tests/unit/domain/test_configuration_diff.py`: los cambios salen
  ordenados (borrar, cambiar, crear); un canal que se borra o cambia de instancia dice que pierde su verificación;
  cambiar una versión de plantilla existente es un error; `plan_digest` cubre el fichero y lo que había en la base
- [X] T075 [US4] Pruebas de aplicar y exportar en `hermes/tests/integration/persistence/test_apply_export.py`:
  - una referencia `secret://` nueva o cambiada que no se resuelve es `validation`, y con Hécate caída, `unavailable`;
  - con otra `plan_digest`, `conflict`; un fallo a mitad no escribe nada; aplicar dos veces no cambia nada (SC-010);
  - tras aplicar, la configuración nueva se carga de una vez, y un envío a la vez usa la de antes o la de después;
  - lo exportado sale ordenado, sin secretos, direcciones ni códigos, y aplicarlo no cambia nada (FR-025);
  - una entrada `config_applied` con los cambios, sin valores de secretos
- [X] T076 [US4] Pruebas de pedir un código en `hermes/tests/unit/application/test_issue_code.py`: el canal queda sin
  verificar, sin dirección y con la huella y la caducidad del código nuevo; una entrada `verification_code_issued` sin
  el código; `not_found` para un contacto o un canal que no existe
- [X] T077 [US4] Pruebas de recoger el código en `hermes/tests/unit/application/test_verify_channel.py`: un texto de un
  chat privado que coincide con un código pendiente y no caducado de un canal de esa instancia liga la dirección,
  verifica y borra el código, y Hermes contesta «Canal verificado.»; uno caducado, usado o desconocido no hace nada ni
  recibe respuesta; los grupos y los demás mensajes se ignoran; una entrada `channel_verified` sin la dirección; el
  mismo mensaje dos veces no hace nada la segunda
- [X] T078 [P] [US4] Pruebas del sondeo en `hermes/tests/integration/telegram/test_polling.py`, con respx: `getUpdates`
  con `offset`, `timeout: 25` y `allowed_updates: ["message"]`; el `offset` avanza; un fallo de transporte espera y
  reintenta; el contenido de los mensajes no sale en los logs; añadir C12 a la batería de T035
- [X] T079 [US4] Prueba de estados en `hermes/tests/unit/application/test_verified_only.py`: una máquina de estados de
  hypothesis con aplicar, pedir código, verificar y enviar comprueba que solo los canales verificados reciben mensajes
  (SC-008)
- [X] T080 [US4] Pruebas del canal local y de la CLI en `hermes/tests/integration/admin/test_config_cli.py`, con
  [contracts/admin-channel.md](contracts/admin-channel.md) y [contracts/cli.md](contracts/cli.md):
  - `config.plan`, `config.apply`, `config.export` y `contacts.code` responden como fija el contrato;
  - `hermes config apply` escribe un cambio por línea con `+`, `~` o `-`, pregunta `apply these <n> changes? [yes/no]`,
    escribe `applied <n> changes` o `no changes`, y con otra respuesta o sin entrada, `cancelled` y 1; `--yes` no
    pregunta;
  - los errores de validación salen como `error: <ruta>: <mensaje>`;
  - `hermes config export --out` no sobrescribe y crea el fichero con 0600;
  - `hermes contacts code` escribe `code <XXXX-XXXX> for <contacto> on <instancia> expires <fecha>`

### Implementación de la historia

- [X] T081 [P] [US4] Implementar el fichero de configuración en `hermes/src/hermes/infrastructure/config_file/`:
  `loader.py` (YAML seguro), `models.py` (Pydantic estricto) y `export.py` (YAML ordenado)
- [X] T082 [P] [US4] Implementar la validación y el plan en `hermes/src/hermes/domain/configuration/validation.py` y
  `hermes/src/hermes/domain/configuration/diff.py`
- [X] T083 [P] [US4] Implementar el código en `hermes/src/hermes/domain/verification/code.py`
- [X] T084 [US4] Implementar plan, aplicar y exportar en `hermes/src/hermes/application/configuration.py`, con la
  aplicación en una transacción y un bloqueo consultivo en `config_repository.py`, la recarga de la instantánea y la
  auditoría
- [X] T085 [US4] Implementar pedir y recoger códigos en `hermes/src/hermes/application/verification.py` y
  `hermes/src/hermes/infrastructure/persistence/channels_repository.py`, y la entrega de los mensajes que llegan a la
  bandeja de entrada del fake
- [X] T086 [US4] Implementar el sondeo en `hermes/src/hermes/infrastructure/providers/telegram/polling.py` y arrancar
  uno por instancia en `hermes/src/hermes/infrastructure/main.py`
- [X] T087 [US4] Implementar `config.plan`, `config.apply`, `config.export` y `contacts.code` en el canal local y las
  órdenes `config apply`, `config export` y `contacts code` en `hermes/src/hermes/infrastructure/admin/`

**Checkpoint**: US1 a US4 funcionan: la configuración se aplica con la CLI y los canales se verifican por Telegram.

---

## Phase 7: User Story 5 - Repetir sin duplicar y reintentar solo lo seguro (Priority: P5)

**Goal**: una petición repetida no envía dos veces, Hermes solo reintenta cuando sabe que el mensaje no salió, y cada
instancia respeta su límite de uso.

**Independent Test**: repetir una petición con la misma clave, a la vez y con un corte simulado, y comprobar que sale un
solo mensaje; con el fake, provocar fallos antes y después de enviar y comprobar cuándo reintenta y cuándo responde
`outcome_unknown`.

### Pruebas de la historia

- [X] T088 [P] [US5] Pruebas de la huella en `hermes/tests/unit/domain/test_idempotency.py`: el mismo método, ruta y
  cuerpo dan la misma huella con otro orden de claves o espacios (hypothesis); un cambio en cualquiera da otra
- [X] T089 [P] [US5] Pruebas del repositorio en `hermes/tests/integration/persistence/test_idempotency_repository.py`:
  `in_progress`, después `completed` con la respuesta y su estado HTTP; la búsqueda por `(caller, key)`; la limpieza a
  las 24 horas
- [X] T090 [US5] Pruebas del envío con idempotencia en `hermes/tests/integration/api/test_idempotency.py`:
  - misma clave y mismo cuerpo: la misma respuesta y el proveedor una sola vez (escenario 1);
  - otro cuerpo: `conflict` (409, escenario 2);
  - 100 peticiones iguales a la vez, y con un corte simulado entre el envío y su registro, dan como mucho un mensaje
    (SC-003);
  - tras un corte con la fila `in_progress`, la repetición responde `timeout` con `outcome_unknown` y la fila queda
    `completed`;
  - una petición que falla antes de llegar al proveedor no deja fila y se puede repetir con la misma clave
- [X] T091 [P] [US5] Pruebas de los reintentos en `hermes/tests/integration/telegram/test_retries.py` y en la batería
  (casos C4 a C8): los fallos que no salieron se reintentan con espera exponencial y jitter (base 0,2 s, tope 2 s) sin
  pasar de 10 s; un 429 con un `retry_after` que cabe se espera y uno que no cabe da `rate_limited` con `retry_after_s`;
  sin conexión hasta el final, `provider_unavailable`; `ReadTimeout`, `WriteTimeout`, `RemoteProtocolError`, un error de
  lectura tras enviar o un 5xx dan `timeout` con `outcome_unknown` y sin reintento
- [X] T092 [P] [US5] Pruebas del límite de uso en `hermes/tests/unit/application/test_rate_limits.py`, con `FakeClock`:
  30 por minuto del manifiesto o el de la instancia (de 1 a 600); sin fichas, `rate_limited` con `retry_after_s`, HTTP
  429 con `Retry-After` y sin llamar al proveedor (escenario 6)

### Implementación de la historia

- [X] T093 [P] [US5] Implementar la huella y la clave en `hermes/src/hermes/domain/idempotency/`
- [X] T094 [US5] Implementar el repositorio en `hermes/src/hermes/infrastructure/persistence/idempotency_repository.py`
  y su limpieza como tarea de fondo
- [X] T095 [US5] Implementar `hermes/src/hermes/application/idempotency.py` (un bloqueo por `(caller, key)` y el flujo
  de research R11) y usarlo en `send_message.py` y en la ruta
- [X] T096 [P] [US5] Implementar los reintentos en `hermes/src/hermes/infrastructure/providers/retry.py` y la
  clasificación de cada fallo en el adaptador de Telegram y en el fake
- [X] T097 [US5] Implementar el límite de uso en `hermes/src/hermes/application/rate_limits.py` y usarlo en
  `send_message.py`

**Checkpoint**: US1 a US5 funcionan: repetir no duplica y los reintentos son solo los seguros.

---

## Phase 8: User Story 6 - Saber qué ofrece Hermes y si está sano (Priority: P6)

**Goal**: quien llama sabe qué tipos de servicio, proveedores, instancias y funciones ofrece Hermes y en qué estado está
cada instancia, y quien lo opera sabe por el canal local si está vivo y listo.

**Independent Test**: consultar `/v1/capabilities` con una instancia sana y otra con su token rechazado, lanzar la
prueba de conexión de cada una y consultar la salud por el canal local con y sin identidad, base y configuración.

### Pruebas de la historia

- [X] T098 [P] [US6] Pruebas del estado de una instancia en `hermes/tests/unit/domain/test_instance_status.py`, con
  [data-model.md](data-model.md#estado-de-una-instancia): `unavailable` si falló la última prueba, si se rechazaron las
  credenciales hasta que una prueba funcione o si fallaron las tres últimas llamadas; `degraded` si falló alguna de las
  diez últimas; `healthy` en lo demás
- [X] T099 [US6] Pruebas de las capacidades en `hermes/tests/integration/api/test_capabilities.py`: la respuesta de
  [contracts/api.md](contracts/api.md#get-v1capabilities) valida contra el contrato, refleja el estado de cada instancia
  y no lleva contactos ni direcciones
- [X] T100 [US6] Pruebas de la prueba de conexión en `hermes/tests/integration/api/test_instance_test.py`: con `getMe`
  correcto, `healthy`; con un fallo, `unavailable` dentro del sobre (200); una instancia desconocida, `not_found` (404);
  sin `Idempotency-Key`; añadir C11 a la batería de T035
- [X] T101 [US6] Pruebas de la salud en `hermes/tests/integration/admin/test_health.py`: la operación `health` del canal
  local da `live`, `ready` y, si no está listo, `needs a new enrollment token`, `database unavailable` o
  `configuration not loaded`, sin datos de instancias ni contactos; `hermes health` escribe `ready` y sale con 0, o
  `not ready: <motivo>` y sale con 1, y con `--live` sale con 0 si el proceso responde; la API de red no tiene rutas de
  salud; al arrancar, Hermes prueba cada instancia

### Implementación de la historia

- [X] T102 [P] [US6] Implementar el estado de las instancias en `hermes/src/hermes/domain/instances/status.py` y las
  capacidades y la prueba de conexión en `hermes/src/hermes/application/capabilities.py`
- [X] T103 [US6] Implementar `hermes/src/hermes/application/health.py`, la ruta en
  `hermes/src/hermes/infrastructure/api/routes_capabilities.py`, la operación `health` del canal local y `hermes health`
  en `hermes/src/hermes/infrastructure/admin/`, `getMe` en el adaptador de Telegram y la prueba de cada instancia al
  arrancar

**Checkpoint**: las seis historias funcionan.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [X] T104 Escribir el cliente de referencia en `hermes/tests/support/caller.py`, con las órdenes `enroll`, `send` y
  `bench` del apartado 7 de [quickstart.md](quickstart.md): se da de alta en Hécate, firma su token corto (con
  `cerbero_audit` opcional) y llama a Hermes a través de Envoy
- [X] T105 Escribir el E2E en `hermes/tests/e2e/test_flow.py` (marcas `docker` y `hecate`), con el compose y el fake
  (`HERMES_FAKE_PROVIDERS=enabled` en un fichero de compose de pruebas): preparar Hécate, `hermes enroll`, aplicar la
  configuración, pedir el código y verificar con un mensaje dejado en la bandeja de entrada del fake
  (`HERMES_FAKE_DIR`), enviar con el cliente de referencia y comprobar el envío en su bandeja de salida, las
  credenciales inválidas de la historia 2 a través de Envoy (SC-002), y con PostgreSQL parado, `hermes health` en
  `not ready: database unavailable` y los envíos en `unavailable`
- [X] T106 Escribir la prueba de fugas en `hermes/tests/integration/test_no_leaks.py`: recorrer el alta, aplicar,
  exportar, pedir y recoger un código y los envíos con éxito y con fallos, y buscar el token del bot, los códigos, las
  direcciones, los tokens cortos y los certificados en los logs, las respuestas, la auditoría y lo exportado, en texto,
  base64, codificación URL y JSON escapado (SC-007); y comprobar que ninguna tabla de la base guarda un secreto, solo
  sus referencias (FR-023)
- [X] T107 Comprobar que todas las respuestas de la API que reciben las suites, también las de error, validan contra
  `hermes.v1.json` y `common.v1.json` (SC-009), con una comprobación común en `hermes/tests/integration/api/conftest.py`
- [X] T108 Pasar las comprobaciones de calidad del apartado 1 de [quickstart.md](quickstart.md): `ruff check`,
  `ruff format --check`, `mypy --strict hermes/src` y `pip-audit`, sin avisos
- [X] T109 Ejecutar la suite entera de Hermes con `HECATE_BIN` y Docker, con todas las marcas, y la de Hécate en macOS y
  en su contenedor Linux por el sobre común 1.1.0 (apartado 1 de quickstart.md)
- [X] T110 Recorrer con el propietario los apartados 2 a 8 de [quickstart.md](quickstart.md) con Telegram real (su bot y
  su cuenta): verificación, envío, repetición, el `bench` de SC-001 y los fallos
- [X] T111 Consolidar los docs según FR-032 y research R20:
  - `docs/hermes.md`: la configuración en la base aplicada con la CLI y el repositorio privado solo para las librerías
    propias; el proxy, los dos sockets y el canal local; la salud por el canal local, fuera de la API; las rutas
    versionadas, los campos nuevos de `meta` y `cerbero_audit`; `conflict` para una clave reutilizada con otro cuerpo;
    la verificación con el código que pide el admin;
  - `ARCHITECTURE.md`: el token corto sin los permisos que conceden el acceso y con `cerbero_audit`; la persistencia de
    las apps; el resumen de Hermes;
  - `GLOSSARY.md`: los términos que fije la consolidación;
  - `DECISIONS.md`: «APIs internas en Python y certificado del cliente» y «Persistencia por proyecto» a Tomadas con los
    ADR 0011 y 0012; «Detalle de contratos y versionado de Hermes» resuelto; «Repositorio privado de lo propio» con las
    instancias y las plantillas de Hermes en su base; el acceso por app como diseñado y no implementado; los riesgos
    aceptados del token ligado a la identidad que declara el proxy y de los secretos en la memoria de Python;
  - el ADR 0013 del acceso por app en `docs/adr/0013-acceso-por-app.md`, con su fila en `docs/adr/README.md`;
  - `README.md`: lo que ya hace Hermes, en «Estado del Proyecto»;
  - `shared/contracts`: `hermes.v1.json` y `common.v1.json`, revisados contra lo implementado
- [X] T112 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `feat:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, terminada con la ruta de la guía de revisión;
  - deben recoger `hermes/`, `shared/contracts/`, el cambio de `hecate/`, `docs/`, `ARCHITECTURE.md`, `GLOSSARY.md`,
    `DECISIONS.md`, `README.md` y `specs/008-hermes-send-message/`;
  - el propietario hace el commit, el push y la PR
- [X] T113 Pedir a Hécate la clave de un `kid` que no está en la lista (`GET /v1/keys/{kid}`), hallado por el E2E: un
  componente recién dado de alta o renovado recibía `auth` (`unknown_key`) hasta el siguiente refresco, 240 s. En
  `hermes/src/hermes/application/authentication.py` y `hermes/src/hermes/application/identity.py`, con pruebas en
  `hermes/tests/unit/application/test_authentication.py` y `test_publications.py`: solo con la cadena validada y un
  `kid` con forma de huella RFC 7638; activa y de la identidad del certificado; una petición por `kid` a la vez;
  guardada hasta el siguiente refresco; desconocida, `auth`; Hécate sin responder, `unavailable`
  ([research R3](research.md#r3-autenticación-de-las-llamadas))

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias; es el contrato, las herramientas y el entorno de pruebas.
- **Foundational (Phase 2)**: depende del Setup y bloquea las historias.
- **US1 (Phase 3)**: depende de la fase fundacional; usa un autenticador doble y la configuración cargada por los
  repositorios.
- **US2 (Phase 4)**: depende de US1, porque sustituye su autenticador doble y prueba sus rutas.
- **US3 (Phase 5)**: depende de US2, porque da al autenticador la CA raíz, la lista de revocación y las claves de
  Hécate, y de US1, por `SecretCache` en los envíos.
- **US4 (Phase 6)**: depende de la fase fundacional y de US1 (los envíos de su prueba). Puede adelantarse a US2 y US3:
  la comprobación de los secretos usa un doble de `SecretSource` hasta que existe `SecretCache`.
- **US5 (Phase 7)**: depende de US1.
- **US6 (Phase 8)**: depende de US3 (la validez de la identidad en la salud) y de US1 (los proveedores).
- **Polish (Phase 9)**: al final; T105 depende de T104.

### Within Each Phase

- Las pruebas primero, y deben fallar; después, dominio → aplicación → infraestructura → CLI.
- Las tareas que tocan el mismo fichero van en orden: `send_message.py` (T041, T095 y T097), la batería de conformidad
  (T035, T078, T091 y T100), `main.py` (T025, T042, T069, T086 y T103) y `compose.yaml` (T007 y T052).
- Cada fase termina en su checkpoint con la suite completa en verde.

### Parallel Opportunities

- Setup: T002, T004, T006 y T007.
- Fase 2: las pruebas T008 a T011 y T015, y las implementaciones T017 a T019.
- US1: las pruebas T026 a T031 y T033, y las implementaciones T036 a T039.
- US2: las pruebas T043 a T045 y la implementación T048.
- US3: las pruebas T053 a T055, T057 a T059 y T063, y las implementaciones T064 y T065.
- US4: las pruebas T070 a T074 y T078, y las implementaciones T081 a T083.
- US5: las pruebas T088, T089, T091 y T092, y las implementaciones T093 y T096.
- US6: la prueba T098 y la implementación T102.

### Parallel Example: User Story 1

```bash
Task: "Pruebas de la sustitución en hermes/tests/unit/domain/test_templating.py"
Task: "Pruebas del mensaje neutral en hermes/tests/unit/domain/test_neutral_message.py"
Task: "Pruebas del contrato del tipo chat en hermes/tests/unit/domain/test_chat_contract.py"
Task: "Pruebas del dibujo en hermes/tests/unit/providers/test_telegram_rendering.py"
Task: "Pruebas del registro y del proveedor fake en hermes/tests/unit/providers/test_registry_fake.py"
Task: "Pruebas de sendMessage en hermes/tests/integration/telegram/test_send.py"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: el contrato, el entorno de pruebas, la persistencia y los esqueletos de la API y del canal
   local.
2. US1: enviar un mensaje con su plantilla, su dibujo de Telegram y su auditoría.
3. **Revisión del propietario:** las pruebas de US1 y la batería de conformidad con sus primeros casos.

### Siguientes incrementos

1. US2: la autenticación y Envoy, con su revisión (las pruebas de la historia 2 y del proxy).
2. US3: la identidad y los secretos con una Hécate real, con su revisión (apartados 3 y 4 de quickstart.md).
3. US4: la configuración con la CLI y la verificación de los canales, con su revisión (apartados 5 y 6).
4. US5: idempotencia, reintentos y límites, con su revisión (repeticiones del apartado 7).
5. US6: capacidades y salud.
6. Polish: el E2E, las fugas, el contrato, la calidad, la suite entera, el recorrido con Telegram real, la consolidación
   y el mensaje de commit.

La feature se entrega entera en una PR con un commit de squash `feat:`; los incrementos son puntos de revisión.

## Phase 10: Convergence

- [X] T114 Llevar al diseño de la Access Policy de Hécate la decisión del acceso por app ([ADR 0013](../../docs/adr/0013-acceso-por-app.md)): en `docs/hecate/acceso.md`, un apartado que diga que Hécate guarda qué identidades pueden llamar a cada app, a la app entera y sin permisos por función, que el token corto no concede acceso y que la lista y su consulta desde cada app las construye la 009 (diseñado, no implementado, como dice DECISIONS); y en el párrafo de acceso de `docs/hecate.md`, su mención con el enlace a ese apartado. Hoy ni `acceso.md` ni `hecate.md` lo nombran, aunque FR-032 pide consolidarla también «en el diseño de la Access Policy de Hécate» y la regla de consolidación de la constitución lleva a los docs del componente lo que cambia su diseño, per FR-032 / Constitución (Consolidación) (partial)
- [X] T115 Añadir a `hermes/tests/e2e/test_flow.py` los casos de la historia 2 que el entorno puede producir y que aún no prueba: un token que vive más de 300 s, y el certificado de un segundo componente dado de alta para revocarlo con `hecate certificates revoke <serie> --urgent`, esperando al siguiente refresco de la lista de revocación de Hermes (240 s como mucho). Decir en el apartado 8 de `specs/008-hermes-send-message/quickstart.md` que el certificado caducado se prueba en `hermes/tests/integration/api/test_auth_routes.py`, porque Hécate solo emite certificados vigentes. Hoy el E2E prueba 8 de los 11 casos y el quickstart dice que los prueba todos, per T105 / SC-002 (partial)
- [X] T116 Rechazar en la validación de la configuración (regla 3, `hermes/src/hermes/domain/configuration/validation.py`) una referencia de secreto de una instancia que no sea `secret://hermes/<proveedor>/<instancia>/<campo>`, con el proveedor y el nombre de esa instancia y un campo de secreto de su manifiesto, y dar la ruta del fallo en el fichero; con su caso en `hermes/tests/unit/domain/test_configuration_validation.py` y la regla en `specs/008-hermes-send-message/contracts/config-file.md`. Hoy se acepta cualquier referencia `secret://`, también de otro ámbito o de otra instancia, per FR-019 (partial)
- [X] T117 Quitar el prefijo «Value error, » que Pydantic pone a los mensajes de los validadores propios del fichero de configuración (`hermes/src/hermes/infrastructure/config_file/models.py`), para que `error: <ruta>: <mensaje>` diga solo el motivo, y comprobarlo en `hermes/tests/unit/config_file/test_models.py`. Lo vio el recorrido de T110 con un `bot_token` en claro: `error: instances[0].secrets.bot_token: Value error, must be a secret:// reference with the rules of Hécate`, per contracts/config-file.md (partial)
- [X] T118 Anotar en `specs/008-hermes-send-message/contracts/admin-channel.md` que la operación `health` deja su línea a nivel `debug` mientras Hermes está listo, y a `info` con el motivo cuando no lo está, porque la pide el healthcheck del contenedor cada 10 s (`hermes/src/hermes/infrastructure/admin/routes_health.py`). Hoy el contrato dice que cada operación deja una línea con su nombre y su resultado, sin esa excepción, per contracts/admin-channel.md (contradicts)
- [X] T119 Quitar «y salud» del comentario de `hermes.v1.json` en la estructura de código de `specs/008-hermes-send-message/plan.md`: la salud va por el canal local, como dicen el mismo plan, research R15 y R16 y el OpenAPI, que no la tiene, per plan: estructura (contradicts)
- [X] T120 Tras T114 a T119, volver a pasar las comprobaciones de calidad y la suite completa de Hermes con `HECATE_BIN` y Docker, con el E2E, del apartado 1 de `specs/008-hermes-send-message/quickstart.md`, y el comprobador de enlaces sobre los docs tocados: las pasadas de T108 y T109 son anteriores a esta fase, per T108 / T109 (partial)

## Phase 11: Convergence

- [X] T121 Validar contra el OpenAPI todas las respuestas de la API que reciben las suites de fuera de `hermes/tests/integration/api/`: llevar la comprobación común de `hermes/tests/integration/api/conftest.py` a `hermes/tests/integration/conftest.py`, para que cubra también `hermes/tests/integration/test_no_leaks.py`, y validar en `hermes/tests/e2e/test_flow.py` cada respuesta en `send` y en `capabilities_status`, que llegan por Envoy y no por el transporte ASGI; después, volver a pasar la calidad y la suite completa con el E2E. Hoy quedan sin validar las seis respuestas de la prueba de fugas y, en el E2E, la repetición, el `conflict` por otra petición con la misma clave, los dos envíos del `jti` reutilizado, el envío antes de revocar y las consultas de capacidades, per T107 / SC-009 (partial)

## Phase 12: Convergence

- [X] T122 Dejar en la auditoría el envío que un corte deja a medias: guardar con la reserva de la `Idempotency-Key`, justo antes del proveedor, la entrada `send` sin su resultado (una columna nueva de `idempotency_keys` en `hermes/src/hermes/infrastructure/persistence/schema.py`, su migración y `specs/008-hermes-send-message/data-model.md`), y escribirla con `timeout` y `outcome_unknown: true` como resultado, en la misma transacción, cuando una repetición completa una fila `in_progress` (`hermes/src/hermes/application/idempotency.py`) y cuando la limpieza de las 24 horas borra una fila que sigue `in_progress` (`hermes/src/hermes/infrastructure/persistence/idempotency_repository.py`); con sus casos en `hermes/tests/integration/api/test_idempotency.py` y `hermes/tests/integration/persistence/test_idempotency_repository.py`, y la regla en research R11 y en la transición de la clave de idempotencia de data-model.md. Hoy la repetición completa la fila con `complete(caller, key, unknown, None)` y la limpieza la borra, las dos sin entrada: un mensaje que pudo salir no queda atribuido a quien lo pidió, per FR-029 / Constitución V (partial)
- [X] T123 Escribir cada entrada de auditoría en la misma transacción que el cambio que registra, como ya hace el envío: `config_applied` con la sustitución de `replace_if` (`hermes/src/hermes/application/configuration.py` y `hermes/src/hermes/infrastructure/persistence/config_repository.py`), y `verification_code_issued` y `channel_verified` con el código que guarda `issue_code` y el canal que liga `bind` (`hermes/src/hermes/application/verification.py` y `hermes/src/hermes/infrastructure/persistence/channels_repository.py`); con casos en `hermes/tests/integration/persistence/test_apply_export.py` y `hermes/tests/integration/persistence/test_channels_repository.py` en los que falla la escritura de la entrada y el cambio no queda. Hoy cada entrada va en una transacción aparte, después del cambio: un fallo entre las dos deja el cambio sin atribuir, y `hermes config apply` responde con un error tras haber aplicado, per FR-029 / FR-024 / Constitución V (partial)
- [X] T124 Comprobar en cada caso de la batería de conformidad, de C1 a C12 (`hermes/tests/contract/test_conformance_chat.py`), que ni los logs ni lo que devuelve o lanza el proveedor (el resultado, y el mensaje y los detalles del error) llevan la URL del proveedor, el token, la dirección del destinatario ni el texto que respondió el proveedor, con lo que declara cada harness, también la descripción de cada respuesta grabada de Telegram que use el caso. Hoy solo lo comprueba C10, y `specs/008-hermes-send-message/contracts/conformance.md` lo exige en todos los casos, también a los proveedores que se añadan, per FR-031 / contracts/conformance.md (partial)
- [X] T125 Exigir en `hermes/src/hermes/infrastructure/settings.py` que `HERMES_DATABASE_PASSWORD_REF` sea un secreto de Hermes (`secret://hermes/...`), con su caso en `hermes/tests/unit/test_settings.py` y la regla en `specs/008-hermes-send-message/contracts/cli.md`. Hoy acepta una referencia de cualquier identidad, y FR-023 y el ADR 0012 dicen que la contraseña de la base es un secreto de la app, per FR-023 (partial)
- [X] T126 Quitar «, salvo en la salud» de la fila `Authorization` de la tabla de cabeceras de `specs/008-hermes-send-message/contracts/api.md`: la API de red no tiene rutas de salud, como dicen el apartado «Salud» del mismo contrato y la descripción de `hermes.v1.json`, per FR-015 (contradicts)
- [X] T127 Tras T122 a T126, volver a pasar las comprobaciones de calidad y la suite completa de Hermes con `HECATE_BIN` y Docker, con el E2E, del apartado 1 de `specs/008-hermes-send-message/quickstart.md`: las pasadas de T120 y T121 son anteriores a esta fase, per T108 / T109 (partial)

## Phase 13: Convergence

- [X] T128 Dejar la configuración en marcha igual que la de la base tras aplicar: que `replace_if` devuelva la configuración leída dentro de su propia transacción (`hermes/src/hermes/application/ports.py`, `hermes/src/hermes/infrastructure/persistence/config_repository.py` y el doble de `hermes/tests/support/doubles.py`) y que `apply` la use para la instantánea y los sondeos, sin una lectura aparte (`hermes/src/hermes/application/configuration.py`); con un caso en `hermes/tests/integration/persistence/test_apply_export.py` con un repositorio cuya lectura falla después de aplicar, en el que la instantánea queda con la configuración aplicada. Hoy `apply` vuelve a leer con `load()` después de confirmar la transacción: si esa lectura falla, `hermes config apply` responde con un error tras haber aplicado, la instantánea y los sondeos siguen con la configuración de antes, y aplicar otra vez el mismo fichero no lo arregla, porque un plan sin cambios vuelve antes de tocar la instantánea, per FR-024 / research R7 / spec: aplicación durante un envío (partial)
- [X] T129 Llevar al apartado de la auditoría de `docs/hermes.md` la regla que research R14 recoge desde T123: cada entrada va en la misma transacción que el cambio que registra, y si no se puede escribir, el cambio no queda. Hoy el documento solo lo dice de la clave de idempotencia, per FR-032 / Constitución (Consolidación) (partial)
- [X] T130 Validar contra el sobre común de `common.v1.json` cada respuesta del canal local que reciben las pruebas de `hermes/tests/integration/admin/`, con una comprobación común en `hermes/tests/integration/admin/conftest.py` alrededor del transporte de httpx por el socket Unix, como la de la API (T121). Hoy solo validan el sobre `test_admin_skeleton.py`, una prueba de `test_config_cli.py` y una de `test_health.py`, y las respuestas de `identity.enroll` solo se comprueban a través de la CLI, per SC-009 / Convenciones: contrato (partial)
- [X] T131 Tras T128 a T130, volver a pasar las comprobaciones de calidad y la suite completa de Hermes con `HECATE_BIN` y Docker, con el E2E, del apartado 1 de `specs/008-hermes-send-message/quickstart.md`: la pasada de T127 es anterior a esta fase, per T108 / T109 (partial)

## Phase 14: Convergence

- [X] T132 Dejar una línea de log por cada petición de la API de red al terminar, como fija research R19: con el `request_id`, la operación, la identidad que llama si se autenticó, la `Idempotency-Key` si la trae, el resultado (`completed` o el código del error), el estado HTTP y `duration_ms`, sin contactos, direcciones, tokens ni certificados. Va en `hermes/src/hermes/infrastructure/api/middleware.py`, solo en la app de `create_network_api` (`hermes/src/hermes/infrastructure/api/app.py`), porque el canal local ya deja su línea por operación, con la identidad que guarde la dependencia de `hermes/src/hermes/infrastructure/api/auth.py`; con casos en `hermes/tests/integration/api/` para un envío completado, uno que falla en el proveedor, uno con `validation` y una llamada rechazada con `auth`, y la regla en el apartado de los logs de `docs/hermes.md`. Hoy la API no deja ninguna línea por petición: un envío completado no aparece en los logs, `auth.refused` va sin `request_id` y un `internal` por un secreto pasado su tiempo duro no deja rastro, así que el `request_id` que recibe quien llama no lleva a nada, per research R19 / Constitución (Operación) (partial)
- [X] T133 Alinear con el orden real lo que dicen de la base caída `specs/008-hermes-send-message/contracts/api.md` («Orden y errores») y `docs/hermes.md` (el orden de las comprobaciones de `chat.send_message`): sin estar listo, `unavailable` antes de todo; con la base caída, la autenticación comprueba el certificado y el token, y como no puede guardar el `jti`, una llamada válida responde `unavailable` sin llegar al proveedor, y una con credenciales que no valen, `auth`. Con un caso en `hermes/tests/unit/application/test_authentication.py` con un repositorio de `jti` que no responde. Hoy los dos documentos dicen que con la base caída `unavailable` va antes de todo lo demás, pero la dependencia de `hermes/src/hermes/infrastructure/api/auth.py` solo mira si Hermes está listo, per contracts/api.md / docs/hermes.md (contradicts)
- [X] T134 Tras T132 y T133, volver a pasar las comprobaciones de calidad y la suite completa de Hermes con `HECATE_BIN` y Docker, con el E2E, del apartado 1 de `specs/008-hermes-send-message/quickstart.md`: la pasada de T131 es anterior a esta fase, per T108 / T109 (partial)
