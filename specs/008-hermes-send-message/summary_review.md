# Guía de revisión: base de Hermes y envío de mensajes por Telegram

Rama `feature/008-hermes-send-message`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene
saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Seis historias (enviar a un contacto, solo quien está autenticado, identidad y
   secretos con Hécate, configurar y verificar un canal, repetir sin duplicar y reintentar solo lo seguro, capacidades
   y salud), 32 requisitos y 10 criterios de éxito. Las aclaraciones fijan el alcance:
   - Telegram como primer proveedor de chat;
   - el acceso por app lo guarda Hécate y el token corto no concede nada; la 008 autentica y la lista de Hécate es de
     la 009;
   - la configuración (instancias, plantillas, variables y contactos) vive en la base de Hermes, en el PostgreSQL común,
     y la aplica el admin con la CLI por un canal local; los secretos siguen en Hécate;
   - un solo contacto por envío, sin grupos;
   - una `Idempotency-Key` repetida con otro cuerpo es `conflict`;
   - la salud va por el canal local, fuera de la API.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md),
   [contratos](contracts/README.md) y los ADR [0011](../../docs/adr/0011-pila-hermes.md),
   [0012](../../docs/adr/0012-persistencia-postgresql-comun.md) y [0013](../../docs/adr/0013-acceso-por-app.md)):
   - el proceso y el orden del arranque (R1), Envoy delante con el certificado del cliente en una cabecera (R2);
   - la autenticación de cada llamada, con la clave de un `kid` nuevo pedida a Hécate (R3), la identidad con Hécate (R4)
     y la caché de secretos (R5);
   - la persistencia (R6), el fichero de configuración (R7), las plantillas (R8) y Telegram (R9);
   - los reintentos (R10), la idempotencia (R11), la verificación (R12) y los límites de uso (R13);
   - el canal local y la CLI (R15), el fake y la batería de conformidad (R17), las pruebas (R18) y el entorno de pruebas
     (R21).
3. **El código nuevo**, en `hermes/src/hermes/` y en este orden:
   - `domain/`: el sobre y los códigos (`shared/`), la configuración con su validación y su plan (`configuration/`), el
     mensaje neutral (`messages/`), el contrato de `chat` y el port de sus librerías (`services/chat/`), la huella y la
     clave de idempotencia (`idempotency/`), el estado de una instancia (`instances/status.py`) y el código de
     verificación (`verification/code.py`).
   - `application/`: `send_message.py` (el caso de uso, con sus dos puntos de traza), `idempotency.py`,
     `rate_limits.py`, `configuration.py`, `verification.py`, `identity.py`, `secrets.py`, `authentication.py`,
     `capabilities.py`, `health.py` y `startup.py`. Los más importantes son `authentication.py`, `idempotency.py` y
     `send_message.py`.
   - `infrastructure/api/`: el middleware y el sobre, la cabecera de Envoy (`xfcc.py`), la dependencia de autenticación
     y las rutas (`routes_chat.py` y `routes_capabilities.py`).
   - `infrastructure/admin/`: el canal local (rutas de alta, configuración, códigos y salud) y la CLI (`cli.py`).
   - `infrastructure/persistence/`: el esquema, la migración `0001`, el motor y los repositorios;
     `infrastructure/config_file/`: el YAML estricto y la exportación.
   - `infrastructure/hecate/`: el TLS fijado a la raíz con la comprobación del SPIFFE ID, el cliente y el almacén de la
     identidad.
   - `infrastructure/providers/`: Telegram (adapter, dibujo, errores y polling), el fake, los reintentos
     (`retry.py`) y el registro; `infrastructure/main.py`: el cableado de `hermes serve`.
   - `hermes/Dockerfile` y `hermes/deploy/`: la imagen, el compose de pruebas con su fichero para el E2E y Envoy.
4. **Lo que toca fuera de Hermes:**
   - `common.v1.json` pasa a 1.1.0: `meta` gana `service`, `provider` e `idempotency_key`, opcionales. La prueba de
     contrato de Hécate (`hecate/crates/infrastructure/tests/contract_schema.rs`) espera la 1.1.0, y su suite pasa
     entera en macOS y en Linux; no cambia nada más de Hécate.
   - `hermes.v1.json` es nuevo, con su fila en `shared/contracts/README.md`.
   - `uv.lock` y `hermes/pyproject.toml`, por las dependencias del ADR 0011; un `.dockerignore` en la raíz para las
     imágenes de pruebas.
