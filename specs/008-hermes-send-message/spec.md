# Feature Specification: Base de Hermes y envío de mensajes por Telegram

**Feature Branch**: `feature/008-hermes-send-message`

**Created**: 2026-11-13

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/008-hermes-send-message. Primera feature de Hermes, el
componente que envuelve los servicios de terceros detrás de contratos estándar por tipología (docs/hermes.md). Construye
su base y su primera función, `chat.send_message` con Telegram. Por qué ahora: Hermes es una de las dos fronteras de
Cerbero con el exterior y solo depende de Hécate, que ya está construida. Además, su base (cómo recibe llamadas
autenticadas, cómo usa Hécate y el sobre común) la reutilizarán los demás servicios en Python. Incluye: El servicio: en
Python, con FastAPI detrás de un proxy TLS que termina el mTLS y reenvía el certificado del cliente en una cabecera,
como `X-Forwarded-Client-Cert` de Envoy. Lo decidió el propietario. Su ADR recoge estas condiciones: Hermes no publica
su puerto (solo le habla el proxy, por una red interna de docker-compose o con mTLS entre el proxy y Hermes); el token
corto se liga a la identidad que declara el proxy, como riesgo aceptado; la decisión va de la mano de la del despliegue
en el home lab; propuesta: el proxy sustituye la cabecera de certificado que llegue del cliente; propuesta: el proxy
recarga sin reiniciarse el certificado que Hermes renueva; propuesta: Hermes sigue haciendo las comprobaciones de la
guía de integración sobre el certificado reenviado. El ADR cierra la decisión abierta «APIs internas en Python y
certificado del cliente». La identidad con Hécate, según docs/hecate/integracion.md: el alta, con el token que crea el
admin; la renovación a los dos tercios de la vida del certificado, repitiendo la misma petición si no llega la
respuesta; el token corto en cada llamada; la resolución de sus secretos
(`secret://hermes/telegram/<instancia>/bot_token`) con su caché blanda y dura, también con Hécate sellada o caída. La
verificación de las llamadas que recibe: el certificado reenviado, el token corto (`iss`, `aud` igual a su SPIFFE ID,
300 s y `jti` de un solo uso), la lista de revocación y las claves que publica Hécate. El contrato común: el sobre, los
códigos de error cerrados con `retryable`, la `Idempotency-Key`, la versión mayor en la ruta, `/capabilities` y la salud
(`/health/live` y `/health/ready`), con su OpenAPI 3.1 en shared/contracts. La categoría communication: el tipo de
servicio chat, con su función síncrona `send_message`, el modelo neutral de mensaje y sus plantillas; la librería de
Telegram con su manifiesto, y sus instancias, con parámetros y referencias a secretos; los contactos, con su canal
verificado por un código de un solo uso; los reintentos seguros y el resultado ambiguo (`outcome_unknown`); los límites
de uso por instancia; el registro append-only de las llamadas con `side_effect` o `discloses_data`. Pruebas: un
proveedor fake de chat y la batería de conformidad del tipo chat. Fuera de alcance: las tasks y `chat.ask`, que van a la
segunda feature; otras categorías y proveedores; la gestión desde Ténaro y el flujo de PRs del repositorio privado; las
copias con `wrap`/`unwrap`; la firma de checkpoints; los permisos que salen de las declaraciones (sus secretos viven
bajo `secret://hermes/`, que su identidad ya resuelve); OpenTelemetry, que sigue como decisión común abierta. Abierto
para clarify: cómo gestiona el admin los contactos, y de dónde lee Hermes sus instancias y plantillas mientras no
existen Ténaro ni el flujo del repositorio privado; quién puede llamar a Hermes; si entran los grupos (`shared_chat` y
`fan_out`). Rama `feature/008-hermes-send-message`, creada desde main; commit de squash `feat:`."

## Clarifications

### Session 2026-11-14

- Q: ¿Qué proveedor de chat estrena Hermes: Telegram, WhatsApp, correo o Slack? → A: Telegram, decidido por el
  propietario tras compararlos con su caso de uso guía (un dispositivo nuevo en la red que se bloquea mientras se pide
  aceptarlo o bloquearlo). Telegram encaja en el diseño tal cual: lo que llega del proveedor, también las pulsaciones de
  los botones que necesitará `chat.ask`, se recoge consultándolo, sin exponer nada, y Telegram guarda lo pendiente hasta
  24 horas. WhatsApp solo entrega lo que entra por webhook a una URL pública y exige cuenta de empresa, plantillas y
  pago por mensaje; el correo es otro tipo de servicio (`email`); y Slack necesitaría una conexión saliente siempre
  abierta (Socket Mode) para los botones. Como los Playbooks usan el contrato del tipo `chat`, otro proveedor se añade
  después como una librería más, sin cambiar a quien llama.
- Q: ¿Quién puede llamar a Hermes, y quién guarda esa lista? → A: Solo las identidades con acceso a Hermes, que con
  acceso pueden usar toda la app, sin permisos por función. La lista de qué identidades pueden llamar a cada app la
  guarda Hécate, de forma central, y Hermes la comprueba con ella; el token corto no concede el acceso, porque lo firma
  quien llama. Los usuarios, los roles y el perfilado, si hacen falta, son del frontal de las apps (Ténaro) o los escala
  cada app: esto es validación interna entre componentes.
