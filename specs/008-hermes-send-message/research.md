# Research: Base de Hermes y envío de mensajes por Telegram

Decisiones técnicas de la feature 008. Cada una dice qué se elige, por qué y qué se descartó. Parte del diseño de
[docs/hermes.md](../../docs/hermes.md) y de la Hécate que dejan las features 001 a 006: sellado, secretos, identidades,
Access Policy y resolución por su API, según su [guía de integración](../../docs/hecate/integracion.md). La pila queda
en el [ADR 0011](../../docs/adr/0011-pila-hermes.md) y la persistencia en el [ADR
0012](../../docs/adr/0012-persistencia-postgresql-comun.md), los dos escritos en este plan.

## R0. Decisiones del propietario

- **Specify (2026-11-13):** FastAPI detrás de un proxy TLS que termina el mTLS y reenvía el certificado del cliente en
  una cabecera. Condiciones: Hermes no publica su puerto; el token corto se liga a la identidad que declara el proxy,
  como riesgo aceptado; la decisión va con la del despliegue en el home lab. Propuestas aceptadas: el proxy sustituye la
  cabecera que mande el cliente, recarga sin reiniciarse el certificado que Hermes renueva, y Hermes sigue haciendo las
  comprobaciones de la guía de integración.
- **Clarify (2026-11-14):**
  - Telegram como primer proveedor de chat;
  - el acceso por app lo guarda Hécate y llega con la 009; Hermes solo autentica;
  - la configuración y lo que Hermes escribe al funcionar van en su base de PostgreSQL, y los secretos siguen en Hécate;
  - la CLI aplica un fichero entero, lo exporta y da los códigos de verificación;
  - un contacto por envío, sin grupos.
- **Plan (2026-11-14), las siete recomendaciones:** Envoy (R2); un socket Unix entre Envoy y Hermes (R2); SQLAlchemy
  2.1 Core asíncrono sobre psycopg 3, con Alembic (R6); YAML con validación estricta (R7); plantillas con sustitución
  propia (R8); httpx contra la Bot API (R9); el binario real de Hécate en las pruebas de su adaptador y en el E2E (R18).
- **Plan, propuestas que el propietario aceptó con ellas:** los ADR 0011 y 0012 en el plan; rutas con la versión
  mayor (R16); el claim `cerbero_audit` (R3); el canal local como HTTP sobre un socket Unix (R15); ruff, mypy estricto y
  pip-audit (R18).

## R1. Proceso y arranque

**Decisión:** `hermes serve` ejecuta un solo proceso asyncio con tres partes:

- la API, una app FastAPI servida por uvicorn en el socket Unix `HERMES_RUN_DIR/api.sock`, sin ningún puerto TCP;
- el canal local de administración, otra app FastAPI servida por un segundo `uvicorn.Server` en
  `HERMES_ADMIN_DIR/admin.sock` (R15);
- las tareas de fondo: el supervisor de la identidad (renovación y publicaciones de Hécate, R4), un sondeo de Telegram
  por instancia (R12) y la limpieza de las claves de idempotencia y de los `jti` caducados.

Al arrancar:

1. abre los dos sockets, con `api.sock` en 0600 y el mismo usuario que Envoy, así que la salud responde desde el
   principio por el canal local;
2. carga su identidad del directorio de estado; sin identidad, o con una que ya no vale, no está listo y espera a
   `hermes enroll` (R4);
3. con identidad, refresca la lista de revocación y las claves publicadas, resuelve la contraseña de la base, se
   conecta, aplica las migraciones pendientes (R6), carga la configuración en memoria y prueba la conexión de cada
   instancia;
4. arranca los sondeos y pasa a estar listo.

- uvicorn va con `proxy_headers` desactivado: la única cabecera del proxy que cuenta es la del certificado (R3).
- Una sola instancia (spec, Assumptions): los límites de uso y el bloqueo de cada clave de idempotencia viven en memoria
  (R11, R13).

**Alternativas:** un proceso por papel (API, administración y sondeo), con más piezas que coordinar sin ganar nada con
una sola instancia; Gunicorn con varios workers, que rompería los bloqueos en memoria.

## R2. Proxy: Envoy, y su enlace con Hermes

