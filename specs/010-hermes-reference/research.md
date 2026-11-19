# Research: Hermes documentado en un documento de alto nivel y documentos específicos

Decisiones del cambio 010. Cada una dice qué se elige, por qué y qué se descartó. Parte de `main` tras la feature 009
(`8d61269`): un solo documento, `docs/hermes.md`, con la plantilla de 14 apartados y unas 500 líneas, y un
`hermes/README.md` de tres líneas. Lo necesario para usar Hermes está en los contratos y el quickstart de la 008 (la
API, el canal local, la CLI, el fichero de la configuración, el proxy, Telegram, la batería de conformidad y el fake),
en los de la 009 (el acceso por app) y en el código.

## R0. Decisiones del propietario

- **Specify (2026-11-16):** el alcance de la entrada: un documento de alto nivel, los documentos específicos en
  `docs/hermes/` y `hermes/README.md` como guía de desarrollo, sin cambiar código ni contratos.
- **Clarify (2026-11-16):**
  - nueve documentos específicos: contratos, llamadas, comunicación, configuración, proveedores, seguridad, integración,
    CLI y operación; configuración queda para el fichero, su aplicación y las instancias;
  - la referencia de la CLI se llama «CLI y variables de entorno», y «configuración» queda solo para la de Hermes;
  - los procedimientos de operación dan cada paso con las órdenes de `hermes` y de `hecate`, y un apartado explica cómo
    se ejecutan en el compose de pruebas, sin presentarlo como el despliegue de casa.
- **Plan (2026-11-16):**
  - las pruebas y el recorrido usan mocks y fakes; solo el E2E del happy path usa un bot de Telegram real, el bot de
    pruebas del propietario;
  - su token vive en `hermes/deploy/.secrets/telegram-test.env`, como `TELEGRAM_TEST_BOT_TOKEN`, con permisos 0600; git
    ya ignora `.secrets/`. El recorrido lo pasa a la Hécate de pruebas por la entrada estándar y nunca lo escribe;
  - el código de verificación lo envía el propietario al bot desde su cuenta de Telegram.
- **Heredadas de la 007:** el estado de lo construido vive solo en DECISIONS y en el README; los documentos de diseño no
  dicen nada de su estado y las referencias de uso describen solo lo que existe; la guía de integración no lleva código;
  ningún documento describe cómo corren los componentes en el home lab.

## R1. Estructura y nombres

**Decisión:** `docs/hermes.md` es el documento de alto nivel, y `docs/hermes/` tiene nueve documentos específicos, con
nombre en español y sin tildes, como `docs/hecate/`:

| Documento | Desarrolla |
|---|---|
| `docs/hermes/contratos.md` | La taxonomía, las tres capas de datos, el sobre común con sus códigos de error y sus estados HTTP, el contrato de una función (`x-cerbero-function`), `chat.send_message` con el orden de sus comprobaciones, y el formato y el versionado de la API |
| `docs/hermes/llamadas.md` | La llamada síncrona, la task (sus estados, su cola, sus rutas, la cancelación y la retención), la idempotencia, los reintentos seguros, el resultado desconocido y los timeouts |
| `docs/hermes/comunicacion.md` | Los contactos (personas y grupos) y sus canales, las plantillas con sus versiones y sus variables, el mensaje neutral y cómo lo dibuja cada proveedor, la verificación de un canal y `chat.ask`, con su ejemplo con Telegram |
| `docs/hermes/configuracion.md` | La configuración y su fichero (formato, lectura estricta y validación), el plan, la aplicación y la exportación, y las instancias (parámetros, referencias de secretos, límite de uso y prueba de conexión) |
| `docs/hermes/proveedores.md` | Qué es una librería, las oficiales y las propias, sus partes, el manifiesto, su versión, el polling, la batería de conformidad de su tipo de servicio, el proveedor fake y la librería de Telegram (errores, límites y dibujo) |
| `docs/hermes/seguridad.md` | Frente a qué protege Hermes, qué mecanismo cubre cada amenaza (la identidad, el proxy, la autenticación, el acceso por app, los secretos, la auditoría, los datos personales y los logs), qué datos guarda y dónde, y los riesgos aceptados |
| `docs/hermes/integracion.md` | El lado de quien llama: lo que necesita, cada llamada a través de Envoy, el token corto, la `Idempotency-Key`, el sobre y los errores, las capacidades y la prueba de conexión, los límites, las operaciones y cómo probar contra Hermes |
| `docs/hermes/cli.md` | «CLI y variables de entorno»: cómo se lanza Hermes, lo que necesita, cada variable de entorno, las convenciones y cada orden |
| `docs/hermes/operacion.md` | La puesta en marcha, un procedimiento por situación y cómo se ejecuta cada orden en el compose de pruebas |