- Q: ¿En qué orden se construye la lista de acceso de Hécate respecto a Hermes? → A: La 008 se termina con su alcance:
  Hermes autentica a quien llama, pero no comprueba el acceso por app, porque Hécate no tiene esa lista. La decisión
  queda tomada y se consolida al cerrar la 008, y la parte de Hécate la aborda la 009.
- Q: ¿Cómo da de alta y gestiona el admin los contactos, y de dónde lee Hermes sus instancias y plantillas mientras no
  existen Ténaro ni el flujo de PRs del repositorio privado? → A: De la base de datos de Hermes, en un PostgreSQL común
  a las apps de Cerbero, con una base y un usuario propios por app y sin acceso a las de las demás (Hécate sigue con
  SQLite). Ahí van su configuración (instancias, plantillas, variables y contactos, también lo sensible que no es
  secreto, como la cuenta de Telegram de cada canal) y lo que escribe al funcionar (la verificación de los canales, la
  idempotencia y la auditoría). Los secretos siguen en Hécate, por `secret://`: el propietario valoró llevarlos a la
  base y los dejó en Hécate, por el principio V de la constitución y para que todos vivan en un solo sitio. El admin
  rellena la base con una CLI de Hermes que solo funciona en su equipo, por un canal local: aplica un fichero con la
  configuración entera, la vuelve a sacar de la base y da el código de verificación de un canal. La API de red no
  gestiona la configuración. Las instancias y las plantillas dejan el repositorio privado, que se queda con las
  librerías propias.
- Q: ¿Entran en esta feature los grupos de contactos (`shared_chat` o `fan_out`) y los envíos a varias personas a la
  vez? → A: No. `send_message` va a un solo contacto por petición: la lista de destinatarios lleva uno, y con más Hermes
  responde `validation`. Para avisar a varias personas, quien llama hace una petición por persona. Los resultados
  parciales y el resultado ambiguo por persona complican la idempotencia, y tienen más sentido con `chat.ask`, donde
  gana la primera respuesta válida. Como el contrato ya recibe una lista, aceptar más de un contacto después no rompe a
  quien llama.
- Q: ¿Qué responde una `Idempotency-Key` repetida con otro cuerpo? → A: `conflict`, como define el contrato común
  (`common.v1.json`) y responde Hécate. La spec decía `validation`; el propietario la alineó al revisar el plan.
- Q: ¿La salud de Hermes se consulta por su API de red sin autenticar? → A: No. Va por el canal local de administración
  (`hermes health`), como la de Hécate, porque ninguna API interna acepta llamadas sin autenticar (principio V). Lo
  decidió el propietario al analizar la feature.

## User Scenarios & Testing *(mandatory)*

Hermes está diseñado ([docs/hermes.md](../../docs/hermes.md)) y sin construir. Es la pieza que envuelve los servicios de
terceros detrás de contratos estándar por tipología: un Playbook que usa el contrato del tipo `chat` funciona igual con
cualquier proveedor de chat, y un cambio en la API del proveedor no le llega. Esta feature deja hecha la base del
servicio y su primera función, `chat.send_message`, con Telegram como proveedor. Hades, que será quien la llame desde
sus Playbooks, no existe; en esta feature la llaman componentes de prueba que se autentican como lo haría él.

En esta spec, «quien llama» es un componente de Cerbero autenticado con su certificado y su token corto; «instancia» es
la librería de un proveedor configurada con sus parámetros (p. ej. un bot de Telegram); «contacto» es una persona con
sus canales; «canal» es una dirección de un contacto en un tipo de servicio, un proveedor y una instancia; «plantilla»
es lo que convierte los datos de quien llama en el modelo neutral de mensaje; «variable» es un valor de la
configuración que las plantillas usan además de esos datos; y «configuración» son las instancias, las plantillas, las
variables y los contactos que el admin aplica con la CLI de Hermes.

Actores:

- **Admin**: da de alta a Hermes en Hécate con un token de alta, guarda en Hécate el token de cada bot y las
  credenciales de la base de Hermes, aplica la configuración con la CLI de Hermes y pide los códigos de verificación.
- **Quien llama**: pide `chat.send_message`. En producción será Hades; en esta feature, un componente de prueba.
- **Contacto**: la persona que recibe los mensajes por Telegram y demuestra que su canal es suyo.
- **Telegram**: el proveedor, al que Hermes habla por su API.
- **Hécate**: da a Hermes su identidad y le resuelve sus secretos.

### User Story 1 - Enviar un mensaje a un contacto por Telegram (Priority: P1)

Quien llama pide a Hermes que avise a una persona: le da una plantilla con sus datos y el contacto al que va. Hermes
compone el mensaje, lo envía por Telegram y devuelve el resultado en el sobre común, igual que lo haría con cualquier
otro proveedor de chat.

**Why this priority**: es la primera función de Hermes y la que necesitan los Playbooks para avisar al propietario.
Prueba de extremo a extremo que Hermes habla con un tercero real con su identidad de Cerbero y sus secretos de Hécate.

**Independent Test**: con una instancia de Telegram, una plantilla y un contacto con su canal verificado, pedir el envío
y comprobar que el mensaje llega al chat del contacto, que la respuesta trae el resultado y que la llamada queda en el
registro de auditoría.

**Acceptance Scenarios**:

1. **Given** una instancia de Telegram, una plantilla y un contacto con su canal verificado, **When** quien llama pide
   `chat.send_message` con datos que cumplen el esquema de la plantilla, **Then** el contacto recibe el mensaje
   compuesto con esos datos y la respuesta dice que se completó, con la referencia del mensaje en el proveedor.