**Decisión:** Envoy delante de Hermes, en el mismo equipo y con el mismo usuario. Lo que hace está en
[contracts/proxy.md](contracts/proxy.md):

- **TLS 1.3 con el certificado de Hermes**, que lee por SDS de ficheros con `watched_directory`. Hermes deja cada
  certificado en un directorio nuevo y cambia de forma atómica el enlace `current`, y Envoy lo recarga sin reiniciarse
  ni cortar las conexiones en curso (FR-002).
- **Pide certificado de cliente sin exigirlo** (`require_client_certificate: false`), anunciando la CA raíz de Cerbero,
  y **acepta los que no verifica** (`trust_chain_verification: ACCEPT_UNTRUSTED`). El saludo TLS prueba que el cliente
  tiene la clave; Hermes valida todo lo demás y responde `auth` en el sobre (FR-003, historia 2).
- **Sin reanudar sesiones TLS** (`disable_stateless_session_resumption` y `disable_stateful_session_resumption`), para
  que cada conexión presente su certificado y la cabecera vaya siempre completa.
- **`forward_client_cert_details: SANITIZE_SET`**, con `cert`, `chain` y `uri` (Envoy añade siempre `Hash` y `By`):
  sustituye cualquier `X-Forwarded-Client-Cert` que traiga el cliente por la del certificado de la conexión, con la
  cadena que mandó el cliente (FR-002).
- **Reenvía a Hermes por el socket Unix** `api.sock` (dirección `pipe`): Hermes no abre ningún puerto TCP, y solo le
  llega quien puede abrir ese fichero (FR-001, historia 2).

Envoy solo monta el directorio del proxy (`HERMES_PROXY_DIR`), con el certificado, su cadena, su clave TLS y la CA raíz.
No ve el directorio de estado, donde está la clave de firma de llamadas, ni el socket de administración. Como presenta
el certificado de Hermes, lee su clave TLS: es parte de la frontera de Hermes, y la clave no sale de su equipo (FR-005).

**Alternativas:**

- Traefik: su documentación no dice si `passTLSClientCert` pasa la cadena ni si quita la cabecera que mande el cliente,
  y sin la cadena Hermes no puede verificar un certificado firmado por una intermedia que aún no conoce;
- Caddy: sus marcadores dan el certificado hoja, sin la cadena;
- uvicorn adaptado o aiohttp: descartados por el propietario (dependen de detalles internos de uvicorn que cambian entre
  versiones, o sacan a Hermes de FastAPI);
- una red interna de docker-compose o mTLS entre el proxy y Hermes: un puerto TCP sin publicar, o un par de certificados
  más que gestionar, frente a un fichero que solo abre el proxy.

## R3. Autenticación de las llamadas

**Decisión:** una dependencia de FastAPI en todas las rutas de la API, que hace por orden:

1. **Cabecera del proxy:** exactamente un elemento con `Hash`, `Cert`, `Chain` y `URI`, en el formato de texto de Envoy
   (valores con codificación URL). Sin cabecera, o mal formada: `auth`.
2. **Cadena:** `cryptography.x509.verification`, con
   `PolicyBuilder().store(Store([raíz])).time(<reloj>).build_client_verifier()` y las intermedias que trae `Chain`.
   Exige el uso de cliente y devuelve los nombres. El certificado de `Cert` tiene que tener un único nombre, un SPIFFE
   ID del trust domain de Hermes, y su serie no puede estar en la CRL de la intermedia que lo emitió.
3. **Token:** PyJWT con `algorithms=["EdDSA"]` y la clave publicada cuyo `kid` trae la cabecera del token, que tiene que
   ser de la identidad del certificado (`cerbero_identity`) y estar activa. Si el `kid` no está en la última lista,
   porque el componente se acaba de dar de alta o de renovar, Hermes pide esa clave a Hécate (`GET /v1/keys/{kid}`),
   solo con la cadena ya validada y un `kid` con la forma de una huella RFC 7638, con una sola petición por `kid` a la
   vez, y la guarda hasta el siguiente refresco de la lista. Si Hécate no la conoce, `auth`; si no responde,
   `unavailable`. Los tiempos y los claims los comprueba código propio con el reloj inyectado: `iss` igual al SPIFFE ID
   del certificado, `aud` igual al de Hermes, `exp - iat` de 300 s como mucho, con 30 s de margen entre relojes, y `jti`
   de 1 a 128 caracteres ASCII imprimibles.
