# Arquitectura de Software — Cerbero

Describe cómo está construido Cerbero: principios, bounded contexts, comunicación entre proyectos y arquitectura interna de cada uno.

Ver también: [README](README.md) (visión funcional) · [GLOSSARY](GLOSSARY.md) (lenguaje ubicuo)

---

## 1. Principios

1. **Independencia real.** Cada proyecto (Orthros, Echidna, Estigia, Caronte, Hades, Hermes, Hécate) se despliega, arranca y evoluciona por separado. Activar o desactivar uno no obliga a modificar los demás.
2. **Solo se comparte el contrato, nunca el dominio.** Lo único común entre proyectos es el lenguaje publicado: el esquema UDM y los mensajes de integración. No hay código de dominio compartido entre bounded contexts.
3. **Comunicación asíncrona en el camino principal.** El flujo `Orthros/Echidna → Estigia → Caronte → Hades` se basa en eventos. Ningún proyecto bloquea su trabajo esperando a otro. Lo que Hades pide a otros componentes (Estigia, Caronte, Hermes) va por API, síncrona o como task que se consulta después.
4. **Degradación elegante.** Si un proyecto cae, los demás siguen trabajando con lo que tienen y se ponen al día cuando vuelve.
5. **Mismo estilo, distinta intensidad.** Todos los proyectos usan arquitectura hexagonal. DDD táctico, CQRS y Event Sourcing se aplican solo donde el dominio lo justifica.
6. **SecOps como referencia.** El diseño toma como referencia cómo resuelve cada problema una plataforma SecOps. La diferencia principal es que las cabezas producen UDM de forma nativa en lugar de delegar el parseo a la plataforma.

---

## 2. Bounded Contexts y Context Map

Cada proyecto es un bounded context con su propio modelo y su propio lenguaje interno.

```text
                    Propietario (Data Tables)
                              │
   ORTHROS ───┐               ▼
              ├──▶        ESTIGIA  ──── flujo ───▶  CARONTE  ── Alerts ──▶  HADES ── API ──▶ HERMES ──▶ terceros
   ECHIDNA ───┘            ▲   ▲                       │   ▲                 │ │                 (Telegram, Jira,
                           │   └ consultas, Detections ┘   └──── API ────────┘ │                  VirusTotal...)
                           └──────────── consultas + comandos ─────────────────┘
                                                                 integraciones de Hades ──▶ terceros

   HÉCATE ◀── secretos, certificados, firma de checkpoints (mTLS) ── todos los componentes y Ténaro
```

| Relación | Patrón DDD | Significado |
|---|---|---|
| Orthros / Echidna → Estigia | **Published Language** (UDM) | Las cabezas traducen su modelo interno al UDM antes de publicar. Su parser es la frontera de traducción. |
| Estigia → Caronte | **Open Host Service** + Published Language | Estigia expone el flujo de eventos UDM. Caronte lo consume sin conocer el origen. |
| Caronte → Hades | Published Language (contrato de Alert) | Caronte notifica Alerts en un formato estable que Hades consume. La Alert es una notificación: Hades obtiene el detalle consultando a Estigia. La Rule declara el Playbook que la responde y Hades valida que existe. |
| Caronte → Estigia (consultas) | **Customer / Supplier** | Caronte consulta eventos, Detections, entidades as-of y Data Tables para evaluar las ventanas de sus Rules. Solo lectura. |
| Caronte → Estigia (Detections) | Published Language (UDM) | Caronte reingesta sus Detections como eventos, igual que una cabeza. Pasan por el pipeline completo de Estigia. |
| Caronte → Estigia (Retention Policy) | **Customer / Supplier** | Caronte publica cuánto tiempo debe retener Estigia en cache las Detections de cada Rule. Estigia la aplica sin conocer las Rules. |
| Hades → Estigia | **Customer / Supplier** (consultas + comandos) | Hades consulta para investigar. Es el único componente que modifica el contexto de las entidades y, junto con el propietario, escribe en las Data Tables. |
| Hades → Caronte | **Customer / Supplier** | Hades consulta Rules y Detections y lanza retrohunts como tasks. |
| Hades → Hermes | **Customer / Supplier** | Hades usa los servicios de terceros con contrato por tipología: llamadas síncronas y tasks cuyo estado consulta por API. Nunca escucha a Hermes. |
| Hermes → terceros | **Anticorruption Layer** | Cada librería de proveedor traduce el modelo de un servicio externo (Telegram, Jira, VirusTotal...) a los contratos de Hermes. |
| Todos → Hécate | **Customer / Supplier** | Cada componente resuelve sus secretos, renueva su identidad y pide la firma de sus checkpoints de auditoría. Hécate nunca llama a nadie. |
| Ténaro → componentes | **Customer / Supplier** | La UI entra solo por Ténaro, que autentica a las personas y llama a cada componente con su identidad y la persona en el contexto. |
| Integraciones de Hades → terceros | **Conformist** | Una integración propia puede hablar directamente con un servicio que Hermes no cubre, adaptándose a su modelo. Lo recomendado es crear la librería en Hermes. |