2. **Given** unos datos que no cumplen el esquema de la plantilla, o una lista de destinatarios con más de un contacto
   o ninguno, **When** se pide el envío, **Then** Hermes responde `validation` y no envía nada.
3. **Given** una plantilla, una instancia o un contacto que no existen, **When** se pide el envío, **Then** Hermes
   responde `not_found` y no envía nada.
4. **Given** un contacto cuyo canal no está verificado, **When** se pide el envío, **Then** Hermes responde `conflict`
   y no envía nada.
5. **Given** un mensaje que no cabe en el límite del proveedor, **When** se envía, **Then** el proveedor recibe el
   detalle recortado, y el título y el enlace se conservan.
6. **Given** un fallo del proveedor (no responde, rechaza la petición o la limita), **When** se pide el envío, **Then**
   la respuesta lo dice dentro del sobre, con su código (`provider_unavailable`, `rate_limited` o `timeout`) y si vale
   reintentar.
7. **Given** un envío que llega a intentarse con el proveedor, **When** termina, bien o mal, **Then** queda una entrada
   en el registro append-only con quién lo pidió, a qué contacto (por referencia), lo que cambia fuera, los datos que
   salen de casa y el resultado, sin el token del bot.

---

### User Story 2 - Atender solo a componentes autenticados (Priority: P2)

Hermes actúa sobre terceros en nombre de otros componentes, así que solo atiende llamadas que llegan por el proxy TLS
con el certificado de un componente de Cerbero y un token corto válido. Lo demás lo rechaza sin dar pistas de qué
falló.

**Why this priority**: una llamada falsa podría escribir a la familia en nombre de Cerbero o sacar datos de casa.
Además, esta base la reutilizarán los demás servicios de Cerbero en Python.

**Independent Test**: con el cliente de referencia, hacer una llamada con cada credencial inválida y comprobar que todas
reciben `auth` sin efectos, y que una llamada correcta se atiende.

**Acceptance Scenarios**:

1. **Given** una llamada sin certificado de cliente, o con un certificado caducado, revocado con urgencia o de otra CA,
   **When** llega, **Then** Hermes responde `auth` y no hace nada.
2. **Given** un token corto que falta, ha caducado, dura más de 300 s, tiene otro `aud`, tiene un `iss` distinto del
   SPIFFE ID del certificado, está firmado con una clave que Hécate no publica para esa identidad o repite un `jti` ya
   usado, **When** llega, **Then** Hermes responde `auth` y no hace nada.
3. **Given** una cabecera de certificado que el cliente envía al proxy, **When** la petición pasa por el proxy, **Then**
   Hermes solo ve el certificado que el cliente presentó en la conexión TLS.
4. **Given** la API de Hermes, **When** algo intenta llegar a ella sin pasar por el proxy, **Then** no puede.

---

### User Story 3 - Mantener su identidad y sus secretos con Hécate (Priority: P3)

Hermes se da de alta en Hécate con el token del admin, renueva solo su certificado, firma cada llamada con su token
corto, mantiene al día la lista de revocación y las claves publicadas, y resuelve el token de cada bot con caché, para
seguir funcionando un tiempo aunque Hécate esté sellada o caída.

**Why this priority**: sin identidad no habla con nadie, y sin secretos no habla con Telegram. Es además el primer
componente real que sigue la [guía de integración de Hécate](../../docs/hecate/integracion.md).

**Independent Test**: contra una Hécate de pruebas, dar un token de alta a un Hermes sin identidad, comprobar su
certificado, adelantar el reloj a los dos tercios de su vida y comprobar la renovación; sellar Hécate y comprobar que
los envíos siguen hasta el tiempo duro de la caché y fallan después.

**Acceptance Scenarios**:

1. **Given** un Hermes sin identidad, **When** el admin le da un token de alta, **Then** comprueba a Hécate con la
   huella de la CA raíz del token antes de enviarle nada, se da de alta y guarda su certificado; sus claves nunca salen
   de su equipo.
2. **Given** un certificado que llega a los dos tercios de su vida, **When** llega ese momento, **Then** Hermes lo
   renueva una vez, con claves nuevas, y el proxy pasa a usar el certificado nuevo sin reiniciarse.
3. **Given** una renovación cuya respuesta no llega, **When** Hermes vuelve a intentarlo, **Then** repite exactamente la
   misma petición y recibe el mismo certificado, sin que Hécate la tome por un segundo intento.
4. **Given** Hécate sellada o caída, **When** Hermes necesita el token de un bot, **Then** usa el que tiene en caché
   hasta una hora después de resolverlo; pasado ese tiempo, el envío falla con `internal` sin intentar llegar a
   Telegram.
5. **Given** un certificado caducado o emitido por una CA que Hécate ya sustituyó, **When** Hermes arranca o lo
   detecta, **Then** deja de estar listo y dice que necesita un token de alta nuevo.
6. **Given** la lista de revocación y las claves publicadas, **When** están a punto de caducar, **Then** Hermes las
   refresca; si Hécate no responde, sigue con su última copia.

---

### User Story 4 - Configurar Hermes y verificar el canal de un contacto (Priority: P4)