4. **`jti` de un solo uso:** una fila `(iss, jti)` en la base hasta `exp` más 30 s, así que una repetición se rechaza
   también después de reiniciar.

- Cualquier fallo responde `auth` con el mismo mensaje. El log dice el motivo, nunca el token ni el certificado.
- **Contexto de auditoría:** el claim opcional `cerbero_audit`, un objeto de hasta 16 claves de texto (de 1 a 64
  caracteres) con valores de texto (hasta 256), 2 KiB como mucho. Se guarda tal cual en la auditoría (FR-029); con otra
  forma, la petición responde `validation`.
- Hermes no toma ningún permiso de lo que diga el token (FR-004).

**Alternativas:** joserfc o authlib (equivalentes; PyJWT es la más adoptada y verifica EdDSA con cryptography); dejar la
cadena al proxy (no habría `auth` en el sobre ni se comprobaría la CRL de Hécate); los `jti` en memoria (se aceptaría
una repetición tras reiniciar, en contra de FR-003). Para un `kid` que no está en la lista: refrescar la lista entera
(un componente nuevo seguiría esperando) o esperar al refresco (cada renovación cortaría sus llamadas hasta 4 minutos).
Lo halló el E2E y lo decidió el propietario.

## R4. Identidad con Hécate

**Decisión:** un adaptador `hecate` con un cliente httpx y un almacén de identidad en ficheros.

- **El token de alta llega por la CLI** (`hermes enroll`, R15), que lo pide sin eco o lo lee de la entrada estándar. No
  va en una variable de entorno, porque la constitución lo prohíbe para los secretos, ni en un argumento, que se vería
  en la lista de procesos. Hasta que llega, Hermes no está listo y dice que necesita un token de alta.
- **Comprobar a Hécate antes de enviar nada (FR-005):** una primera conexión TLS 1.3 sin certificado de cliente toma la
  cadena que presenta Hécate (`get_unverified_chain()`, de Python 3.13 en adelante). La CA raíz tiene que tener la
  huella del token, el certificado de Hécate tiene que verificar contra ella con `build_client_verifier` (los
  certificados de Cerbero valen como servidor y como cliente) y su único nombre tiene que ser `spiffe://<td>/hecate`.
  Solo entonces se envía el alta.
- **Cada llamada a Hécate (FR-007):** httpx con un `SSLContext` de TLS 1.3 que solo confía en la CA raíz guardada y no
  compara nombres DNS, porque los certificados de Cerbero solo llevan un SPIFFE ID. Un `network_backend` propio de
  httpcore exige, tras el saludo y antes de enviar la petición, que el certificado de Hécate tenga ese SPIFFE ID y no
  esté en la CRL. Cada llamada lleva el certificado de cliente y un token corto nuevo.
- **Claves:** P-256 para la TLS y Ed25519 para la firma de llamadas, generadas con cryptography en el equipo de Hermes;
  la solicitud va sin nombre, porque la identidad la decide el token.
- **Almacén:**
  - el directorio de estado (`HERMES_STATE_DIR`, 0700, ficheros 0600) guarda la clave de firma de llamadas, los
    metadatos (SPIFFE ID, serie, `not_after`, `call_key_id` y huella de la raíz), la última lista de revocación, las
    últimas claves publicadas y, durante una renovación, la petición en curso;
  - el directorio del proxy (`HERMES_PROXY_DIR`) guarda el certificado, su cadena, su clave TLS y la CA raíz, en
    `gen-<n>/` con el enlace `current` (R2).
- **Renovación (FR-006):** a los dos tercios de la vida del certificado, una vez por certificado. Las claves nuevas, la
  solicitud y la `Idempotency-Key` se guardan antes de enviar. Si no llega la respuesta, repite la misma petición con un
  token nuevo; con `unavailable` o sin conexión, conserva el certificado y reintenta cada 60 s. Con el certificado
  nuevo, escribe la generación nueva, cambia el enlace y pasa a firmar con la clave nueva.