El desarrollo de Hermes va en `hermes/README.md` (R8).

- Cada documento específico abre con un párrafo que dice qué cubre y para quién, desarrolla su tema y cierra con
  «Relacionado»: los enlaces al documento de alto nivel y a sus vecinos, como en `docs/hecate/`.
- `cli.md` se llama así, como en Hécate; su título es «CLI y variables de entorno de Hermes» (R0).

**Alternativas:** siete documentos (el propietario prefirió separar llamadas y comunicación); un `telegram.md` aparte
(lo propio de Telegram es una librería, y proveedores la desarrolla con su apartado); nombres en inglés (rompen con el
idioma de los docs).

## R2. El documento de alto nivel

**Decisión:** `docs/hermes.md` deja la plantilla de 14 apartados y pasa al esquema de `docs/hecate.md`:

1. **Por qué existe:** los servicios de terceros detrás de contratos estándar por tipo de servicio, para que un Playbook
   funcione igual con cualquier proveedor y cada integración siga las mismas buenas prácticas.
2. **Qué hace y qué no hace:** su frontera.
3. **Quién lo usa:** los componentes a través de Envoy (en producción, Hades), el admin con su CLI, Hécate (identidad,
   publicaciones, lista de acceso, secretos y checkpoints), el PostgreSQL común y los proveedores, con el diagrama de
   contexto.
4. **Cómo encaja todo:** la vida de una llamada, con su diagrama, y lo que pasa al arrancar.
5. **Conceptos:** un párrafo y un enlace por concepto: contratos, llamadas, comunicación, configuración, proveedores y
   seguridad.
6. **Usarlo y desarrollarlo:** un párrafo y un enlace por uso: integración, CLI y variables de entorno, operación y
   desarrollo.
7. **Ante fallos:** qué pasa si cae Hermes, un proveedor, Hécate o la base, o si su identidad deja de valer (principio
   VI), con los avisos; los procedimientos, en operación.
8. **Decisiones y riesgos:** los ADR 0011, 0012 y 0013, las desviaciones frente a SecOps, lo descartado y los riesgos
   aceptados, con enlaces a DECISIONS.
9. **Preguntas frecuentes:** las de hoy; la que toca un concepto enlaza a su documento.
10. **Referencias.**

El documento de alto nivel no pasa de 2.500 palabras, unos 10 minutos de lectura (SC-001), como `docs/hecate.md`. Los
demás `docs/<componente>.md` mantienen su plantilla (FR-018).

**Alternativas:** mantener los 14 apartados con remisiones (sigue siendo largo y mezcla el alto nivel con el detalle).

## R3. Qué va a cada documento

**Decisión:** cada apartado del documento actual tiene destino (FR-004, SC-002). A una referencia de uso (integración,
CLI y operación) solo llega lo construido; el diseño de lo que no lo está va a su documento de concepto (FR-013).