El admin aplica con la CLI de Hermes un fichero con la configuración entera: instancias, plantillas, variables y
contactos. Después pide el código de un solo uso de cada canal nuevo, y la persona demuestra que el canal es suyo
enviándoselo al bot. Mientras no lo demuestra, el canal no recibe mensajes.

**Why this priority**: sin configuración, Hermes no tiene a quién ni cómo enviar, y un mensaje a una dirección
equivocada saca datos de casa. Además, Telegram no deja que un bot escriba primero a quien no le ha escrito antes.

**Independent Test**: aplicar un fichero con una instancia, una plantilla y un contacto; comprobar que un envío al
contacto se rechaza; verificar su canal con el código y comprobar que el envío llega; aplicar ficheros con errores y
comprobar que no cambian nada.

**Acceptance Scenarios**:

1. **Given** un fichero de configuración válido, **When** el admin lo aplica, **Then** la CLI enseña lo que se va a
   crear, cambiar y borrar, pide confirmación y lo escribe todo en una sola transacción; aplicar otra vez el mismo
   fichero no cambia nada.
2. **Given** un fichero con un error (no cumple el esquema, tiene una referencia entre plantillas, variables,
   instancias y contactos que no existe, o lleva un valor donde va un `secret://`), **When** se aplica, **Then** no se
   escribe nada y la CLI dice qué falla.
3. **Given** un fichero que añade o cambia un `secret://`, **When** se aplica, **Then** Hermes comprueba que el secreto
   existe en Hécate y que puede leerlo; si no es así, o no puede comprobarlo, no se escribe nada y la CLI dice cuál.
4. **Given** la configuración en la base, **When** el admin la exporta, **Then** recibe un fichero sin secretos ni
   direcciones de contactos que, aplicado, no cambia nada.
5. **Given** un canal sin verificar, **When** el admin pide su código, **Then** Hermes da un código de un solo uso con
   su caducidad.
6. **Given** ese código, **When** la persona se lo envía al bot desde su cuenta de Telegram antes de que caduque,
   **Then** Hermes liga el canal a esa cuenta, lo marca como verificado y le contesta que el canal quedó verificado.
7. **Given** un código caducado, ya usado o que no corresponde a ningún canal pendiente, **When** llega al bot, **Then**
   no verifica nada ni recibe respuesta.
8. **Given** un canal verificado, **When** el admin le cambia la instancia o pide para él un código nuevo, **Then** el
   canal vuelve a quedar sin verificar hasta que llegue el código nuevo.

---

### User Story 5 - Repetir sin duplicar y reintentar solo lo seguro (Priority: P5)

Quien llama puede repetir una petición sin miedo a mandar dos veces el mismo mensaje, y Hermes solo reintenta por su
cuenta cuando sabe que el mensaje no salió.

**Why this priority**: un aviso duplicado confunde, y un reintento a ciegas tras un corte puede repetir un efecto en un
tercero.

**Independent Test**: repetir una petición con la misma clave, también a la vez y con un corte simulado entre el envío y
su registro, y comprobar que sale un solo mensaje; con el proveedor fake, provocar fallos de transporte antes y después
de enviar y comprobar cuándo reintenta Hermes y cuándo devuelve un resultado ambiguo.

**Acceptance Scenarios**:

1. **Given** una petición atendida, **When** llega otra con la misma `Idempotency-Key` y el mismo cuerpo, **Then**
   Hermes devuelve el mismo resultado sin volver a enviar.
2. **Given** una `Idempotency-Key` ya usada, **When** llega con otro cuerpo, **Then** Hermes responde `conflict`.
3. **Given** dos peticiones iguales que llegan a la vez, **When** las atiende, **Then** sale un solo mensaje.
4. **Given** un fallo de transporte que garantiza que el mensaje no salió, **When** ocurre, **Then** Hermes reintenta
   con esperas crecientes y aleatorias, respeta el tiempo de espera que pida el proveedor y no pasa del timeout técnico
   de la función.
5. **Given** un fallo después de enviar, del que no se sabe si el mensaje salió, **When** ocurre, **Then** Hermes no
   reintenta y responde `timeout` con `retryable` falso y `outcome_unknown` verdadero.
6. **Given** una instancia que llega a su límite de uso, **When** llegan más peticiones, **Then** Hermes responde
   `rate_limited` con el tiempo de espera, sin llegar al proveedor.

---

### User Story 6 - Saber qué ofrece Hermes y si está sano (Priority: P6)

Quien llama sabe qué tipos de servicio, proveedores, instancias y funciones ofrece Hermes y en qué estado está cada
instancia, y quien lo opera sabe si Hermes está vivo y listo.

**Why this priority**: Hades elegirá las instancias por su nombre y avisará cuando una caiga; sin `/v1/capabilities` no
sabría qué hay ni si funciona.

**Independent Test**: consultar `/v1/capabilities` con una instancia sana y otra con su token revocado en Telegram;
lanzar la prueba de conexión de cada una; consultar la salud por el canal local con y sin identidad, base y
configuración.

**Acceptance Scenarios**:

1. **Given** Hermes en marcha, **When** quien llama consulta `/v1/capabilities`, **Then** recibe, por tipo de servicio,
   sus proveedores, sus instancias y sus funciones con su modo, y el estado de cada instancia (`healthy`, `degraded` o
   `unavailable`).
2. **Given** una instancia, **When** se lanza su prueba de conexión, **Then** Hermes comprueba con el proveedor que sus
   credenciales funcionan y actualiza su estado.