- **Publicaciones (FR-008):** la lista de revocación y las claves se refrescan cada 240 s, porque valen 300 s. Si Hécate
  no responde, Hermes sigue con la última copia, que también guarda en disco para arrancar con ella.
- **Sin certificado válido (FR-010):** si caduca, si aparece en la lista de revocación, si Hécate rechaza la renovación
  con `auth` o si Hécate presenta otra CA raíz, Hermes deja de estar listo y dice que necesita un token de alta nuevo.

**Alternativas:** el cliente de referencia de Hécate (está en Rust y es de sus pruebas); `ssl` y `http.client` a mano
(se pierde el cliente asíncrono y su pool); confiar en la CA raíz sin comprobar el SPIFFE ID (cualquier componente con
un certificado de Cerbero podría hacerse pasar por Hécate y dar secretos falsos).

## R5. Secretos y su caché

**Decisión:** un servicio de aplicación, `SecretCache`, con el reloj inyectado. Resuelve en Hécate todo o nada y guarda
cada valor en memoria con los tiempos que da Hécate: 300 s el blando y 3600 s el duro. Pasado el blando, refresca en la
siguiente petición; si Hécate no responde o está sellada, sigue con el valor hasta el duro, y después lo que lo necesita
falla con `internal` sin llegar al proveedor (FR-009).

- **Las credenciales de la base son un secreto más:** `HERMES_DATABASE_PASSWORD_REF`, por defecto
  `secret://hermes/postgres/password`. El pool pide la contraseña vigente a la caché cada vez que abre una conexión.
- **Memoria de Python:** los valores son `str`, que Python no permite borrar de la memoria. Se aceptó al elegir Python
  para Hermes ([ADR 0005](../../docs/adr/0005-rust-para-orthros-y-hecate.md)) y pasa a los riesgos aceptados al
  consolidar (R20).
- Nunca en disco, logs, eventos ni respuestas: cada valor va en un tipo cuyo `repr` lo oculta.

## R6. Persistencia

**Decisión ([ADR 0012](../../docs/adr/0012-persistencia-postgresql-comun.md)):** el servidor PostgreSQL común, con la
base `hermes` y el usuario `hermes`, sin acceso a las demás bases.

- **Acceso:** SQLAlchemy 2.1 Core en modo asíncrono sobre psycopg 3.3 (`postgresql+psycopg`), sin ORM. Las tablas se
  declaran en un `MetaData` y las consultas viven en los adaptadores.
- **Migraciones:** Alembic 1.20, escritas a mano. `hermes serve` las aplica al arrancar, con un bloqueo consultivo de
  PostgreSQL.
- **Configuración en memoria:** tras arrancar y tras cada aplicación, Hermes carga una instantánea inmutable de la
  configuración y la sustituye de una vez; la de una aplicación es la que leyó su propia transacción (R7). Cada envío usa la que había al empezar: la de antes o la de después, nunca
  una mezcla (edge case).
- **Auditoría append-only:** un disparador rechaza `UPDATE` y `DELETE` en su tabla (FR-029).
- **Base caída:** Hermes no está listo y responde `unavailable`, que vale reintentar, sin llegar al proveedor (edge
  case).
- Las tablas están en [data-model.md](data-model.md).

**Alternativas:** psycopg 3 con SQL a mano y migraciones propias (descartada por el propietario); el ORM de SQLAlchemy,
un modelo de objetos que el dominio no necesita; asyncpg, solo asíncrono y sin el pool ni el ecosistema de psycopg.

## R7. Fichero de configuración

**Decisión:** YAML leído con PyYAML 6 en modo seguro, con un cargador propio que rechaza las claves repetidas, los alias
y las anclas, y ficheros de más de 1 MiB; después, modelos estrictos de Pydantic 2, sin claves desconocidas ni
conversiones implícitas. El formato está en [contracts/config-file.md](contracts/config-file.md).