| Apartado actual | Destino |
|---|---|
| 1. Propósito y frontera | Alto nivel: «Por qué existe» y «Qué hace y qué no hace» |
| 2. Contexto: diagrama y quién usa qué | Alto nivel: «Quién lo usa»; lo que necesita quien llama, en integración |
| 3. Taxonomía | Contratos; un párrafo en el alto nivel |
| 3. Tres capas de datos | Contratos |
| 3. Task | Llamadas |
| 3. Instancia, contacto y configuración | La instancia y la configuración, en configuración; el contacto, la plantilla y la variable, en comunicación |
| 4. Llamada síncrona | Llamadas; su resumen, en el alto nivel («Cómo encaja todo») |
| 4. Task | Llamadas |
| 4. Ejemplo: preguntar al propietario por Telegram | Comunicación, con `chat.ask` |
| 4. Idempotencia y reintentos | Llamadas; el lado de quien llama, en integración |
| 4. Timeouts | Llamadas |
| 5. Introducción (el OpenAPI) | Contratos; el enlace a cada esquema, en integración |
| 5. Sobre común | Contratos (códigos, estados HTTP y qué es un error); qué hace quien llama con cada código, en integración |
| 5. Contrato de una función | Contratos |
| 5. `chat.send_message` | Contratos |
| 5. Autenticación | Seguridad (el diseño); el lado de quien llama, en integración |
| 5. API | Las operaciones construidas, en integración; las rutas de las tasks, en llamadas, como diseño; fuera de la API, el formato y el versionado, en contratos |
| 6. Añadir un proveedor: una librería | Proveedores |
| 6. Instancias | Configuración |
| 6. La configuración y la CLI | El fichero, la validación, el plan, la aplicación y la exportación, en configuración; la tabla de órdenes, los códigos de salida y las variables, en la CLI |
| 6. Comunicación: contactos y plantillas | Comunicación |
| 7. Organización interna | `hermes/README.md`, con los paquetes reales (R8); Event Sourcing y CQRS de la task, en llamadas, como diseño; el proceso con sus tareas de fondo, en operación |
| 8. Datos | Qué guarda y dónde, y los datos personales, en seguridad; las copias, en operación; la retención de las tasks, en llamadas |
| 9. Seguridad | Seguridad; el acceso por app, también en integración |
| 10. Operación | Alto nivel: «Ante fallos», con los avisos como diseño; cada situación, con su procedimiento, en operación; el arranque y la salud, en operación y en la CLI; el estado por instancia, en integración (las capacidades) y en operación; los límites de uso, en configuración; la observabilidad, en decisiones y riesgos |
| 11. Pruebas | `hermes/README.md`; la batería de conformidad y el fake, en proveedores; probar contra Hermes, en integración |
| 12. Decisiones y riesgos | Alto nivel |
| 13. Preguntas frecuentes | Alto nivel |
| 14. Referencias | Alto nivel; la de Telegram, también en proveedores, y la de Envoy, en seguridad |

**Alternativas:** repartir sin un mapa (no habría forma de comprobar que nada se pierde).

## R4. Guía de integración

**Decisión:** `docs/hermes/integracion.md` sigue la vida de un componente que llama a Hermes, sin código (R0):

1. **Qué necesita antes de la primera llamada:** su identidad de Hécate, que desarrolla la [guía de integración de
   Hécate](../../docs/hecate/integracion.md); el acceso a Hermes que le concede el admin; la dirección de Envoy; y la CA
   raíz de Cerbero, con la que reconoce el certificado de Hermes.
2. **El flujo completo:** un diagrama desde que el admin le concede el acceso hasta un envío y su repetición.
3. **Cada llamada:** TLS 1.3 con su certificado y su intermedia, y el token corto: `iss`, `aud` (el SPIFFE ID de
   Hermes), 300 s como mucho con 30 s de margen, un `jti` de un solo uso y, si lo pone, el contexto de auditoría en
   `cerbero_audit` con sus límites. Qué comprueba Hermes y en qué orden, enlazando al diseño en seguridad.
4. **La `Idempotency-Key`:** obligatoria en `chat.send_message`, de 1 a 128 caracteres ASCII imprimibles y de quien
   llama; qué devuelve Hermes al repetir la misma petición, al reutilizarla con otra y tras un corte, y cuánto la
   guarda.
5. **El sobre y los errores:** el sobre común, `X-Request-Id` y `meta.request_id`, y cada código con su estado HTTP, si
   se puede reintentar y qué hacer; un fallo del proveedor va en un 200 con el sobre `failed`; Hermes sin estar listo, o
   sin respuesta de Hécate al buscar a quien llama, responde `unavailable`.
6. **Las capacidades y la prueba de conexión:** qué dice cada estado de instancia y cuándo probar una.
7. **Límites:** el cuerpo, las cabeceras, el token, un destinatario por petición y el límite de uso de cada instancia.
8. **Operaciones:** una tabla con las tres de la versión 1, su autenticación, si llevan `Idempotency-Key` y su enlace al
   OpenAPI de `shared/contracts`.
9. **Probar un componente contra Hermes:** el compose de pruebas con el proveedor fake y sus bandejas, y el cliente de
   referencia de las pruebas (`hermes/tests/support/caller.py`), con sus órdenes `enroll`, `send` y `bench`.

**Fuentes:** el OpenAPI de Hermes y el sobre común; los contratos `api.md` de la 008 y de la 009; en el código
`api/middleware.py`, `api/envelope.py`, `api/auth.py`, `application/authentication.py`, `application/idempotency.py` y
`application/send_message.py`; y el cliente de referencia.