3. **Given** una instancia cuyas últimas llamadas fallan, **When** se consulta `/v1/capabilities`, **Then** su estado es
   `degraded` o `unavailable`.
4. **Given** Hermes sin un certificado válido, sin su base o sin poder leer su configuración, **When** se consulta su
   salud por el canal local, **Then** dice que no está listo y por qué, y aparte, que el proceso responde.

---

### Edge Cases

- **Hécate sellada o caída**: Hermes sigue con los secretos de su caché hasta el tiempo duro y con su certificado
  mientras no caduque; después, lo que necesita un secreto falla con `internal`. Si Hécate deja de responder, Hermes
  sigue con su última lista de revocación y sus últimas claves publicadas.
- **Hermes parado más de lo que dura su certificado** (3 días por defecto) o **CA de Hécate sustituida**: ya no puede
  renovar; al arrancar, no está listo hasta que el admin le da un token de alta nuevo.
- **Corte entre el envío y su registro**: una repetición con la misma `Idempotency-Key` no vuelve a enviar; si Hermes no
  sabe si el mensaje salió, responde `timeout` con `outcome_unknown`.
- **Telegram limita** con un `429` y su tiempo de espera: si la espera cabe en el timeout técnico, Hermes reintenta tras
  ella; si no, el sobre responde `rate_limited` con ese tiempo, y vale reintentar.
- **Telegram rechaza el token del bot** (revocado o cambiado fuera de Cerbero): el envío falla con el error del
  proveedor dentro del sobre, y la instancia pasa a `unavailable` hasta que su prueba de conexión vuelva a funcionar.
- **La persona bloquea al bot**: el envío falla con el error del proveedor dentro del sobre.
- **Un contacto con varios canales verificados de chat**: quien llama indica la instancia; si no la indica y hay más de
  uno, Hermes responde `validation`.
- **Una plantilla con varias versiones**: quien llama puede pedir una versión; sin versión, se usa la última.
- **Un mensaje más largo que el límite del proveedor**: se recorta el detalle, y el título y el enlace se conservan.
- **Una petición con efecto sin `Idempotency-Key`**: Hermes responde `validation`.
- **La base de datos no responde**: Hermes deja de estar listo y responde `unavailable`, que vale reintentar, sin
  llegar al proveedor: no envía nada que no pueda registrar.
- **Un fichero que quita un contacto o un canal**: al aplicarlo se borran con su verificación, y la CLI lo enseña
  entre los cambios antes de pedir confirmación.
- **Una aplicación de la configuración durante un envío**: cada envío usa la configuración de antes o la de después,
  nunca una mezcla.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hermes DEBE atender las operaciones de su API solo a través de un proxy TLS que termina el mTLS con los
  certificados de Cerbero y le reenvía el certificado del cliente. Hermes NO DEBE aceptar conexiones de red de otro
  origen: su puerto no se publica fuera de la red interna entre el proxy y Hermes.
- **FR-002**: El proxy DEBE sustituir cualquier cabecera de certificado que llegue del cliente por la del certificado
  que el cliente presentó en la conexión, tras comprobar que tiene su clave (la validez la comprueba Hermes, FR-003), y
  DEBE pasar a usar el certificado que Hermes renueva sin reiniciarse ni cortar las conexiones en curso.
- **FR-003**: Toda operación de la API de Hermes DEBE exigir el certificado reenviado y un token corto en
  `Authorization`, y Hermes DEBE comprobar sobre ellos lo que fija la guía de integración de Hécate:
  - que la cadena del certificado llega a la CA raíz vigente de Hécate y que el certificado no está caducado ni en la
    lista de revocación;
  - que el token lo firmó una clave de firma de llamadas que Hécate publica para esa identidad;
  - que su `iss` es el SPIFFE ID del certificado y su `aud`, el de Hermes;
  - que dura 300 s como mucho, con 30 s de margen entre relojes, y que su `jti` no se ha usado antes con el mismo `iss`.

  Cualquier fallo DEBE responder `auth`, sin efectos y sin decir qué comprobación falló.
- **FR-004**: Hermes DEBE atender a todo componente de Cerbero que pase la autenticación de FR-003; la comprobación del
  acceso por app queda fuera de esta feature, porque Hécate no guarda qué identidades pueden llamar a cada app. Hermes
  NO DEBE tomar ningún permiso de lo que declare el token corto, que firma quien llama.
- **FR-005**: Hermes DEBE darse de alta en Hécate con el token de alta que crea el admin: comprueba a Hécate con la
  huella de la CA raíz del token antes de enviarle nada, genera en su equipo su clave TLS y su clave de firma de
  llamadas, que nunca salen de él, y las guarda con su certificado en un sitio que solo lee su usuario.
- **FR-006**: Hermes DEBE renovar su certificado a los dos tercios de su vida, una sola vez por certificado y con claves
  nuevas. Si la respuesta no llega, DEBE repetir exactamente la misma petición; si Hécate responde `unavailable` o no
  acepta la conexión, DEBE conservar el certificado actual y reintentar más tarde.
- **FR-007**: Hermes DEBE firmar cada llamada a Hécate con un token corto nuevo, con el formato de la guía de
  integración.
- **FR-008**: Hermes DEBE refrescar la lista de revocación y las claves de firma de llamadas que publica Hécate antes de
  que caduquen, y DEBE seguir con su última copia mientras Hécate no responda.