5. **Los tests:** 835 en Hermes. Lo más crítico:
   - `tests/integration/api/test_idempotency.py`: SC-003, 100 peticiones iguales a la vez, y con un corte entre el envío
     y su registro, dan un solo mensaje.
   - `tests/integration/test_no_leaks.py`: SC-007 y FR-023, tokens, códigos, direcciones y certificados, en claro y
     codificados, en logs, respuestas, auditoría, lo exportado y todas las tablas; con su propia prueba de que la
     búsqueda encuentra lo que busca.
   - `tests/e2e/test_flow.py`: el flujo completo en contenedores, con Envoy, una Hécate de pruebas, PostgreSQL y el fake;
     las credenciales inválidas de la historia 2 que el entorno puede producir (SC-002), también un certificado revocado
     con urgencia una vez que Hermes refresca su lista de revocación, y PostgreSQL parado. Tarda unos 5 minutos por esa
     espera.
   - `tests/contract/test_conformance_chat.py`: la batería C1 a C12, con el fake y con Telegram grabado.
   - `tests/unit/application/test_authentication.py`, `test_verified_only.py` (una máquina de estados de hypothesis
     para SC-008), `test_rate_limits.py` y `tests/integration/telegram/test_retries.py`.
   - `tests/integration/conftest.py`: cada respuesta de la API que reciben las pruebas de integración se valida contra el
     OpenAPI (SC-009), con una prueba de que la comprobación ve una respuesta fuera del contrato; el E2E valida cada una
     que recibe por Envoy, y `tests/integration/admin/conftest.py`, cada respuesta del canal local contra el sobre
     común, por el transporte de la CLI y por el asíncrono.

## Cómo probarlo

```bash
cargo build --release --manifest-path hecate/Cargo.toml
```

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

Las comprobaciones de calidad y la pasada de Hécate en Linux son las del apartado 1 de [quickstart.md](quickstart.md).
El E2E y el quickstart usan el compose de `hermes/deploy/`; en este Mac, con Colima, va con `docker-compose`, porque el
plugin `docker compose` no está instalado. El recorrido con Telegram real son los apartados 2 a 8 del quickstart.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, tareas, checklists, contratos, quickstart y los ADR 0011, 0012 y 0013;
  - el código, las pruebas, la imagen y el compose de pruebas;
  - los OpenAPI en `shared/contracts/src/cerbero_contracts/openapi/` y la fila de `hermes.v1.json`;
  - los docs consolidados: `docs/hermes.md`, `ARCHITECTURE.md`, `GLOSSARY.md`, `DECISIONS.md`, `README.md`, el índice
    de ADR, y `docs/tenaro.md` y `docs/hades.md`, que contradecían lo nuevo.
- **Resultados:** 835 pruebas de Hermes con Docker y `HECATE_BIN`, el E2E incluido; ruff, el formato, mypy estricto y
  pip-audit sin avisos; las 841 de Hécate en macOS y en un contenedor Linux (`rust:1.99.1-bookworm`). En el recorrido
  con tu bot real todo dio lo esperado: verificación, envío, repetición sin duplicar, `conflict` con otro cuerpo, el
  bench de SC-001 (20 de 20, p95 de 0,23 s, sin reintentos), `getMe`, los rechazos de `auth`, el envío con Hécate
  sellada y PostgreSQL parado; llegaron los 22 mensajes previstos.
- **Tus decisiones durante la implementación:**
  - un `kid` que no está en la lista se pide a Hécate a demanda (T113);
  - toda línea de log que deja una petición lleva su `request_id`, no solo la línea `api.request` (T132);
  - la documentación de Hermes se consolida en `docs/hermes.md` con su plantilla, y la reestructuración en un doc de
    alto nivel con `docs/hermes/` va en un cambio propio;
  - que Ténaro acabe gestionando la configuración de Hermes queda como idea en «Abiertas» de DECISIONS.