- **Validación completa (FR-024):**
  - el esquema y los nombres;
  - que cada `${...}` de una plantilla apunte a `data.*` o a una variable que existe;
  - que el esquema de datos de cada versión sea un JSON Schema válido;
  - que cada canal use una instancia que existe;
  - que cada campo de secreto sea una referencia `secret://` (FR-025) y que el manifiesto del proveedor tenga todos los
    suyos;
  - que cada referencia nueva o cambiada se resuelva en Hécate; si Hécate no responde, no se aplica nada.
- **Versiones de plantilla inmutables:** cambiar el contenido de una versión que ya existe se rechaza; para cambiarla se
  añade otra. Quitar una versión la borra.
- **Plan y aplicación:** el canal local calcula lo que se crea, cambia y borra, con una huella de esa diferencia. La CLI
  lo enseña y pide confirmación, y la aplicación se envía con esa huella: si la base cambió entre medias, responde
  `conflict` y no aplica nada. Todo va en una transacción (FR-024), que también lee la configuración que queda: es la
  que pasa a usarse, sin otra lectura después de confirmar.
- **Exportar:** el mismo formato, ordenado, con las referencias de los secretos y sin direcciones ni estado de
  verificación. Aplicar lo exportado no cambia nada (FR-025).

**Alternativas:** TOML (descartada por el propietario); ruamel.yaml, que conserva los comentarios, algo que no hace
falta porque lo exportado sale de la base; JSON, sin texto de varias líneas.

## R8. Plantillas y mensaje neutral

**Decisión:** cada versión de plantilla tiene un JSON Schema de sus datos (Draft 2020-12, validado con jsonschema 4) y
un texto por campo del mensaje neutral (FR-018): `title`, `severity`, `summary`, `facts` (pares de `label` y `value`),
`actions_done`, `expected` y `link`. Los textos sustituyen `${data.<ruta>}` y `${vars.<nombre>}`, sin lógica, con la
misma sintaxis que las expresiones de los Playbooks.

- `severity` tiene que ser `info`, `low`, `medium`, `high` o `critical`, y `link`, una URL `https`. Si un campo no
  cumple tras sustituir, el envío responde `validation`.
- Un dato que falta en la ruta pedida es `validation`. Un valor que no es texto se escribe con su JSON.
- El escapado lo hace el proveedor al dibujar (R9), no la plantilla.
- Sin versión, se usa la última (edge case).

**Alternativas:** Jinja2 en sandbox (descartada por el propietario); `string.Template`, con otra sintaxis (`$var`) que
la del resto de Cerbero.

## R9. Proveedor Telegram

**Decisión:** un adaptador con httpx 0.28 contra la Bot API, sin librerías de bots. El contrato está en
[contracts/telegram.md](contracts/telegram.md).

- **Métodos:** `getMe` para la prueba de conexión, `sendMessage` para el mensaje (HTML y sin vista previa del enlace) y
  `getUpdates` para el sondeo largo de 25 s, con `allowed_updates` igual a `["message"]`.
- **Dibujo:** el mensaje neutral pasa a HTML escapado. Si pasa de 4096 caracteres, se recortan por este orden los
  hechos, las acciones y el resumen, y nunca el título ni el enlace (FR-018).
- **El token del bot va en la ruta de la URL**, así que ninguna URL ni excepción de httpx llega a los logs ni al sobre:
  el adaptador traduce cada fallo a un error propio sin URL.
- **Sondeo:** uno por instancia, con el `offset` en memoria, porque los cursores no se copian (docs/hermes.md). Al
  reiniciar, Telegram devuelve lo que no se confirmó en las últimas 24 horas, y la verificación es idempotente (R12).
- **Errores:** la traducción de cada respuesta de Telegram está en el contrato.

**Alternativas:** python-telegram-bot o aiogram (descartadas por el propietario).

## R10. Reintentos, timeouts y resultado ambiguo

**Decisión:** el adaptador clasifica cada fallo:

- **No salió:** `ConnectError`, `ConnectTimeout` o `PoolTimeout` de httpx, o un 429 de Telegram. Hermes reintenta con
  espera exponencial y jitter completo (base de 0,2 s y tope de 2 s), respetando el `retry_after` de Telegram (FR-027),
  mientras quede tiempo del timeout técnico de la función (10 s, del manifiesto). Si el `retry_after` no cabe, responde
  `rate_limited` con ese tiempo; si se agota el tiempo sin conectar, `provider_unavailable`, que vale reintentar.