- **FR-009**: Hermes DEBE resolver en Hécate sus secretos (los de sus instancias y las credenciales de su base) y
  guardarlos solo en memoria, nunca en disco, logs, eventos ni respuestas. DEBE usar cada valor sin volver a pedirlo
  durante su tiempo blando; si Hécate no responde o está sellada, DEBE seguir con el valor guardado hasta su tiempo
  duro; después, lo que lo necesite DEBE fallar con `internal` sin llegar al proveedor.
- **FR-010**: Sin un certificado válido (caducado, revocado o de una CA sustituida), Hermes NO DEBE estar listo, y DEBE
  decir que necesita un token de alta nuevo.
- **FR-011**: Toda respuesta de Hermes DEBE ir en el sobre común de Cerbero: `status`, `data`, `error` y `meta` (con
  `request_id`, el tipo de servicio, el proveedor, la `Idempotency-Key` y la duración). Los códigos de error DEBEN ser
  los cerrados del contrato (`validation`, `auth`, `not_found`, `conflict`, `rate_limited`, `provider_unavailable`,
  `timeout`, `unavailable` e `internal`), cada uno con `retryable`. Un fallo del proveedor DEBE ir dentro del sobre; los
  errores HTTP de Hermes son solo para fallos del propio Hermes o de la petición.
- **FR-012**: La versión mayor DEBE ir en la ruta (p. ej. `/chat/v1/send_message`). Hermes DEBE publicar su API como un
  contrato OpenAPI en shared/contracts, con el contrato común del tipo `chat` y el del proveedor, y cada respuesta,
  también las de error, DEBE cumplirlo.
- **FR-013**: Hermes DEBE ofrecer `/v1/capabilities`, con sus tipos de servicio, y para cada uno sus proveedores, sus
  instancias, sus funciones con su modo y el estado de cada instancia (`healthy`, `degraded` o `unavailable`), calculado
  de sus últimas llamadas y de su prueba de conexión.
- **FR-014**: Hermes DEBE ofrecer la prueba de conexión de una instancia, que comprueba con el proveedor que sus
  credenciales funcionan.
- **FR-015**: Hermes DEBE ofrecer su salud por el canal local de administración: si el proceso responde y si está listo
  (con un certificado válido, su base y su configuración), sin datos de sus instancias ni de sus contactos. La API de
  red NO DEBE ofrecerla.
- **FR-016**: Las peticiones con efecto DEBEN llevar una `Idempotency-Key`. La misma clave con el mismo cuerpo DEBE
  devolver el mismo resultado sin repetir el efecto, también si llegan a la vez; con otro cuerpo, DEBE responder
  `conflict`, como fija el contrato común. Si un corte deja sin saber si el efecto ocurrió, la repetición NO DEBE
  volver a ejecutarlo: DEBE responder `timeout` con `outcome_unknown`.
- **FR-017**: Hermes DEBE ofrecer la categoría `communication`, con el tipo de servicio `chat` y su función síncrona
  `send_message`, de tipo `command`, con su `side_effect` (qué mensaje se envía y a quién) y su `discloses_data` (qué
  datos llegan al proveedor) descritos en su contrato.
- **FR-018**: `send_message` DEBE recibir los destinatarios como referencias a contactos, una plantilla con su versión
  opcional y los datos que la rellenan, y opcionalmente la instancia. DEBE validar los datos contra el esquema de la
  plantilla, componer con ella, los datos y las variables de la configuración el modelo neutral de mensaje (título,
  severidad, resumen, hechos, acciones hechas, qué se espera y enlace) y dibujarlo según las capacidades del proveedor,
  recortando el detalle si no cabe y conservando el título y el enlace.
- **FR-019**: Las instancias DEBEN llevar sus parámetros y referencias `secret://hermes/<proveedor>/<instancia>/<campo>`
  a sus secretos, nunca su valor. Puede haber varias por proveedor, y quien llama las elige por su nombre.
- **FR-020**: Los contactos DEBEN tener uno o varios canales, cada uno con su tipo de servicio, su proveedor, su
  instancia, la dirección que le da la verificación y si está verificado, y un canal sin verificar NO DEBE recibir
  mensajes (`conflict`).
- **FR-021**: `send_message` DEBE ir a un solo contacto por petición: la lista de destinatarios DEBE llevar exactamente
  uno, y con más o con ninguno Hermes DEBE responder `validation` sin enviar nada.
- **FR-022**: Un canal de Telegram DEBE verificarse con un código de un solo uso que caduca: el admin lo pide con la
  CLI, la persona se lo envía al bot desde su cuenta, y Hermes, al recogerlo del proveedor, liga el canal a esa cuenta,
  lo marca verificado y le contesta que el canal quedó verificado. Un código caducado, usado o desconocido NO DEBE
  verificar nada ni recibir respuesta, y cambiar la instancia de un canal verificado o pedir para él un código nuevo
  DEBE dejarlo sin verificar.
- **FR-023**: Hermes DEBE guardar su configuración y lo que escribe al funcionar (la verificación de los canales, las
  claves de idempotencia y el registro de auditoría) en su base de datos, en el PostgreSQL común de Cerbero, con su
  propio usuario y sin acceso a las bases de las demás apps. Las credenciales de la base DEBEN ser un secreto de Hermes
  en Hécate, y en la base NO DEBE haber ningún secreto: solo sus referencias `secret://`.