- **Decisiones mías durante la implementación:**
  - Hermes abre él mismo sus dos sockets, porque uvicorn los dejaría en 0666, y la base arranca en segundo plano tras
    abrirlos, así que la salud responde desde el principio;
  - un fallo de conexión con la base es `unavailable`; `Retry-After` solo va con estados distintos de 200;
  - las respuestas grabadas de la Bot API están escritas a mano con su formato documentado, y Telegram real lo comprobó
    T110;
  - la caché de secretos solo usa el último valor ante `unavailable` o `internal`, y los tokens de los bots siempre
    salen de ella;
  - el canal local acepta cuerpos de hasta 8 MiB, porque lleva el fichero entero (de 1 MiB como mucho) dentro de un
    JSON, y su contrato lo dice;
  - lo exportado ordena los canales de cada contacto por servicio, proveedor e instancia;
  - los servicios se instalan tras las migraciones, así que una configuración guardada que no carga se corrige con
    `config apply` mientras el arranque espera;
  - el puerto de idempotencia que dejó la fase 2 se sustituyó por uno que completa la clave y la auditoría en una sola
    transacción; una excepción inesperada del proveedor queda como `internal` en su clave;
  - los límites de uso se calculan con aritmética exacta, porque con coma flotante una ficha se quedaba en 0,9999;
  - el estado de una instancia se lee al pie de la letra: una prueba fallida o unas credenciales rechazadas la dejan
    `unavailable` hasta que una prueba funcione, y una prueba que funciona no borra las llamadas fallidas;
  - la prueba de las instancias al arrancar nunca para el arranque, y las instancias que añade un `config apply` no se
    prueban hasta su primera llamada o prueba;
  - la línea de log de `health` va a nivel `debug` cuando está listo, porque la pide el healthcheck del contenedor;
  - la batería gana C9, que ninguna tarea nombraba y fija el contrato de conformidad;
  - el E2E usa un `compose.test.yaml` con el fake y otros puertos en el anfitrión, para no chocar con un quickstart en
    marcha.
- **Lo que encontró el E2E (T113):** un componente recién dado de alta o renovado recibía `auth` (`unknown_key`)
  durante hasta 4 minutos, porque Hermes solo miraba la lista de claves que refresca cada 240 s; con cada renovación de
  Hades habría pasado lo mismo. Ahora pide la clave a Hécate, solo con la cadena ya validada y un `kid` con forma de
  huella, y la guarda hasta el siguiente refresco; si Hécate no responde, `unavailable`. Están al día research R3, el
  contrato de la API y las descripciones de los 503 del OpenAPI.
- **Lo que encontraron las fases:** la regla del fake se comprobaba después de buscar el manifiesto, así que en
  producción, donde el registro no carga el fake, salía el mensaje genérico (fase 6); y el mensaje del límite de cuerpo
  decía 64 KiB también en el canal local (fase 6).
- **Detalles cosméticos que dejó el recorrido**, sin efecto en el comportamiento: lo exportado reordena las claves
  dentro de `properties` de un esquema, y una respuesta repetida sale con las claves del JSON en otro orden, porque
  PostgreSQL guarda el JSONB en su propio orden. El prefijo «Value error, » de los errores del fichero lo quitó T117.
- **Fuera de alcance**, como fija la spec:
  - las tasks y `chat.ask`, los grupos de contactos y otros proveedores y tipos de servicio;
  - la lista de acceso por app de Hécate (la 009) y la gestión de Hermes desde Ténaro;
  - los permisos que salen de las declaraciones, `wrap` / `unwrap` y las copias de seguridad;
  - OpenTelemetry y el despliegue en el home lab: el compose es solo el entorno de pruebas.

## Dónde mirar con más sospecha

- **La autenticación (`application/authentication.py`, `infrastructure/api/xfcc.py` y `deploy/envoy/`):**
  - Envoy pide el certificado sin rechazar ninguno y sustituye la cabecera que traiga la petición; Hermes valida la
    cadena con la raíz, el único SPIFFE ID, la CRL de la intermedia y el token;
  - el `kid` se pide a Hécate solo después de la cadena y con la forma de una huella, porque va en la ruta de una
    petición que Hermes hace con su propia identidad (`IdentityService.key`, una petición por `kid` a la vez);
  - cualquier rechazo tiene el mismo mensaje, y el motivo solo va al log.
- **La idempotencia (`application/idempotency.py`, `send_message.py`, `routes_chat.py` e
  `idempotency_repository.py`):**
  - el cerrojo por llamante y clave, y que se libere también cuando la petición falla;
  - la clave se reserva justo antes del proveedor y se completa con la entrada de auditoría en la misma transacción;
  - la reserva guarda la entrada del envío con resultado desconocido, y la escriben la repetición o la limpieza de las
    24 horas si un corte se lleva el registro (`complete_unknown` y `purge` del repositorio);
  - la repetición devuelve el sobre guardado con el `request_id` y la duración de la petición nueva.
- **Los reintentos (`providers/retry.py` y `_send_once` del adapter de Telegram):** solo `ConnectError`,
  `ConnectTimeout`, `PoolTimeout` y el 429 son «no salió»; cualquier otro error de httpx o un 5xx es resultado
  desconocido, y cada intento solo tiene el tiempo que queda.