- **No se sabe si salió:** `ReadTimeout`, `WriteTimeout`, `RemoteProtocolError`, un error de lectura tras enviar, o un
  5xx de Telegram a `sendMessage`. No reintenta y responde `timeout` con `retryable` falso y `outcome_unknown` verdadero
  (FR-016, FR-027).
- **El proveedor rechaza la petición:** 400, 401, 403 o 404, sin reintento, con la traducción del contrato.

## R11. Idempotencia

**Decisión:** una tabla `idempotency_keys` con clave `(caller, key)`: la identidad que llama y su `Idempotency-Key`.
Guarda la huella de la petición (SHA-256 del método, la ruta y el JSON canónico del cuerpo), el estado (`in_progress` o
`completed`) y la respuesta con su estado HTTP.

1. Un bloqueo en memoria por `(caller, key)` serializa las peticiones con la misma clave que llegan a la vez (una sola
   instancia; historia 5).
2. Si la fila existe y su huella es otra, responde `conflict`. Si está `completed`, devuelve la misma respuesta sin
   enviar. Si está `in_progress`, porque un corte dejó la petición a medias, responde `timeout` con `outcome_unknown`, y
   la fila pasa a `completed` con esa respuesta (FR-016), en la misma transacción que la entrada de auditoría que guardó
   su reserva (FR-029).
3. Si no existe, la petición se valida entera y, justo antes de llamar al proveedor, se inserta la fila `in_progress`
   con la entrada de auditoría del envío con `timeout` y `outcome_unknown`, y se confirma. Con el resultado, la fila
   pasa a `completed` en la misma transacción que la entrada de auditoría del resultado.

- Una petición que falla antes de llegar al proveedor no deja fila: se puede repetir con la misma clave cuando se
  corrija la causa (por ejemplo, el contacto que no existía).
- Las filas se conservan 24 horas (Assumptions), y una tarea de fondo borra las más antiguas; una que sigue
  `in_progress` porque nadie repitió la petición deja antes su entrada guardada en la auditoría, con la hora del
  intento (FR-029).
- **Código para una clave con otro cuerpo:** `conflict`, el que define el contrato común (`common.v1.json`) y usa
  Hécate. La spec decía `validation` y se alineó al revisar el plan (FR-016 y el escenario 2 de la historia 5).

## R12. Verificación de canales

**Decisión:**

- **El código:** 8 caracteres del alfabeto de Crockford sin `I`, `L`, `O` ni `U` (40 bits), generados con `secrets`. Se
  enseña como `XXXX-XXXX` y se compara sin distinguir mayúsculas, espacios ni guiones. Solo se guarda su SHA-256, con su
  caducidad: `HERMES_VERIFICATION_CODE_MINUTES`, 30 minutos por defecto, de 5 a 1440 (FR-022).
- **Pedir un código** (`contacts.code`) deja el canal sin verificar, borra su dirección y guarda el código nuevo; el
  anterior deja de valer.
- **Recoger el código:** el sondeo de la instancia del canal lee los mensajes de los chats privados. Un texto que
  coincide con un código pendiente y no caducado de un canal de esa instancia liga el canal a ese chat, que pasa a ser
  su dirección, lo marca verificado y borra el código, todo en una transacción. Hermes contesta «Canal verificado.» a
  ese chat. Un código caducado, usado o desconocido no hace nada ni recibe respuesta, para no dar pistas. Los demás
  mensajes se ignoran y no se guardan.
- **Fuerza bruta:** un código vale 30 minutos y solo en su instancia, y probar 2^40 combinaciones en ese tiempo no cabe
  en los límites de Telegram; no hace falta un freno propio.

## R13. Límites de uso

**Decisión:** un cubo de fichas por instancia, en memoria, con `requests_per_minute` del manifiesto (30 en Telegram),
ajustable en la instancia de 1 a 600. Sin fichas, el envío responde `rate_limited` con `retry_after_s` hasta la
siguiente, con HTTP 429 y sin llegar al proveedor (FR-028).

## R14. Auditoría