- **FR-024**: El admin DEBE gestionar la configuración con una CLI de Hermes que le habla por un canal local de
  administración, sin superficie de red y con el sobre y los códigos de error comunes, como fija la constitución; la
  API de red de Hermes NO DEBE ofrecer esa gestión. La CLI DEBE aplicar un fichero que describe la configuración
  entera: Hermes lo valida completo (su esquema, las referencias entre plantillas, variables, instancias y contactos, y
  que cada `secret://` que añade o cambia existe en Hécate y puede leerlo), enseña lo que va a crear, cambiar y borrar,
  pide confirmación y lo escribe en una sola transacción, o nada si algo falla.
- **FR-025**: Aplicar dos veces el mismo fichero NO DEBE cambiar nada, y aplicar NO DEBE tocar la verificación de los
  canales que no cambian. Los campos de secreto del fichero solo DEBEN aceptar referencias `secret://`. La CLI DEBE
  exportar la configuración vigente en el mismo formato, sin secretos ni direcciones de contactos, y dar el código de
  verificación de un canal, que solo enseña a quien lo pide.
- **FR-026**: La librería de Telegram DEBE declarar en su manifiesto su proveedor y su versión, los tipos de servicio y
  las funciones que implementa con su modo y su timeout técnico, sus capacidades (longitud máxima y formatos), sus
  límites de uso, los permisos mínimos que necesita en el proveedor y la configuración de sus instancias.
- **FR-027**: Ante un fallo de transporte que garantiza que la petición no se ejecutó, Hermes DEBE reintentar con
  esperas crecientes y aleatorias, respetando el tiempo de espera que pida el proveedor, sin pasar del timeout técnico
  de la función. Si no sabe si la petición se ejecutó, NO DEBE reintentar: DEBE responder `timeout` con `retryable`
  falso y `outcome_unknown` verdadero.
- **FR-028**: Cada instancia DEBE tener un límite de uso con los valores de su librería, ajustables en la instancia; al
  superarlo, Hermes DEBE responder `rate_limited` con el tiempo de espera, sin llegar al proveedor.
- **FR-029**: Cada envío que llega a intentarse con el proveedor DEBE dejar una entrada en un registro append-only:
  cuándo, quién llama, la función, la instancia, los destinatarios por referencia, el texto resuelto de `side_effect` y
  de `discloses_data`, el contexto de auditoría que trae la llamada, la `Idempotency-Key` y el resultado. También
  DEBEN dejarla cada aplicación de la configuración, con lo que cambió, cada código de verificación pedido y cada canal
  verificado. El registro NO DEBE llevar secretos, códigos de verificación ni direcciones de contactos.
- **FR-030**: Los logs de Hermes NO DEBEN llevar secretos, códigos de verificación ni direcciones de contactos, y las
  direcciones de los contactos solo DEBEN vivir en la base de datos de Hermes.
- **FR-031**: Hermes DEBE tener un proveedor fake del tipo `chat`, que cumple su contrato, para los entornos funcionales
  y la CI de otros componentes, y una batería de conformidad del tipo `chat` que el fake y la librería de Telegram (con
  respuestas grabadas del proveedor) DEBEN pasar.
- **FR-032**: Al cerrar el cambio, DEBEN consolidarse docs/hermes.md y lo que cambie de ARCHITECTURE, GLOSSARY y
  DECISIONS, y la decisión abierta «APIs internas en Python y certificado del cliente» DEBE pasar a un ADR, como fija la
  constitución. También DEBE consolidarse la decisión del acceso por app (Clarifications): en un ADR; en ARCHITECTURE,
  donde el token corto deja de llevar los permisos que conceden el acceso; en el diseño de la Access Policy de Hécate;
  y en DECISIONS, como diseñado y no implementado. Y la de la persistencia (Clarifications): un ADR recoge que Hécate
  sigue con SQLite y que las demás apps usan un PostgreSQL común, con una base y un usuario propios por app y sin
  acceso a las de las demás, y resuelve la decisión abierta «Persistencia por proyecto», en la que el buffer de las
  cabezas sigue en cada cabeza; y docs/hermes.md, ARCHITECTURE y DECISIONS («Repositorio privado de lo propio»)
  recogen que las instancias y las plantillas de Hermes viven en su base y que el repositorio privado se queda con las
  librerías propias.

### Key Entities *(include if feature involves data)*

- **Instancia**: una librería de proveedor configurada. Nombre, proveedor, parámetros, referencias a sus secretos,
  límites de uso y estado (`healthy`, `degraded` o `unavailable`).
- **Plantilla**: identificador, versión, esquema de los datos que acepta y cómo rellena el modelo neutral de mensaje.
- **Variable**: un nombre y su valor en la configuración, que las plantillas usan además de los datos de quien llama
  (p. ej. la dirección base de los enlaces).
- **Mensaje neutral**: título, severidad, resumen, hechos, acciones hechas, qué se espera y enlace; cada proveedor lo
  dibuja según sus capacidades.
- **Contacto y canal**: una persona con sus canales; cada canal, con su tipo de servicio, proveedor, instancia, la
  dirección que le da la verificación, si está verificado y, mientras no lo está, su código de verificación pendiente.
- **Entrada registrada**: la entrada append-only de un envío (quién lo pidió, a quién iba, qué cambió fuera, qué salió
  de casa y su resultado), de una aplicación de la configuración (qué cambió) o de la verificación de un canal (código
  pedido o canal verificado), sin secretos, códigos ni direcciones.