---

## 3. Comunicación entre proyectos

Se distinguen tres tipos de mensaje. No se mezclan.

| Tipo | Dirección | Naturaleza | Ejemplos |
|---|---|---|---|
| **Eventos de integración** | Cabezas → Estigia → Caronte → Hades, Caronte → Estigia | Asíncronos, hechos ya ocurridos | UDM Event, entidad actualizada, Detection, Alert emitida |
| **Consultas** | Caronte, Hades → Estigia; Hades → Caronte, Hermes | Petición/respuesta (o task que se consulta después), solo lectura | Contexto de una entidad as-of, ventanas de eventos y Detections, Data Tables, retrohunt, reputación de una IP |
| **Comandos** | Caronte → Estigia | Configuración que Estigia aplica sin conocer su origen | Retention Policy de la cache de Detections |
| **Comandos** | Hades → Estigia | Modificación del contexto y de Data Tables | Cambiar el trust level de una entidad, anotarla, añadir una fila a una Data Table |
| **Comandos** | Hades → Hermes | Cambios en servicios de terceros, síncronos o como task | Enviar un mensaje, crear un ticket, bloquear una IP |
| **Consultas y comandos** | Todos → Hécate | Petición/respuesta por mTLS | Resolver un secreto, renovar un certificado, envolver una clave, firmar un checkpoint |

Reglas:

- **Productores fiables:** quien publica eventos tras cambiar su estado usa **Transactional Outbox**. Así no se pierde un evento ni se publica uno sin haber persistido.
- **Consumidores idempotentes:** todo consumidor tolera duplicados y reentregas.
- **Buffer en las cabezas:** si Estigia no está disponible, las cabezas acumulan localmente y reenvían al volver.
- **Contrato común de las APIs internas** (Estigia, Caronte, Hades, Hermes, Hécate): misma estructura de respuesta y mismos códigos de error; cada componente publica sus API Contracts (OpenAPI) y su `/capabilities`; la versión mayor va en la ruta; las peticiones con efecto llevan la cabecera `Idempotency-Key`. Hades los usa con los mismos verbos (ver [los contratos de Hades](docs/hades.md#5-contratos)).
- **Eventos ≠ Event Sourcing:** el río de Estigia es un log de eventos de integración (hechos del mundo). Event Sourcing es persistir un aggregate como la secuencia de sus propios eventos de dominio. Los dos conviven, pero no se confunden.

---

## 4. Contrato UDM

El esquema UDM es el contrato del que dependen los cinco proyectos.

- **Dueña:** Estigia define y versiona el esquema.
- **Distribución:** el esquema se publica como artefacto versionado que consumen productores y consumidores.
- **Versión explícita:** cada evento declara la versión del esquema con la que fue construido.
- **Compatibilidad hacia atrás:** los cambios del esquema no rompen a productores ni consumidores existentes.
- **Contenido de un evento:** tipo de evento, origen (cabeza y sensor), marca temporal, identificadores de entidad observados y datos normalizados.
- **Sin Raw Log:** el dato original se parsea en origen y no viaja a Estigia.

---

## 5. Arquitectura interna por proyecto

### Esqueleto común (hexagonal)

```text
<proyecto>/
├── domain/          # Modelo del dominio: sin dependencias externas
├── application/     # Casos de uso, puertos (interfaces) y orquestación
└── infrastructure/  # Adapters: entrada (collectors, consumidores, API) y salida (publisher, persistencia, notificaciones)
```

Regla de dependencia: `infrastructure → application → domain`. El dominio no conoce brokers, bases de datos ni protocolos.

### Intensidad de patrones

| Proyecto | Naturaleza | DDD táctico | CQRS | Event Sourcing |
|---|---|---|---|---|
| **Orthros (control plane)** | Gestión del Fleet | Medio: Profile, Policy, Agent (registro) | No | No |
| **Orthros (Eurytion)** | Pipeline | Ligero: value objects (MAC, IP, PID, Segmento) | No | No |
| **Orthros (NSM)** | Pipeline | Ligero: value objects (Flow, Session, IP) | No | No |
| **Echidna** | Pipeline | Ligero: value objects (Zona, Sensor físico) | No | No |
| **Estigia** | Ingesta, entidades y búsqueda | Fuerte en entidades (Asset, identificadores, resolución) | **Sí** | No |
| **Caronte** | Motor de reglas | Medio: Rule, Rule Deployment, Detection, Alert | No | No |
| **Hades** | Flujo de trabajo con estado | **Fuerte**: Alert, Case, Playbook, Response | **Sí** | **Sí** (los cuatro aggregates) |
| **Hécate** | Secretos y claves | Medio: Secret, Key, Certificate, Identity, Policy | No | **Sí** (los cinco aggregates y el registro de accesos) |
| **Hermes** | Librerías de terceros con contrato | Fuerte en lo común (sobre, contratos, Task, contactos); tipo de servicio = port, proveedor = adapter | **Sí** (Task) | **Sí** (Task) |

---

### Orthros

Detalle completo en [docs/orthros.md](docs/orthros.md).

- **Tres piezas:** el **control plane** (Profiles, Policies firmadas y Fleet; fuera del camino de los datos), los **Eurytion** (agentes en cada equipo) y el **NSM** (análisis del tráfico de toda la red). Eurytion y NSM son las dos cabezas.
- **Eurytion:** núcleo fijo y Sensors enchufables (process, connection, dns, network), cada uno con un adapter por sistema operativo; pipeline `sensor → filter / aggregate → parser (→ UDM) → publisher`. Las Policies son datos: llegan por pull en cada heartbeat.
- **Reducción y parseo en el borde:** solo viajan UDM Events; los Raw Logs y los paquetes nunca salen de origen. Los fallos de parseo y los descartes se cuentan en el heartbeat.
- **Frontera:** observa, reduce y parsea; nunca detecta.

---

### Echidna — Pipes & Filters

Detalle en [docs/echidna.md](docs/echidna.md).

- **Qué hace:** recoge los eventos del dominio físico y domótico (puertas, ventanas, presencia, estado de la casa y sus zonas) y los entrega a Estigia en UDM.
- **Pipeline:** `collector → sensor → mapper (→ UDM) → publisher (buffer + outbox)`, con un collector por fuente (MQTT, Zigbee, Z-Wave...).
- **Frontera:** sus sensores emiten eventos, nunca Alerts.

---

### Estigia — CQRS

Detalle completo en [docs/estigia.md](docs/estigia.md).

- **Qué hace:** es dueña del esquema UDM y el cauce obligatorio de los datos: valida, deduplica, resuelve entidades con historia (*as-of*), enriquece, persiste y publica el río que consume Caronte.
- **Pipeline:** `Ingest → Validate → Resolve → Enrich → Persist → Publish`. Persiste antes de publicar: todo lo que ve Caronte ya es consultable.
- **Modelo:** UDM Events y Detections inmutables, entidades con intervalos de validez, Reference Data de feeds y Data Tables (temporales, con TTL opcional e historial; las escriben el propietario y Hades).
- **Comandos de Hades:** contexto de entidad y filas de Data Tables, idempotentes, nunca retroactivos y con historial añadido.
- **Frontera:** anota hechos; juzgarlos es de Caronte y de Hades.

---

### Caronte — motor de Rules reactivo y sin estado

Detalle completo en [docs/caronte.md](docs/caronte.md).

- **Qué hace:** evalúa Rules YARA-L (single-event, multi-event y compuestas) sobre el río de Estigia; emite Detections, que se reingestan en Estigia, y Alerts para Hades cuando la Rule alerta.
- **Reactivo y sin estado:** cada evaluación la dispara un evento real; las ventanas son consultas a Estigia por `event_time` y el contexto se consulta *as-of*. Ventana simétrica y lateness `L` para los tardíos; bombas para "A sin B".
- **Modelo:** Rule (texto versionado), Rule Deployment (`status`, `detection`, `alerting`), Detection (hecho inmutable con identificador determinista) y Alert (notificación).
- **Compuestas solo por `rule_id`,** con validación del grafo, integridad referencial y Retention Policy publicada en Estigia.
- **Contención de ráfagas:** `suppression_window` y cuota de Detections por Rule y día. La supresión de acción es de Hades.
- **Frontera:** detecta y notifica; no da veredictos ni agrupa Alerts.

---

### Hades — Event Sourcing + CQRS + Process Manager

Detalle completo en [docs/hades.md](docs/hades.md).

- **Qué hace:** agrupa las Alerts de Caronte en Cases (`rule_id` + `$grouping_key`, ventana de 7 días), orquesta la respuesta con Playbooks declarativos (una Response por ejecución) y concluye con un Verdict (`true_positive`, `benign_positive`, `false_positive`, `undetermined`).
- **Modelo:** cuatro aggregates event-sourced (Alert, Case, Playbook, Response); proyecciones por pregunta (`case-timeline`, `playbook-health`, `event-log`).
- **Flujo de una Response:** `triage` → `investigation` → `intervention` (corte de entrada irreversible) → esperas a terceros → `close_case`.
- **Habla con los demás** con bloques `destino.verbo` (`query`, `command`, `submit.query`, `submit.command`, `submit.result`, `dispatch`, `signal`) sobre los API Contracts de Estigia, Caronte, Hermes y el propio Hades; la lógica propia va en integraciones (`action`, Jobs, cronjobs).
- **Eventos de primera clase:** todo se registra, categorizado; los Playbooks secundarios reaccionan a cualquier evento.
- **Frontera:** no mira el `mode` de la Alert ni guarda secretos.

---

### Hermes — librerías de terceros con contrato

Detalle completo en [docs/hermes.md](docs/hermes.md).

- **Qué hace:** reúne las librerías que envuelven servicios de terceros detrás de contratos estándar por tipología (categoría → tipo de servicio → función → proveedor → instancia). Un Playbook que usa el contrato del tipo de servicio funciona con cualquier proveedor.
- **Contratos en tres capas,** todas de Hermes: sobre común (con errores cerrados y `retryable`), contrato del tipo de servicio y contrato del proveedor. Cada función declara si es consulta o cambio, su modo nativo (síncrona o task), `side_effect` y `discloses_data`.
- **Ejecución:** llamadas síncronas y tasks event-sourced que Hades consulta por API; idempotencia por `Idempotency-Key`, reintentos solo cuando es seguro, polling hacia los proveedores.
- **Librerías de proveedor** como código de Hermes (manifiesto, contrato, adapter, polling, pruebas de conformidad) e instancias configuradas desde Ténaro.
- **Frontera:** no orquesta ni avisa a nadie; es lo recomendado para integrar un tercero, no un paso obligatorio.

---

### Hécate — secretos y claves

Detalle completo en [docs/hecate.md](docs/hecate.md).

- **Qué hace:** guarda los secretos de terceros y gestiona las claves de Cerbero (CA interna, claves de firma, rotación); firma los checkpoints de auditoría de todos los componentes. Se construye en Cerbero con primitivas de librerías auditadas detrás de un port `CryptoProvider`.
- **Modelo:** cinco aggregates event-sourced (Secret, Key, Certificate, Identity, Policy) y un registro de accesos aparte; ningún evento lleva un valor.
- **Cifrado por sobre:** raíz solo en memoria → claves de ámbito (una por identidad) → DEK por versión de secreto, con AES-256-GCM y la ruta, la versión y el ámbito como datos asociados.
- **Arranque:** sellada al reiniciar; se desbloquea con la frase del owner o, para recuperar, con 2 de 3 partes de Shamir.
- **Acceso:** cada identidad solo lee `secret://<su identidad>/**`, con permisos derivados de lo que declaran instancias e integraciones. Los Playbooks nunca manejan secretos.
- **Frontera:** solo red interna; nunca llama a nadie.

---

### Ténaro — la entrada a Cerbero

Detalle en [docs/tenaro.md](docs/tenaro.md).

- **Qué hace:** es el backend de Cerbero para la UI y la única superficie pública: autentica a las personas con passkeys, reúne las vistas de los componentes y les pasa las acciones de gestión y las manuales con la persona en el contexto.
- **Frontera:** no detecta, decide ni orquesta; consulta y delega en cada componente.

---

## 6. Identidad y seguridad

Común a todo Cerbero. La CA y las claves las gestiona Hécate.

### Identidad de los componentes

- **Formato SPIFFE:** `spiffe://cerbero.home/<componente>` como único URI SAN del certificado. Cada entorno tiene su **trust domain** (p. ej. `cerbero.test` para pruebas), así un certificado de pruebas nunca vale en casa.
- **Identidades hijas:** cada integración de Hades tiene la suya (`spiffe://cerbero.home/hades/integration/<nombre>`), pedida por Hades con su certificado y solo bajo su ruta.
- **PKI en dos niveles:** CA raíz offline (ECDSA P-256, unos 10 años) y CA intermedia en Hécate, renovada en una ceremonia offline. Certificados de componente de **3 días**, renovados a los dos tercios por mTLS y con clave nueva.
- **Alta con un token de un solo uso** que genera el owner: caduca en minutos, lleva la huella de la CA raíz y se marca usado. La clave privada se genera en el componente y nunca sale de él.
- **Revocación pasiva** por defecto (se bloquea la renovación); para lo urgente, una lista corta de certificados revocados que los componentes refrescan. Deshabilitar una identidad retira también sus claves del JWKS.
- **Relojes sincronizados (NTP)** en todos los equipos.

### Llamadas entre componentes

- **mTLS** con los certificados anteriores: identidad y canal cifrado.
- **JWT corto firmado por quien llama** con su propia clave Ed25519 (distinta de la del TLS), publicada en el JWKS de Hécate. Lleva `iss`, `aud`, `exp` de minutos, `jti`, permisos (por tipo de servicio y operación) y el contexto de auditoría (`source`, Response, Playbook o Job, owner / maintainer o la persona).
- **Ligado al canal:** el receptor exige que `iss` coincida con la identidad del certificado mTLS; un JWT robado no sirve desde otro componente.
- **No repudio firmado:** cada cambio queda atribuido criptográficamente a quien lo pidió (ver la auditoría de Hécate).

### Personas

- Ténaro, el backend de Cerbero para la UI, autentica con **passkeys** (WebAuthn): sin contraseñas, resistente al phishing, y el servidor solo guarda la clave pública.
- **Alta por invitación** con un enlace de un solo uso; **recuperación** con al menos dos dispositivos por persona o códigos de recuperación para el owner.
- **VPN delante** (WireGuard o Tailscale) si no se quiere exponer la app a internet.
- La autenticación va detrás de un port (primer adapter, passkeys; OIDC si algún día hace falta un IdP).
- Por ahora sin permisos por rol: owner y maintainer los declaran las definiciones, y cada modificación queda registrada con quién la hizo. Cuando actúa una persona, Ténaro firma el JWT con ella en el contexto.

### Superficies de red

- **Pública:** solo Ténaro, tras un proxy inverso con TLS.
- **Interna:** las APIs entre componentes, con mTLS. Hécate, solo interna.
- **Salida:** libre.

---

## 7. Comportamiento ante fallos

| Pieza caída | Efecto | Recuperación |
|---|---|---|
| Orthros control plane | Los Eurytion siguen operando con la última Policy aplicada; no hay cambios de Policy ni enrollment | Los Eurytion recogen la Policy pendiente en el siguiente heartbeat |
| Un Eurytion | No llegan eventos de ese host. La falta de heartbeat es en sí misma una señal | Eurytion reanuda y vacía su buffer |
| NSM | Se pierde la visión de la red completa; los Eurytion siguen operando | El NSM reanuda la observación |
| Echidna | No llegan eventos del dominio físico | El resto sigue operando con el dominio de red |
| Estigia | Las cabezas bufferizan; Caronte no recibe flujo ni puede consultar; Hades no puede investigar | Las cabezas reenvían; el flujo se reanuda |
| Caronte | Los eventos se acumulan en el río sin evaluar; las bombas que vencen esperan en el servicio de temporizadores | Caronte consume lo pendiente al volver, incluidas las bombas vencidas |
| Hades | Las Alerts se acumulan sin atender | Hades procesa las pendientes al volver; cada Response se reanuda desde su stream |
| Hermes | Hades no puede llamar a los servicios con contrato: el bloque falla y se reintenta según el Playbook. Las tasks ya aceptadas no se pierden | Hermes retoma las tasks no terminales desde su event store |
| Un proveedor de Hermes | Hermes reintenta lo seguro; si no, devuelve `provider_unavailable`, `rate_limited` o `timeout` | El estado de la instancia lo refleja en `/capabilities` |
| Hécate | Los componentes siguen con la caché de secretos hasta 1 h; después, lo que necesita un secreto falla. Las identidades aguantan hasta 3 días sin renovar | Arrancar y desbloquear (Ténaro o CLI local); procedimientos escritos para pérdida del disco, frase olvidada, CA intermedia o clave de ámbito comprometidas |