**Decisión:** la tabla append-only `audit_log` (R6), con una entrada por cada envío que llega a intentarse, cada
aplicación de la configuración, cada código pedido y cada canal verificado (FR-029). Cada entrada se escribe en la
misma transacción que el cambio que registra, así que, si no se puede escribir, el cambio no queda: la de un envío, con
la fila de su `Idempotency-Key` (R11); la de una aplicación, con la configuración; y las de un código y una
verificación, con el canal. `side_effect` y `discloses_data` salen del contrato de la función, resueltos con la
petición. Ninguna entrada lleva secretos, códigos ni direcciones
(SC-007). Los campos están en [data-model.md](data-model.md).

## R15. Canal local de administración y CLI

**Decisión:**

- **Canal:** una app FastAPI en `HERMES_ADMIN_DIR/admin.sock`, con el directorio en 0700 y el socket en 0600. Usa el
  sobre y los códigos comunes, sin API Contract publicado, `/capabilities` ni `Idempotency-Key` (principio II), y su
  seguridad viene del acceso local: Envoy no monta ese directorio. Operaciones `health`, `identity.enroll`,
  `config.plan`, `config.apply`, `config.export` y `contacts.code`
  ([contracts/admin-channel.md](contracts/admin-channel.md)). La salud va por este canal, como la de Hécate, porque
  ninguna API interna acepta llamadas sin autenticar (principio V).
- **CLI `hermes`:** argparse, de la biblioteca estándar, que sale con 2 ante un uso incorrecto, como la CLI de Hécate, y
  httpx con transporte por socket Unix. Órdenes `serve`, `health`, `enroll`, `config apply`, `config export` y
  `contacts code` ([contracts/cli.md](contracts/cli.md)); `hermes health --live` es el healthcheck del contenedor.

**Alternativas:** el protocolo JSON propio del canal de Hécate, otra pieza que escribir cuando FastAPI ya da el enrutado
y la validación; typer o click, una dependencia para cinco órdenes.

## R16. Contrato y OpenAPI

**Decisión:**

- **Fichero:** `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`, OpenAPI 3.1 en JSON, con el contrato
  común del tipo `chat` y la capa del proveedor Telegram ([contracts/api.md](contracts/api.md)).
- **Rutas:** `POST /chat/v1/send_message`, `GET /v1/capabilities` y `POST /v1/instances/{name}/test`, con la versión
  mayor en la ruta como pide la constitución. La salud no está en la API de red: va por el canal local (R15).
- **El sobre común gana tres campos opcionales en `meta`:** `service`, `provider` e `idempotency_key` (FR-011).
  `common.v1.json` pasa a la 1.1.0, compatible hacia atrás. La prueba de contrato de Hécate fija la versión del común,
  así que se actualiza, y la suite de Hécate se vuelve a pasar en macOS y en Linux.
- **Estado HTTP:** 200 para toda respuesta que llegó al proveedor, se completara o fallara, porque el fallo del
  proveedor va dentro del sobre (docs/hermes.md). Los fallos anteriores llevan el estado de su código: 400, 401, 404,
  409, 429, 500 o 503.
- **Contrato de la función (FR-017):** `kind`, `mode`, `timeout`, `side_effect`, `discloses_data` y `non_reversible`, en
  la extensión `x-cerbero-function` de la operación; `/v1/capabilities` lista de cada función su nombre, `kind`, `mode`
  y timeout.
- **Pruebas de contrato:** jsonschema con referencing, que resuelve las referencias a `common.v1.json`.

## R17. Proveedor fake y batería de conformidad

**Decisión:**

- **El proveedor `fake`** vive en memoria, guarda lo que envía y puede simular cada fallo; con `HERMES_FAKE_DIR`, deja
  lo que envía y recoge lo que le llega en ficheros, para las pruebas que lo miran desde fuera de Hermes
  ([contracts/fake.md](contracts/fake.md)). Solo se carga con `HERMES_FAKE_PROVIDERS=enabled`; sin ella, una
  configuración que lo usa se rechaza al aplicarla y al arrancar, para que nunca llegue a producción (principio IV).
- **La batería del tipo `chat`** se parametriza por proveedor: el fake y Telegram, con respuestas grabadas en respx,
  pasan los mismos casos ([contracts/conformance.md](contracts/conformance.md)) (FR-031).