**Alternativas:** repetir en esta guía el lado de Hécate (alta, renovación y publicaciones): ya lo desarrolla su guía, y
se enlaza (FR-003).

## R5. Referencia de la CLI y las variables de entorno

**Decisión:** `docs/hermes/cli.md` tiene:

1. **Lanzar Hermes:** `hermes serve`, en primer plano hasta SIGINT o SIGTERM: abre el socket de la API y el del canal
   local, sin ningún puerto TCP, sigue los pasos del arranque, escribe sus logs en JSON y, al parar, borra los sockets.
   Sus códigos de salida.
2. **Lo que necesita:** el directorio de estado y el del proxy, el socket de la API, al que solo llega Envoy, el del
   canal local en un directorio privado, Envoy delante (como desarrolla seguridad), Hécate en `HERMES_HECATE_URL` y su
   base en el PostgreSQL común.
3. **Variables de entorno:** las once que existen (`HERMES_STATE_DIR`, `HERMES_PROXY_DIR`, `HERMES_RUN_DIR`,
   `HERMES_ADMIN_DIR`, `HERMES_HECATE_URL`, `HERMES_DATABASE_URL`, `HERMES_DATABASE_PASSWORD_REF`,
   `HERMES_VERIFICATION_CODE_MINUTES`, `HERMES_FAKE_PROVIDERS`, `HERMES_FAKE_DIR` y `HERMES_LOG_LEVEL`), con su valor
   por defecto, su efecto y sus límites.
4. **Convenciones:** la salida, el formato de los errores (`error: <código>: <mensaje>`, con una línea `error: <ruta>:
   <mensaje>` por detalle), los errores locales (`error: cannot reach hermes at <socket>: <motivo>`), los códigos de
   salida (0, 1 y 2) y cómo se pide el token de alta: sin eco en un terminal o desde la entrada estándar, nunca en la
   línea de órdenes.
5. **Las órdenes:** las cinco de primer nivel y sus seis órdenes finales (`serve`, `enroll`, `config apply`, `config
   export`, `contacts code` y `health`), cada una con sus argumentos, su salida si va bien, sus errores y su código de
   salida.

**Fuentes:** `infrastructure/admin/cli.py`, la ayuda de la CLI, `infrastructure/settings.py`, `infrastructure/main.py`,
`infrastructure/sockets.py` y los contratos `cli.md` y `admin-channel.md` de la 008.

**Alternativas:** documentar también el protocolo del canal local (fuera de alcance: su único cliente es la CLI).

## R6. Procedimientos de operación

**Decisión:** `docs/hermes/operacion.md` tiene un procedimiento por situación, cada uno con «Cuándo», «Qué necesitas»,
«Pasos» (con las órdenes de `hermes` y de `hecate`) y «Cómo comprobarlo», como en Hécate:

| Procedimiento | Notas |
|---|---|
| Puesta en marcha | En Hécate: la contraseña de la base y los tokens de los bots como secretos de Hermes, su token de alta y el acceso de quien lo llama; la base y su usuario en el PostgreSQL común; Envoy; `hermes serve`, `hermes enroll`, `hermes config apply` y la verificación de un canal |
| Conceder y retirar el acceso a quien llama | La concesión vale en la llamada siguiente y la retirada, como mucho a los 240 s |
| Verificar un canal | Pedir el código, que la persona lo envíe al bot y comprobar la respuesta; cuándo se pierde la verificación |
| Rotar el token de un bot | La versión nueva del secreto en Hécate; cuándo la toma Hermes y cómo se comprueba (R12); volver a la anterior |
| Hécate sellada o caída | Qué sigue funcionando (la caché de secretos, las publicaciones y la lista de acceso, la identidad) y cómo se vuelve |
| PostgreSQL caído | Hermes no está listo y vuelve solo cuando la base responde |
| Una instancia que falla | Su estado en las capacidades, la prueba de conexión, las credenciales rechazadas y el proveedor que no responde |
| La identidad de Hermes caducada, revocada o de otra CA | Un token de alta nuevo y `hermes enroll` |
| Sin copia de la lista de acceso y Hécate caída | La salud lo dice y el arranque sigue cuando Hécate responde |
| Restaurar una copia | Lo que se puede hacer con la base y el directorio de estado; la política común de copias y el cifrado con `wrap`, en DECISIONS |