- **Aplicar la configuración (`application/configuration.py`, `config_repository.py`, `domain/configuration/diff.py`
  y `config_file/loader.py`):** el bloqueo consultivo con la huella de lo que había, los canales que conservan su
  verificación, las versiones de plantilla que no cambian, el YAML sin alias, anclas, claves repetidas ni fechas, y la
  entrada `config_applied` en la misma transacción que la configuración, como las de los códigos y los canales
  verificados.
- **La verificación (`application/verification.py`):** solo chats privados, el canal se liga con un `UPDATE`
  condicional, y un código desconocido, caducado o usado no recibe respuesta.
- **El arranque y la salud (`infrastructure/main.py`):** los servicios tras las migraciones, la validación de la
  configuración guardada contra las librerías cargadas, y el `SELECT 1` con 2 s de límite de la salud.
- **Los logs de una petición (`infrastructure/logging.py` y `infrastructure/api/middleware.py`):** el middleware guarda
  el `request_id` en un contextvar y cada registro lo toma al crearse, también en la aplicación; la línea `api.request`
  solo la deja la API de red, con la operación que apunta la dependencia `recorded_operation` antes de autenticar; y
  los sondeos que arranca un `config apply` empiezan fuera de la petición, para no llevar su `request_id` para siempre.

## Después de la revisión

El primer `/speckit-converge` no encontró nada sin construir. Añadió la fase 10 (T114 a T120), que ya está hecha:

- el acceso por app en el diseño de la Access Policy de Hécate, como pide FR-032 (`docs/hecate/acceso.md` y
  `docs/hecate.md`);
- el E2E prueba ahora un token de más de 300 s y un certificado revocado con urgencia, y el quickstart dice que el
  caducado lo prueban las pruebas de la API;
- la validación exige que cada referencia de secreto de una instancia sea `secret://hermes/<proveedor>/<instancia>/<campo>`
  (FR-019);
- los errores del fichero ya no llevan el prefijo de Pydantic;
- el contrato del canal local anota el nivel del log de `health`, y el plan ya no dice que la salud está en el OpenAPI;
- la calidad y la suite completa, con el E2E, vuelven a pasar: 803 pruebas.

El segundo añadió la fase 11 (T121), ya hecha: la comprobación de contrato pasó a `tests/integration/conftest.py` y
cubre también la prueba de fugas, y el E2E valida cada respuesta que recibe por Envoy. La suite completa vuelve a pasar,
con las mismas 803 pruebas.

El tercero añadió la fase 12 (T122 a T127), ya hecha:

- un envío que un corte deja a medias queda en la auditoría: la reserva de su clave guarda su entrada con `timeout` y
  `outcome_unknown`, y la escriben la repetición o la limpieza de las 24 horas, con la hora del intento (FR-029);
- las entradas de aplicar la configuración, pedir un código y verificar un canal van en la misma transacción que su
  cambio: si no se pueden escribir, el cambio no queda;
- la batería de conformidad busca fugas en todos sus casos, en los logs con sus campos y en lo que devuelve o lanza el
  proveedor;
- `HERMES_DATABASE_PASSWORD_REF` solo acepta un secreto de Hermes, y el contrato de la API ya no dice que la salud va
  sin `Authorization`;
- la calidad y la suite completa, con el E2E, vuelven a pasar: 814 pruebas.

El cuarto añadió la fase 13 (T128 a T131), ya hecha:

- la aplicación de la configuración lee lo que queda dentro de su propia transacción, así que, una vez confirmada, la
  instantánea y los sondeos la toman sin otra lectura que pueda fallar entre medias;
- `docs/hermes.md` dice que cada entrada de auditoría va en la misma transacción que su cambio;
- `tests/integration/admin/conftest.py` valida contra el sobre común cada respuesta del canal local;
- la calidad y la suite completa, con el E2E, vuelven a pasar: 816 pruebas.

El quinto añadió la fase 14 (T132 a T134), ya hecha:

- cada petición a la API deja al terminar una línea `api.request` con su `request_id`, la operación, quien llama, la
  clave, el resultado, el estado HTTP y la duración, y toda línea que deja una petición lleva su `request_id`, también
  la del motivo de un `auth` (research R19);
- el contrato de la API y `docs/hermes.md` dicen ya que, con la base caída, Hermes comprueba antes el certificado y el
  token: una llamada válida responde `unavailable` y una que no se autentica, `auth`;
- la calidad y la suite completa, con el E2E, vuelven a pasar: 835 pruebas.

Con T112, el mensaje del commit de squash y la descripción de la PR, están hechas las 134 tareas.