- **Fichero de configuración**: la configuración entera que el admin aplica con la CLI y que la exportación devuelve,
  sin secretos (solo `secret://`) ni direcciones de contactos, y fuera de cualquier repositorio.
- **Clave de idempotencia**: la clave, lo que identifica el cuerpo de la petición, si su efecto está en curso o
  terminado, y su resultado.
- **Identidad de Hermes**: su certificado y su cadena, su clave TLS, su clave de firma de llamadas y, mientras dura una
  renovación, la petición en curso.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Con Telegram disponible, el 95 % de los mensajes pedidos a un contacto verificado llegan a su chat en
  menos de 5 segundos, medido en un recorrido real con al menos 20 envíos.
- **SC-002**: El 100 % de las llamadas con alguna credencial inválida de la historia 2 reciben `auth`, sin enviar nada
  ni dejar efectos.
- **SC-003**: 100 repeticiones de una misma petición con la misma `Idempotency-Key`, también a la vez y con un corte
  simulado entre el envío y su registro, producen como mucho un mensaje.
- **SC-004**: Con Hécate sellada, los envíos siguen funcionando durante la hora siguiente a la última resolución de su
  secreto y, después, fallan con `internal` sin llegar al proveedor.
- **SC-005**: Con el reloj adelantado, Hermes encadena al menos 15 renovaciones seguidas, 30 días con certificados de 3
  días, sin intervención y sin que Hécate detecte un segundo intento.
- **SC-006**: El proveedor fake y la librería de Telegram pasan el 100 % de la batería de conformidad del tipo `chat`.
- **SC-007**: Ningún token de bot, código de verificación ni dirección de contacto aparece en los logs, en las
  respuestas de error, en el registro de auditoría ni en la configuración exportada, buscados en todas sus
  codificaciones.
- **SC-008**: Ningún canal sin verificar recibe un mensaje.
- **SC-009**: El 100 % de las respuestas de Hermes, también las de error, cumplen su contrato publicado.
- **SC-010**: El 100 % de los ficheros de configuración con alguno de los errores de la historia 4 se rechazan sin
  cambiar nada de la base, y volver a aplicar un fichero ya aplicado no produce ningún cambio.

## Assumptions

- **Base**: `main` tras el cambio 007 (`7c54d4e`). Hécate está construida con su API mTLS: alta, renovación, token
  corto, lista de revocación, claves publicadas, resolución con caché y estado, según su guía de integración, con su
  componente de prueba como cliente de referencia.
- **Quien llama**: Hades no existe; en esta feature, las pruebas usan un cliente de referencia que se autentica como un
  componente de Cerbero.
- **Secretos**: el admin guarda en Hécate, bajo `secret://hermes/`, el token de cada bot y las credenciales de la base
  de Hermes, y la regla por defecto de la Access Policy deja a Hermes resolverlos; no hace falta ningún permiso del
  admin.
- **Acceso por app**: en esta feature, cualquier componente que el admin haya dado de alta en Hécate puede llamar a
  Hermes. La decisión está tomada (Clarifications): Hécate guarda, de forma central, qué identidades pueden llamar a
  cada app, por app entera y sin permisos por función, y el token corto no concede el acceso. Su parte de Hécate la
  aborda la 009.
- **Pila y despliegue**: Hermes se escribe en Python ([ADR 0005](../../docs/adr/0005-rust-para-orthros-y-hecate.md)). El
  framework, el proxy, la estructura de la base y cómo se despliega se deciden en el plan. Su ADR recoge la decisión del
  propietario (la API detrás de un proxy TLS que reenvía el certificado del cliente) con sus condiciones, entre ellas el
  riesgo aceptado de que el token corto se ligue a la identidad que declara el proxy, y va de la mano de la decisión
  abierta del despliegue en el home lab.
- **Persistencia**: hay un PostgreSQL común para las apps de Cerbero; en las pruebas, uno de usar y tirar. Montarlo en
  el home lab va con la decisión abierta del despliegue.
- **Nombres concretos**: el sobre, los códigos de error, las rutas y el OpenAPI de shared/contracts aparecen porque son
  el contrato común de Cerbero (ARCHITECTURE y docs/hermes.md), no una elección de esta feature.
- **Verificación del canal**: como Telegram no deja que un bot escriba primero, la persona envía el código al bot, y
  Hermes lo recoge consultando al proveedor, como todo lo que llega de él.
- **Idempotencia**: Hermes conserva las claves de idempotencia y sus resultados al menos 24 horas.
- **Una sola instancia de Hermes**, sin alta disponibilidad, como el resto de Cerbero.
- **Registro de auditoría**: append-only y en la base de Hermes; su firma de checkpoints queda fuera, porque Hécate no
  la ofrece.
- **Fuera de alcance**: las tasks y `chat.ask`, que van a la segunda feature; los grupos de contactos (`shared_chat` y
  `fan_out`) y los envíos a varias personas en una sola petición; otras categorías, tipos de servicio y proveedores; la
  gestión desde Ténaro y las librerías propias del repositorio privado; las copias de seguridad cifradas con
  `wrap`/`unwrap`; la firma de checkpoints; los permisos que salen de las declaraciones; la comprobación del acceso por
  app; los usuarios, los roles y el perfilado dentro de cada app; y OpenTelemetry, que sigue como decisión común
  abierta.
- **Rama**: `feature/008-hermes-send-message`, creada desde `main`; commit de squash `feat:`.