- **El compose de pruebas:** un apartado explica cómo se ejecuta cada orden en el compose de `hermes/deploy/` (con
  `compose.test.yaml` para el proveedor fake), cómo se preparan sus secretos (`prepare.sh` y `.secrets/`) y cómo se usa
  un bot de Telegram de pruebas: su token en `hermes/deploy/.secrets/telegram-test.env` y en la Hécate de pruebas desde
  la entrada estándar (R0). No se presenta como el despliegue de casa.
- No describe cómo corren los componentes en el home lab ni nada que no exista. Lo que no existe se dice como un hecho y
  remite a DECISIONS, sin palabras de tiempo.

**Fuentes:** el apartado 10 actual, los apartados 2 a 9 del quickstart de la 008, el 4 del de la 009 y `hermes/deploy/`.

**Alternativas:** los procedimientos dentro de la referencia de la CLI (mezcla qué hace cada orden con qué hacer ante
cada situación).

## R7. Documento de seguridad

**Decisión:** `docs/hermes/seguridad.md` tiene:

1. **Frente a qué protege:** alguien en la red interna sin un certificado de Cerbero, un componente sin acceso, un token
   robado o repetido, una petición que intenta saltarse a Envoy, una fuga de secretos o de datos personales, un error
   del admin al configurar y una reescritura de la auditoría.
2. **Qué lo cubre:** una tabla amenaza, mecanismo y documento que lo desarrolla.
3. **El proxy:** lo que tiene que hacer Envoy (TLS 1.3, pedir el certificado sin rechazar ninguno, sustituir
   `X-Forwarded-Client-Cert` por la del certificado de la conexión, recargar el certificado renovado) y por qué el
   socket de la API solo lo alcanza Envoy, con su configuración de pruebas como referencia.
4. **La autenticación y el acceso:** el orden de las comprobaciones, la lista de acceso de Hécate y que el token no
   concede nada.
5. **Secretos, auditoría, datos personales y logs:** lo que nunca sale de la memoria ni llega a un log, la entrada de
   auditoría en la misma transacción que su cambio, dónde viven las direcciones y el enmascarado de los logs.
6. **Los datos:** qué guarda Hermes, dónde y qué se copia.
7. **Lo que no cubre y los riesgos aceptados:** el token ligado a la identidad que declara Envoy y los secretos en la
   memoria de Python, con enlaces a DECISIONS.

**Fuentes:** los apartados 8 y 9 actuales, el contrato `proxy.md` de la 008, los riesgos de DECISIONS y los ADR 0011 y
0013.

## R8. README de desarrollo

**Decisión:** `hermes/README.md` explica:

- qué es Hermes, con el enlace al documento de alto nivel;
- qué herramientas necesita: uv, Python 3.15, Docker con Colima y el binario de Hécate en `HECATE_BIN`;
- cómo se ejecuta la suite, sus marcas (`docker` y `hecate`) y qué se salta sin ellas, y el E2E con el compose;
- las comprobaciones de calidad: ruff, el formato, `mypy --strict` y pip-audit;
- para qué sirve cada ayudante de `hermes/tests/support/`: la Hécate real y su doble, el cliente de referencia, la CA de
  pruebas, Envoy, el compose, PostgreSQL y las respuestas grabadas de Telegram;
- la organización real del paquete (`domain`, `application` e `infrastructure`) y sus patrones;
- dónde está la spec de cada feature.

**Fuentes:** `hermes/pyproject.toml`, el árbol de `hermes/tests/`, el apartado 1 de los quickstarts de la 008 y la 009.

## R9. El estado: DECISIONS y README

**Decisión:**

- **DECISIONS, «Abiertas»,** ya recoge las piezas de Hermes sin construir: las tasks, `chat.ask`, los grupos de
  contactos, la gestión desde Ténaro, el repositorio privado de lo propio, la política común de copias y `wrap` /
  `unwrap`. El plan no encontró otra; si al implementar aparece alguna, se añade con el formato de las demás.
- **Sus enlaces:** las filas de `chat.ask`, de las tasks y de los grupos de contactos enlazan a apartados de
  `docs/hermes.md` que dejan de existir; pasan a enlazar a comunicación y a llamadas (FR-016).
- **README, «Estado del Proyecto»:** la línea de Hermes ya dice lo que hace, con la comprobación del acceso de la 009, y
  enlaza a `docs/hermes.md`; se comprueba y no cambia salvo que falte algo.

