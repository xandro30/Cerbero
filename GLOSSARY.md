# Glosario — Cerbero

Lenguaje ubicuo del sistema. Los términos se mantienen en inglés en código y documentación técnica; la definición se da en español.

Ver también: [README](README.md) · [ARCHITECTURE](ARCHITECTURE.md)

| Term | Contexto | Significado |
|---|---|---|
| **Raw Log** | Cabezas | Dato original tal como lo produce la fuente. Se parsea en origen y se descarta: no viaja a Estigia. |
| **UDM Event** | Todos | Hecho del mundo normalizado al esquema común. Inmutable. |
| **Entity** | Estigia | Asset, zona o usuario que Estigia reconcilia a partir de múltiples identificadores. |
| **Asset** | Estigia | Entity que representa un dispositivo (móvil, portátil, sensor, router...). |
| **Context** | Estigia, Hades | Estado de las Entities en un momento dado. El contexto permanente (trust level, anotaciones) solo lo modifica Hades; el estado de la casa y sus zonas sale de los eventos de Echidna. |
| **Quarantine** | Estigia | Destino de los eventos que no cumplen el esquema UDM. |
| **Entity Resolution** | Estigia | Proceso que reconcilia identificadores observados (MAC, IP, hostname...) en una Entity única. |
| **Validity Interval** | Estigia | Ventana temporal (desde/hasta) en la que un dato de contexto de una Entity fue cierto. |
| **Trust Level** | Estigia, Hades | Grado de confianza de un Asset (desconocido, conocido...). Es conocimiento permanente, sin caducidad: lo temporal va a Data Tables con TTL. Solo Hades lo modifica. |
| **Enrichment** | Estigia | Añadir contexto a un UDM Event. Tres niveles: *core* (síncrono, en la ingesta), *extended* (asíncrono, servicios externos) y *batch* (carga programada de feeds). |
| **IOC** | Estigia, Caronte | *Indicator of Compromise*: dato concreto que indica amenaza (dominio de phishing, IP maliciosa, hash de un binario). |
| **CVE** | Estigia | *Common Vulnerabilities and Exposures*: identificador público de una vulnerabilidad de software. |
| **Feed** | Estigia | Fuente externa periódica de IOCs, CVEs u otra inteligencia, cargada por procesos batch. |
| **Reference Data** | Estigia | Inteligencia externa almacenada localmente a partir de Feeds. Consultable en el enrichment core. |
| **IOC Matching** | Estigia | Cruce de UDM Events con IOCs conocidos. Al cargar IOCs nuevos se aplica también a eventos recientes. |
| **Data Table** | Estigia, Caronte, Hades | Tabla de varias columnas fuera del UDM para registros temporales o de duración incierta (p. ej. white-list de invitados), que Caronte y Hades relacionan con eventos por valores coincidentes. La escriben el propietario y Hades. TTL opcional por tabla y por fila (recomendable si se conoce la duración) e historial de cambios. |
| **Rule** | Caronte | Texto YARA-L que Caronte evalúa sobre UDM Events, Detections y Context. Describe qué detecta; cada cambio genera una versión. Su `rule_id` es estable entre versiones. |
| **Rule Deployment** | Caronte | Cómo se ejecuta una Rule: `status`, flags `detection` y `alerting`, y estados degradada o limitada. Cambiarlo no genera versión. |
| **Rule Status** | Caronte, Hades | Ciclo de vida de una Rule: `develop`, `test` o `production`. Llega a Hades como `mode` de la Alert. |
| **single-event / multi-event** | Caronte | Rule sin sección `match` (un solo evento) o con ella (varios eventos en una ventana: umbral, secuencia, "A sin B"). |
| **Detection** | Caronte, Estigia | Hecho inmutable: una Rule se cumplió. Es un conteo útil que se reingesta en Estigia y pueden consumir las compuestas. No implica actuar. |
| **Composite Rule** | Caronte | Rule multi-event cuyas entradas son Detections de otras Rules, referenciadas por `rule_id`. |
| **Weak Signal** | Caronte | Tag de `meta` de las Rules que solo emiten Detections, porque por sí solas no justifican una Alert. Es descriptivo. |
| **Pivot** | Caronte | Evento al que se ancla una ventana `before` o `after` en una Rule. |
| **Bomb** | Caronte | Modelo de ejecución de "A sin B": A arma un temporizador; al vencer, si no hay B en el plazo, explota y genera la Detection. |
| **Grace Period** (`G`) | Caronte | Mecha extra de una Bomb para absorber retrasos normales. Configurable por Rule. |
| **Lateness** (`L`) | Caronte | Retraso máximo (`ingest_time − event_time`) con el que un participante todavía dispara evaluaciones. |
| **Detection Invalidated** | Caronte, Hades | Hecho que anula una Detection ya emitida (p. ej. llega tarde el B de una Bomb que ya explotó). No modifica la Detection. |
| **Degraded** | Caronte | Estado de una Composite Rule cuando una Rule que referencia está desactivada, limitada o filtrada por `status`. |
| **Suppression Window** | Caronte | Opción YARA-L con la que el autor de una Rule agrupa las Detections repetidas de una misma clave en bloques de tiempo. Pensada para ráfagas. |
| **Detection Quota** | Caronte | Máximo de Detections por Rule y día natural. Al superarlo, la Rule queda **limitada** hasta el día siguiente. |
| **Retention Policy** | Caronte, Estigia | Tiempo que Estigia retiene en la cache de Detections las de cada `rule_id`. La calcula y publica Caronte. |
| **Detection Cache** | Estigia | Cache derivada y reconstruible de Detections recientes que acelera las ventanas de Caronte. |
| **Retrohunt** | Caronte | Ejecución bajo demanda de una Rule sobre un rango temporal pasado. |
| **Alert** | Caronte, Hades | Notificación a Hades de una Detection sobre la que actuar. Lleva solo lo necesario para enrutar; el detalle se consulta en Estigia. Indica que algo merece revisión, no que sea un ataque. |
| **Alert Validity** | Hades | Dimensión de la Alert aparte de su ciclo de vida (`received → grouped → reviewed`): `valid`, `discarded` (desestimada por una revisión) o `withdrawn` (retirada por el sistema de forma tardía). |
| **Case** | Hades | Agrupación de las Alerts de una Rule con el mismo Grouping Key, que se resuelven con un único Verdict. Unidad de investigación. Status `open / closed`. |
| **Grouping Key** | Caronte, Hades | Outcome reservado `$grouping_key` de una Rule: un `entity_id` o una tupla ordenada de `entity_id`. Junto con el `rule_id`, decide a qué Case va una Alert. |
| **Related Cases** | Hades | Referencias de un Case a otros: el anterior con la misma clave (`same_key`) o similares (`similar`). |
| **Intake Cut** | Hades | Corte de entrada del Case (`CaseIntakeClosed`): ocurre al entrar en `intervention` y es irreversible. Las Alerts siguientes van a un Case nuevo. |
| **Stage** | Hades | Punto del trabajo en el que está una Response (`triage`, `investigation`, `intervention`, `awaiting`, `error`). Vive en la Response; el Case solo refleja el corte de entrada. |
| **Silence** | Hades | Modo silencio de un Case: sigue recibiendo Alerts, pero sus reacciones no se ejecutan. Con fin y motivo obligatorios. |
| **SLA** | Hades | Plazo opcional de una Alert, un Case, un Stage o una espera. Al vencer emite `SlaBreached`, una señal, no un error. |
| **Verdict** | Hades | Conclusión sobre un Case (disposition): `true_positive` (True Positive, amenaza real), `benign_positive` (Benign Positive: la Rule acertó pero es legítimo), `false_positive` (False Positive: la Rule se equivocó) o `undetermined` (Undetermined: sin datos para concluir). Registra quién lo dio (`by`: persona, automatización o sistema externo). |
| **Close Reason** | Hades | Motivo de cierre de un Case: `verdict` (con Verdict), `discarded` o `withdrawn` (sin Verdict). |
| **Root Cause** | Hades | Origen técnico de lo ocurrido, registrado al cerrar el Case junto al Verdict. |
| **Playbook** | Hades | Definición declarativa y versionada de una respuesta: trigger, caminos y bloques. El **principal** lo dispara la Rule y abre, corta y cierra el Case; un **secundario** lo dispara un evento. |
| **Response** | Hades | Una ejecución de un Playbook (Process Manager), con la versión fijada al arrancar y un `subject` (hoy, el Case). |
| **Entry Alert** | Hades | Camino de la Response principal que evalúa cada Alert que entra en el Case antes del corte. |
| **Awaiting** | Hades | Bloque que espera a varios `handle` (`poll`) sin recoger datos y devuelve sus estados en orden. Vencer su SLA es una señal y la espera continúa; el `timeout` es el límite duro. |
| **API Contract** | Hades, Hermes, Estigia, Caronte | Contrato de un bloque o de una función: entradas, salidas, si es consulta o cambio, side effect, compensación, datos que salen a terceros, eventos que emite, timeout técnico y reintentos. Se valida al registrar. |
| **Side Effect** | Hades, Hermes | Lo que un bloque o una función cambia fuera (una fila de Data Table, un trust level, un dispositivo bloqueado, un ticket), frente a lo que solo lee. En el contrato se describe con la entrada, p. ej. "Bloquea la URL `${input.url}` en el proxy". |
| **Discloses Data** | Hermes, Hades | Marca descriptiva de una función que envía a un tercero datos que dejan de ser privados (p. ej. una URL a un análisis público). No cambia nada, pero el dato sale de casa. |
| **Compensation** | Hades | Acción inversa que un bloque con side effect declara para deshacerlo. Algunos efectos no son reversibles. |
| **Idempotency Key** | Hades, Estigia, Hermes | `response_id + paso + hash(input resuelto)`. Quien ejecuta el efecto la comprueba para no repetirlo en un reintento. |
| **Non-repudiation** | Hades | Atribución de todo cambio: al sistema y al owner / maintainer del Playbook o Job en una ejecución automática, a la persona en una acción humana. |
| **Job / Cronjob** | Hades | Código de una integración que se ejecuta fuera de los Playbooks, a mano o por un evento; el cronjob, por calendario. Declara sus entradas y sus side effects. |
| **Source** | Hades, Estigia | Origen de un cambio: `response`, `job`, `cronjob` u `owner`, con su id. |
| **Signal** | Hades | Evento de categoría `signal`: un hecho que importa sin ser estado ni error. Los usuarios lo emiten con `hades.signal` (`stream`, `payload`, `reason`); no tiene efectos propios, los provocan sus suscriptores. |
| **Reason / Purpose** | Hades | `reason`: por qué se emite una señal. `purpose`: qué se quiere conseguir con una suscripción como proceso completo. |
| **Event Category** | Hades | Clasificación de todo evento de Hades: `lifecycle`, `progress`, `error` o `signal`, más una subcategoría. Permite buscar en `event-log`. |
| **Origin** | Hades | Procedencia de un evento: un componente de Cerbero o un servicio externo (`{type, name}`). |
| **Destino.verbo** | Hades | Forma de los bloques que hablan con un componente (`hermes`, `hades`, `caronte`, `estigia`): `query` y `command` (síncronos), `submit.query` y `submit.command` (devuelven un `handle`), `submit.result` (recoge), `dispatch` (manda y se desentiende) y `signal` (avisa). |
| **Handle** | Hades, Hermes, Caronte | Id que devuelve un `submit` y con el que `submit.result` o `awaiting` recogen o esperan su resultado. Siempre se usa: se recoge, se espera o se pasa a otro bloque. |
| **Integration** | Hades | Código Python con un manifiesto: clases de Hermes que usa (`use`) o extiende (`extends`), `env`, `config`, Managers, clases expuestas, actions, Jobs y cronjobs. Las nuestras son de solo lectura. |
| **Action** | Hades | Código propio de una integración que hace una sola cosa (SRP), con su API Contract. |
| **Manager** | Hades | Definiciones de clases y su jerarquía dentro de una integración, sin instanciar nada a nivel de módulo. Solo lo que declara `exposes` se puede importar desde otra integración. |
| **Config / Env** | Hades | `config`: valores fijos de una integración, sin lógica. `env`: lo que cambia por despliegue (instancia, secretos como referencias). |
| **Deprecation** | Hades, Hermes | Marca de una versión que va a dejar de usarse, con mensaje y sustituta. Avisa sin bloquear y emite `VersionDeprecated`; no se retira mientras algo la use. |
| **Hermes** | Hermes | Componente con las librerías de terceros y sus contratos estándar por tipología. Lo recomendado para integrar un servicio externo, no un paso obligatorio. En la mitología, el mensajero entre los mundos. |
| **Envelope** | Hermes, todos | Estructura común de respuesta de las APIs internas: `status`, `data`, `error` (código común y `retryable`) y `meta`. Los códigos son cerrados: `validation`, `auth`, `not_found`, `conflict`, `rate_limited`, `provider_unavailable`, `timeout`, `unavailable` (el componente no puede atender ahora, p. ej. Hécate sellada) e `internal`. |
| **Service Type** | Hermes | Contrato común de una tipología (`chat`, `email`, `ticketing`, `threat_intel`, `network_control`...), agrupado por categoría. Es el port que implementan los proveedores. |
| **Function** | Hermes | Operación de un tipo de servicio (p. ej. `chat.ask`, `threat_intel.ip_reputation`): consulta o cambio, modo nativo (síncrona o task) y schemas. |
| **Provider Library** | Hermes | Librería que implementa uno o varios tipos de servicio para un proveedor (adapter), con su manifiesto, su contrato de proveedor y su versión propia. |
| **Instance** | Hermes | Librería de proveedor configurada (parámetros y referencias a secretos). Puede haber varias por proveedor. |
| **Capabilities** | Hermes, todos | `GET /capabilities`: qué funciones, proveedores e instancias ofrece un componente y en qué estado. Se usa para validar al registrar. |
| **Task** | Hermes | Petición asíncrona en curso, event-sourced (`accepted → running → completed / failed / timeout / cancelled`). Su estado se consulta por API, nunca escuchando su stream. |
| **Notification** | Hermes | Mensaje enviado por un canal de comunicación (chat, email, ticketing) mediante una función de su tipo de servicio. |
| **Message Model** | Hermes | Formato neutral de un mensaje (título, severidad, resumen, datos, acciones hechas, qué se espera, enlace al Case), que rellenan las plantillas y dibuja cada proveedor. |
| **Contact** | Hermes, Hades | Persona o grupo con sus canales verificados y direcciones. Hades solo maneja referencias; el directorio es de Hermes. |
| **mTLS** | Todos | TLS mutuo: cliente y servidor se identifican con certificados. Da identidad y cifra el canal entre componentes, con certificados de corta duración de una CA interna. |
| **CA interna** | Todos | Autoridad de certificados propia de Cerbero que emite los certificados de los componentes. |
| **JWT** | Todos | Token corto firmado por quien llama, con permisos y contexto de auditoría. El receptor lo verifica con la clave de firma de llamadas del emisor. |
| **Call Signing Key** | Todos, Hécate | Clave de firma de llamadas: clave Ed25519 de un componente, distinta de la de su certificado, con la que firma sus tokens cortos. Se registra en el alta y en cada renovación; Hécate la publica, solo a componentes autenticados, mientras su certificado es válido y su identidad está habilitada, y la conserva para verificar firmas pasadas. |
| **Admin** | Ténaro, Hécate | Único usuario privilegiado: inicializa, desbloquea y sella Hécate, gestiona secretos, identidades y tokens de alta, invita a las personas y tiene códigos de recuperación. No confundir con el owner / maintainer de una definición. |
| **Passkey** (WebAuthn) | Ténaro | Credencial sin contraseña, resistente al phishing, con la que una persona entra en Ténaro; el servidor solo guarda la clave pública. |
| **Ténaro** | Ténaro | Backend de Cerbero para la UI y única superficie pública: autentica a las personas con passkeys y delega en cada componente. En la mitología, la entrada al inframundo desde el mundo de los vivos. |
| **Hécate** | Hécate | Componente que guarda los secretos de terceros y gestiona las claves de Cerbero (CA interna, firmas, rotación). En la mitología, la guardiana de las llaves y las puertas del inframundo. |
| **Secret Reference** | Todos | `secret://<identidad>/<resto>` (o `@versión`): referencia a un secreto que resuelve el componente al ejecutar; nunca el valor. |
| **Scope** | Hécate | Ámbito de un secreto y de su clave de cifrado: coincide con la identidad que puede leerlo. |
| **KEK / DEK** | Hécate | *Key Encryption Key*: clave que solo envuelve otras claves (la raíz y las claves de ámbito). *Data Encryption Key*: clave aleatoria que cifra un solo dato (una versión de un secreto). |
| **Envelope Encryption** | Hécate | Cifrar cada dato con su DEK y guardar la DEK envuelta por una KEK. Rotar la KEK solo vuelve a envolver las DEK. |
| **AEAD** | Hécate | Cifrado autenticado con datos asociados (p. ej. AES-256-GCM): detecta cualquier cambio y liga el dato cifrado a su ruta, versión y ámbito. |
| **Argon2id** | Hécate | Función que deriva una clave a partir de la frase del admin, costosa a propósito para frenar ataques por fuerza bruta. |
| **Shamir** | Hécate | Reparto de un secreto en partes, de las que hacen falta un mínimo (p. ej. 2 de 3) para reconstruirlo. Se usa para recuperar la raíz. |
| **Sealed / Unsealed** | Hécate | Sellada: arranca sin la raíz en memoria, solo admite desbloquearse y responde `unavailable` a los componentes. Desbloqueada: con la raíz en memoria, sirve secretos y emite certificados. |
| **Crypto-shredding** | Hécate | Borrado criptográfico: destruir la DEK de un dato para que no se pueda descifrar, aunque sus eventos sigan existiendo. |
| **Fingerprint** | Hécate | Huella con clave de una versión de secreto (HMAC del valor con una clave derivada de la clave de ámbito, 8 bytes: `ab12-cd34-ef56-7890`). Es lo único que el admin ve de un valor: le sirve para comprobar lo que escribió y no permite recuperarlo ni probar valores sin la clave. |
| **Grace Period** (secretos) | Hécate | Tiempo en que la versión anterior de un secreto aún se resuelve tras llegar una nueva (24 h por defecto). |
| **Checkpoint** | Hécate, todos | Firma de Hécate sobre el último hash de cada stream de auditoría. Fija lo anterior: modificarlo rompe la cadena o la firma. |
| **Hash Chain** | Todos | Cada evento de un stream incluye el hash del anterior, de modo que cualquier cambio o borrado se detecta. |
| **Trust Domain** | Todos | Raíz de confianza de un entorno (`cerbero.home`, `cerbero.test`): un certificado de un trust domain no vale en otro. |
| **SPIFFE ID** | Todos | Identidad de un componente con forma `spiffe://<trust domain>/<ruta>`, como único URI SAN de su certificado. |
| **Child Identity** | Hades, Hécate | Identidad bajo la ruta de otra (p. ej. una integración de Hades), que la pide su padre; el padre no lee sus secretos. |
| **Enrollment Token** | Hécate | Token de alta: de un solo uso, ligado a una identidad y a la huella de la CA raíz, que caduca en minutos. Con él un componente obtiene su primer certificado o vuelve a darse de alta; solo lo gasta la emisión. |
| **Passive Revocation** | Hécate | Revocar un certificado bloqueando su renovación: deja de valer al caducar (como mucho, 3 días). |
| **Urgent Revocation** | Hécate | Revocación urgente: el certificado deja de valer al momento, entra en la lista de revocación hasta que caduca y su clave de firma de llamadas deja de valer para llamadas nuevas. |
| **Revocation List** | Hécate, todos | Lista de revocación: CRL X.509 con los certificados revocados con urgencia que no han caducado, firmada por la intermedia que los emitió. Vale 5 minutos y los componentes la refrescan. |
| **Eurytion** (Agent) | Orthros | Agent de Cerbero: proceso residente en un host que recolecta, filtra, parsea y envía UDM Events a Estigia. En la mitología, el pastor que guardaba el rebaño junto a Orthros. |
| **Sensor** | Orthros | Capacidad de observación enchufable dentro del Agent (process, connection, dns, network...). Un mismo Sensor tiene una implementación por sistema operativo. |
| **Profile** | Orthros | Tipología de equipo (Linux, macOS, Windows, Android, iOS, IoT...). Determina qué Policy recibe un Agent. |
| **Policy** | Orthros | Configuración versionada y firmada que Orthros publica para un Profile: Flags, Filters, Aggregations y Parsers. |
| **Access Policy** | Hécate | Lo que una identidad puede usar además de sus propios secretos (`secret://<su identidad>/**`, la regla por defecto, que no se guarda): las excepciones del admin, cada una para leer un secreto concreto de otra identidad, sin comodines; el permiso de estado, que deja consultar los recuentos agregados de Hécate; y los permisos que salen de lo que declaran instancias e integraciones. Versionada, con quién hizo cada cambio. No confundir con la Policy de Orthros. |
| **Flag** | Orthros | Activación o desactivación de un Sensor dentro de una Policy. |
| **Filter** | Orthros | Regla de descarte ejecutada en el Agent para no enviar datos de bajo valor. |
| **Aggregation** | Orthros | Regla que resume en el Agent eventos repetidos en un único registro con contador. |
| **Parser** | Orthros | Regla declarativa de mapeo de un Raw Log al UDM. Se define en Orthros y se ejecuta en el Agent. |
| **Fleet** | Orthros | Conjunto de Agents desplegados y su gestión. |
| **Enrollment** | Orthros | Alta de un Agent nuevo en el Fleet. |
| **Heartbeat** | Orthros | Señal periódica del Agent al control plane: está vivo, qué versión de Policy aplica y cuánto ha descartado. El control plane publica en Estigia su resumen y la ausencia de heartbeats como UDM Events. |
| **Drift** | Orthros | Desfase entre la Policy publicada para un Profile y la que aplica realmente un Agent. |
| **Agentless** | Orthros | Profile de equipos donde no se puede instalar un Agent (p. ej. móviles). Se observan desde la red con el Sensor `network` de un Eurytion o con el NSM. |