## R18. Pruebas y calidad

**Decisión:**

- **Herramientas:** pytest e hypothesis ([ADR 0004](../../docs/adr/0004-pruebas-pytest-hypothesis.md)), con el plugin de
  anyio para lo asíncrono; respx para las respuestas grabadas de Telegram; testcontainers para un PostgreSQL de usar y
  tirar; jsonschema y referencing para los contratos.
- **Hécate real:** las pruebas del adaptador de Hécate y el E2E lanzan el binario `hecate` compilado del repositorio
  (`HECATE_BIN`), con su propio directorio de datos, el perfil `moderate` y certificados de 1 hora. Lo que necesita
  mover el reloj usa dobles del puerto en memoria con el reloj inyectado: las 15 renovaciones de SC-005 y la hora de
  SC-004.
- **E2E:** docker compose con PostgreSQL, Hécate, Hermes y Envoy, y un cliente de referencia en Python que se da de
  alta, firma su token corto y llama como un componente.
- **Calidad:** ruff para el estilo y el formato, mypy en modo estricto y pip-audit para las vulnerabilidades conocidas.
- **Plataformas:** las pruebas unitarias y de integración corren en macOS; el E2E, en contenedores Linux.

## R19. Logs y observabilidad

**Decisión:** una línea JSON por evento en la salida estándar, con el logging de la biblioteca estándar y un formateador
propio: `ts`, `level`, `logger`, `event` y sus campos. Cada petición a la API deja al terminar una línea `api.request`
con el `request_id`, la operación, la identidad que llama, la `Idempotency-Key`, el resultado, el estado HTTP y la
duración; el canal local deja la suya por operación. Toda línea que deja una petición, también las de la aplicación,
lleva su `request_id`, y lo que sigue después de la petición, como los sondeos que arranca una aplicación de la
configuración, empieza fuera de ella. Nunca secretos, tokens, códigos, direcciones, URLs de Telegram ni certificados
(FR-030). httpx y httpcore, en nivel `WARNING`. Sin OpenTelemetry (Complexity Tracking).

## R20. Consolidación al cerrar (FR-032)

- **docs/hermes.md:** el diseño vigente:
  - la configuración en la base, aplicada con la CLI, y el repositorio privado solo para las librerías propias;
  - el proxy, el canal local con la salud y los dos sockets;
  - las rutas versionadas, los campos nuevos de `meta` y el claim `cerbero_audit`;
  - `conflict` para una clave reutilizada con otro cuerpo, donde hoy dice `validation`;
  - la verificación con el código que pide el admin.
- **ARCHITECTURE:** el token corto deja de llevar los permisos que conceden el acceso y lleva el contexto de auditoría
  en `cerbero_audit`; la persistencia de las apps; el resumen de Hermes.
- **GLOSSARY:** los términos nuevos que fije la consolidación (configuración, variable de plantilla).
- **DECISIONS:**
  - «APIs internas en Python y certificado del cliente» pasa a Tomadas con el ADR 0011;
  - «Persistencia por proyecto», con el ADR 0012;
  - «Detalle de contratos y versionado de Hermes» queda resuelto por el contrato de esta feature;
  - «Repositorio privado de lo propio» recoge que las instancias y las plantillas de Hermes viven en su base;
  - el acceso por app, como diseñado y no implementado, con su ADR 0013;
  - dos riesgos aceptados: el token corto ligado a la identidad que declara el proxy, y los secretos en la memoria de
    Python, que no se pueden borrar.
- **ADR 0013:** el acceso por app, guardado en Hécate y sin permisos en el token, como el ADR 0010 se escribió al cerrar
  la 006.
- **README:** lo que ya hace Hermes, en «Estado del Proyecto».

## R21. Entorno de pruebas sin resolver el despliegue

**Decisión:** el compose de `hermes/deploy/` es el entorno de pruebas y de desarrollo, el del E2E y el quickstart. No
fija cómo corre Hermes en el home lab, que sigue abierto en DECISIONS. La imagen de Hermes parte de `python:3.15-slim`
con uv y corre con un usuario sin privilegios (UID 10001), el mismo que Envoy.