## R10. Sin enmienda de la constitución

**Decisión:** la constitución 1.2.1 ya dice que el diseño vigente de un componente vive en `docs/<componente>.md` y en
sus documentos de detalle en `docs/<componente>/`. Este cambio no la toca.

## R11. Verificación: comprobaciones y recorrido

**Decisión:** las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas: se escriben antes que los
documentos, fallan sobre `main` y pasan al terminar.

- **Comprobaciones automáticas:**
  - cada orden final de la ayuda de la CLI está en la referencia, y ningún documento de Hermes nombra una orden que no
    exista;
  - cada variable de `settings.py` está en la referencia, y ningún documento nombra otra;
  - cada operación del OpenAPI está en la guía de integración, y ninguna referencia de uso nombra otra ruta;
  - los enlaces y las anclas de los documentos del cambio, y los de DECISIONS, llevan a un sitio que existe;
  - el cambio no toca código, configuración, contratos ni manifiestos;
  - ningún documento usa palabras de tiempo, el documento de alto nivel no pasa de 2.500 palabras y ningún documento
    lleva el token del bot de pruebas.
- **Recorrido** con el compose de `hermes/deploy/`, siguiendo los procedimientos tal como están escritos:
  - **el happy path con Telegram real:** la puesta en marcha con una instancia del bot de pruebas, la verificación de un
    canal con el propietario enviando el código y un envío como componente a través de Envoy que llega a su Telegram;
  - **con el proveedor fake:** conceder y retirar el acceso, verificar un canal desde su bandeja, Hécate sellada y
    caída, PostgreSQL caído, la identidad de Hermes revocada con un alta nueva, y sin copia de la lista de acceso con
    Hécate caída;
  - **rotar el token de un bot y una instancia que falla:** con mocks, como decidió el propietario: las pruebas con
    respuestas grabadas de Telegram (credenciales rechazadas, prueba de conexión) y de la caché de secretos, y la
    lectura de que cada paso usa una orden que existe.
- Los scripts viven fuera del repositorio, en el espacio de trabajo de la sesión: el cambio no añade código (FR-001).

**Alternativas:** solo comprobaciones y lectura (en la 007 el propietario prefirió el recorrido, porque un paso mal
escrito solo se vería en un incidente); el bot real en todos los procedimientos (el propietario lo limita al happy
path).

## R12. Datos que no coinciden con el código

**Decisión:** el documento pasa a describir lo que hace el código (FR-004), y la diferencia queda anotada en la guía de
revisión. Encontrado al planificar:

- **Rotar el token de un bot:** el diseño (y la pregunta frecuente de `docs/hecate.md`) dice que se lanza la prueba de
  conexión de la instancia, pero esa prueba es una operación de la API, para componentes autenticados con acceso, y la
  CLI de Hermes no tiene una orden para ella. El procedimiento dice cómo se comprueba con lo que existe: Hermes toma la
  versión nueva al refrescar su caché, y el estado de la instancia, o su prueba de conexión desde un componente con
  acceso, lo confirma.
- Lo que aparezca al contrastar cada dato se resuelve igual. Si el código parece contradecir una spec fusionada, no se
  corrige aquí: se avisa al propietario.

## R13. Enlaces y otros documentos

**Decisión:**

- DECISIONS: sus tres enlaces a apartados de `docs/hermes.md` pasan a los documentos específicos (R9).
- ARCHITECTURE remite a `docs/hermes.md`, que sigue valiendo; si en algún punto remite a un detalle de Hermes, pasa a
  enlazar al documento específico.
- El README, los ADR 0011, 0012 y 0013 y los docs de otros componentes enlazan al fichero, que se mantiene; los ADR no
  se modifican.
- `hermes/README.md` se reescribe (R8). Las specs fusionadas no se tocan (FR-019).

## R14. Diagramas y formato

**Decisión:**

- Mermaid, como hoy. Los diagramas se mueven con su contenido: el contexto, al alto nivel; la llamada síncrona y los
  estados de la task, a llamadas; el ejemplo de `chat.ask`, a comunicación.
- La guía de integración gana el suyo: el flujo completo de un componente.
- Las líneas no pasan de 120 caracteres, como en `docs/hecate/`, y las referencias van en tablas (órdenes, variables,
  operaciones y códigos).
