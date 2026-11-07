# Decisiones — Cerbero

Decisiones de diseño y tecnología. Las abiertas se tomarán en su momento, se documentarán como ADR en [docs/adr](docs/adr/README.md) y se reflejarán en [ARCHITECTURE](ARCHITECTURE.md) o en el documento del componente en [docs/](docs/). Las desviaciones registran dónde Cerbero se aparta a propósito de su referencia: SecOps, salvo que se nombre otra.

---

## Desviaciones respecto a la referencia

### Caronte frente a YARA-L (lenguaje)

| Desviación | Justificación |
|---|---|
| `over <d>` sin pivote se ejecuta como *sliding* exacta, no como *hop window* | La ejecución es reactiva, no programada. El significado para el autor es el mismo, y la *sliding* no pierde coincidencias en las fronteras de los saltos. |

No hay más desviaciones de lenguaje. Se descartaron `by <d>` (tumbling: se cubre con `over` + supresión) y `before_strict` / `after_strict` (se cubre con comparaciones explícitas, como en YARA-L).

### Caronte frente a SecOps (producto)

| Desviación | Justificación |
|---|---|
| `max_depth` de las compuestas configurable, con valor actual 1 (SecOps permite 10) | Uso personal. Multi-sobre-multi se habilitará subiendo el valor; caso previsto: "logins masivos" + "descubrimiento masivo". |
| Entradas de las compuestas solo por `rule_id`: ni tags ni Data Tables | Dependencias explícitas, lo que permite validar el grafo y la integridad referencial. Las Data Tables son del propietario. |
| Integridad referencial y compuestas degradadas | Una compuesta no puede quedarse sin entradas en silencio. |
| Lateness `L` y ventana simétrica para tardíos | Sin ejecución programada que reprocese ventanas. |
| `DETECTION_INVALIDATED` (desactivación tardía de una bomba) | Las Detections son inmutables; la corrección es un hecho nuevo y la decisión es de Hades. |
| La Detection referencia por IDs y no incrusta muestras de eventos | Estigia ya guarda los eventos. |
| La Alert es un objeto propio y una notificación, no una Detection con `alert_state` | Hades consulta el detalle a Estigia, así que no hay datos duplicados que puedan divergir. |
| Supresión de acción en Hades; Caronte solo suprime ráfagas (`suppression_window`) | Las Detections son conteo útil para las compuestas. |
| Cuota de Detections por `rule_id` y día natural UTC (configurable, mínimo 100, por defecto 1.000), sumando todas las versiones (SecOps: 10.000 por versión) | Red de seguridad para uso personal. Una versión nueva no reinicia el contador en mitad de una ráfaga. |
| La Rule declara su Playbook principal (label `playbook`) y su clave de agrupación (`$grouping_key`), validados al desplegar | Cada Rule que alerta es un caso de uso ya cribado: su autor sabe cómo se agrupa y cómo se responde. Las dependencias explícitas evitan un Playbook fallback. |
| Caronte no escribe en Data Tables (SecOps: `write_row` en la sección `export`) | Por ahora, un escritor menos. Se puede retomar con `write_row` como referencia. |

### Hades frente a SecOps SOAR (producto)

| Desviación | Justificación |
|---|---|
| Alert y Case son aggregates separados; la Alert tiene ciclo de vida y validez propios (`valid / discarded / withdrawn`) | La validez cambia por eventos tardíos (`DETECTION_INVALIDATED`, extensión propia) sin tocar el Case ni su Verdict. |
| Agrupación por `rule_id` + `$grouping_key` de valor único (SecOps: basta una entidad en común) | Una Alert nunca coincide con dos Cases, y `Case.open_from(alert, grouping_key)` es determinista. |
| Ventana de agrupación de 7 días anclada a la primera Alert del Case (SecOps: 0,5 a 24 h, sin documentar desde dónde se mide) | Un Case es "nuevo" mientras dura su ventana; la señal persistente se corta en la Rule (ver más abajo). |
| Sin tope de Alerts por Case (SecOps: 30) | El Case solo guarda referencias; abrir otro a las 30 rompería la definición de "nuevo". |
| Sin Playbook fallback | Su existencia se garantiza al desplegar la Rule y al retirar el Playbook. Red de seguridad: error `playbook_missing`. |
| Corte de entrada irreversible al entrar en `intervention`, con `related_cases` | Lo que llega después de actuar va a un Case nuevo: mantiene el sentido de la ventana y de la acción ya lanzada. |
| Hades no mira el `mode` (`rule_status`) | Las Rules inmaduras se prueban en entornos funcionales completos y con CI; SecOps tampoco lo aplica en la respuesta. |
| Verdict separado del motivo de cierre y del root cause, con `benign_positive` | Es la recomendación de Google (campo *Disposition*). `benign_positive` y `false_positive` miden cosas distintas: actualizar el contexto frente a calidad de la Rule. |
| El Verdict registra quién lo dio (`by`) y se corrige con `VerdictCorrected` sin reabrir el Case | Uso real: el SOAR cierra tickets con Verdict. Más automatización implica más correcciones, y la historia se conserva. |
| Eventos de primera clase, categorizados y suscribibles; los errores los avisa un Playbook secundario o un Job | En SecOps un Playbook en error solo se ve en dashboards; aquí cualquier fallo es un evento al que reaccionar. |
| Playbook como documento declarativo (JSON Schema escrito en YAML) con semver y referencia estilo Docker | Texto + CI, como las Rules. SecOps no documenta versiones de Playbook. |
| Lanzar y recoger separados: un `submit` devuelve un `handle`, que recogen después `submit.result` o `awaiting`; `dispatch` manda sin seguir | Permite lanzar en paralelo, esperar después e incluso recoger desde otro Playbook. En SecOps, una action asíncrona bloquea el Playbook. |
| Bloques `destino.verbo` sin código para hablar con Estigia, Caronte, Hades y Hermes (SecOps: cada paso es una action de una integración) | Con un contrato común y publicado, consultar o encargar algo no necesita código; la `action` queda para la lógica propia. |
| Sin connectors en Hades; Jobs y cronjobs sí | En Cerbero la ingesta es de las cabezas y Estigia. |
| Data Tables e historial de contexto con cambios añadidos (revertir y volver a un punto, estilo git) | Reescribir la historia rompería las consultas *as-of* y la auditoría. |

### Descartado en Hades

| Propuesta | Por qué se descartó |
|---|---|
| Playbook fallback | Se prefiere validar las dependencias. |
| Silence como aggregate y rama `suppressed` de la Alert | Silence es un atributo del Case y un filtro en los suscriptores: la Alert sigue entrando en el Case. |
| Tratamiento por `mode` en Hades (Case `test`, `dry_run`, `recorded_only`, `mode` en la clave de agrupación) | Las etiquetas son de entornos y CI. |
| `assigned` en la Alert | La asignación es del Case (contacto) y de la Response. |
| Case de overflow, merge y Case padre (agrupar Cases) | Aparcado: el overflow se mitiga en las Rules y el alerting. Se retoma si aparecen casos en los que agrupar Cases sea la solución. |
| Grafo plano con `next` (estilo Step Functions) | Flujo estructurado con caminos referenciados: se lee mejor y no hay saltos arbitrarios. |
| `listen_event` dentro del Playbook | El trabajo fuera de la línea principal va a un Playbook secundario cuyo trigger es el evento. |
| `valid_to` en el trust level | Lo temporal va a Data Tables con TTL. |
| Bloque `intake_cut` y `accepts_alerts` por stage | El corte es implícito al entrar en `intervention`. |
| Esperar al customer en la UI de Hades | Solo se esperan respuestas de terceros; las personas responden por un canal (p. ej. `chat.ask`). |
| Notificar cada Alert nueva de un Case abierto | La comunicación sigue el flujo del Playbook. |

### Hermes frente a SecOps SOAR (integraciones)

| Desviación | Justificación |
|---|---|
| Contratos estándar por tipología (SecOps: cada integración devuelve su propia salida) | Un Playbook que usa el contrato del tipo de servicio funciona con cualquier proveedor, y un cambio en la API del proveedor se absorbe en su librería sin tocar los Playbooks. |
| Las librerías de terceros viven en un componente aparte, Hermes, recomendado pero no obligatorio | Un sitio para contratos, credenciales, reintentos seguros y límites; las integraciones de Hades pueden hablar con un tercero si Hermes no lo cubre. |
| Integraciones de Hades con `use` / `extends` y clases expuestas que otras importan (SecOps: las comerciales se duplican para cambiarlas) | Extender sin sobrescribir y reutilizar sin copias que divergen. |
| `side_effect` y `discloses_data` descritos en el contrato de cada función | Se sabe qué cambia cada paso y qué datos salen de casa, con aviso al registrar y traza en la auditoría. |

### Descartado en Hermes

| Propuesta | Por qué se descartó |
|---|---|
| Hermes como frontera obligatoria con terceros | Solo daba complicaciones; conectarse al exterior desde una integración no es problema. |
| Payload nativo del proveedor en el Playbook | Los contratos siempre los define Hermes; lo nativo ataría los Playbooks al proveedor. |
| Convertir modos (envolver una llamada síncrona en task o al revés) | Cada función usa el modo nativo del proveedor. |
| Webhooks | No se prevén; si llegan, se analizan en ese momento. |
| Control de ruido en Hermes (throttling, digest) | El control de avalanchas está aguas arriba. |
| Ejecución remota (Remote Agent de SecOps) | Un solo entorno en casa. |

### Hécate frente a sus referencias (OpenBao / Vault, step-ca, SPIFFE)

| Desviación | Justificación |
|---|---|
| Hécate se construye en Cerbero, con primitivas de librerías auditadas y sin inventar criptografía | Aprender cómo funciona por dentro; lo delicado es la gestión de claves (rotación, revocación, recuperación), no el algoritmo. |
| Desbloqueo con la frase del admin y Shamir 2 de 3 solo para recuperar (OpenBao: Shamir o auto-unseal) | Reinicios cómodos en casa sin perder la raíz si se olvida la frase. |
| Certificados de componente de 3 días (step-ca: 24 h por defecto) | Aguantar que Hécate esté sellada un tiempo sin que caigan todas las identidades; la revocación pasiva sigue acotada a días. |
| Identidad en formato SPIFFE emitida por la CA de Hécate, sin SPIRE | Mismo formato estándar sin otro sistema que operar; las identidades hijas cubren el caso de las integraciones. |
| JWT ligado al canal mTLS (SPIFFE desaconseja los JWT-SVID por *replay*) | El JWT lleva permisos y contexto de auditoría, y exigir que `iss` coincida con el certificado de la conexión evita su reutilización desde otro componente. |
| Hécate firma los checkpoints de auditoría de todo Cerbero | Un componente comprometido no puede reescribir su historia ya firmada: el no repudio queda respaldado criptográficamente, no solo por el registro append-only. |

### Descartado en Hécate y en la identidad

| Propuesta | Por qué se descartó |
|---|---|
| IdP propio con OIDC para las personas | Las passkeys en Ténaro bastan; OIDC queda como adapter futuro. |
| OAuth *client credentials* entre componentes | Todo el tráfico interno dependería de que el IdP estuviera disponible. |
| Claves API compartidas entre componentes | Un secreto común, sin caducidad ni identidad. |
| OpenBao, SOPS + age, variables de entorno o Docker secrets como almacén | Se prefiere construirlo para aprender. |
| Inventar criptografía | Solo primitivas de librerías auditadas. |

### Riesgos aceptados

| Riesgo | Por qué se acepta |
|---|---|
| Hécate firma sus propios checkpoints: quien llegue a la propia Hécate podría reescribirla y volver a firmarla | Solo lo cubre anclar los checkpoints fuera de Cerbero, que queda para más adelante. |
| Tras un corte de luz de toda la casa, Hécate queda sellada y no hay avisos hasta desbloquearla | Los avisos dependen de secretos que custodia Hécate; Ténaro muestra el estado. |
| Python no garantiza borrar la raíz de la memoria | Se mitiga más adelante con el adapter de hardware. |

---

## Tomadas

Las decisiones de tecnología y organización se documentan como ADR en [docs/adr](docs/adr/README.md).

| Decisión | ADR |
|---|---|
| Monorepo con proyectos independientes en un workspace de uv | [0001](docs/adr/0001-monorepo-uv-workspaces.md) |
| Rust para Eurytion; Python para el resto | [0002](docs/adr/0002-lenguajes-por-proyecto.md) |
| Persistencia de Hécate en SQLite | [0003](docs/adr/0003-persistencia-hecate-sqlite.md) |
| Pruebas en Python con pytest e hypothesis | [0004](docs/adr/0004-pruebas-pytest-hypothesis.md) |

---

## Abiertas

| Decisión | Notas |
|---|---|
| Broker / bus de eventos | Debe correr en el home lab y soportar consumo con reentrega y relectura del flujo. |
| Formato y distribución del esquema UDM | Formato del esquema y cómo se publica como artefacto versionado. |
| Infraestructura de observabilidad | Decidido: OpenTelemetry para trazas y métricas y logs estructurados a stdout en todos los componentes. Falta el colector, el backend y el almacenamiento. |
| Persistencia por proyecto | Hécate: SQLite ([ADR 0003](docs/adr/0003-persistencia-hecate-sqlite.md)). Falta: event store de Hades, almacenamiento y búsqueda de Estigia, buffer de las cabezas. |
| Dueño del contrato de Alert | Caronte lo publica; falta decidir dónde se define y versiona. |
| Motor de YARA-L | Implementación del parser y evaluador de YARA-L en Caronte. |
| Temporizadores de las bombas | Bus con entrega diferida o servicio de tareas. Debe vivir fuera de Caronte. |
| Tecnología de la cache de Detections | Estructura ordenada por `event_time` con TTL por entrada (p. ej. Valkey/Redis con sorted sets). |
| Ruta YARA-L para leer variables de una Detection | `detection.detection.outcomes[...]` frente a `variables[...]` (`outcomes` está deprecated en la API de SecOps). Verificar al implementar. |
| Límite inclusivo del pivote | La documentación de SecOps no aclara si `over <d> after $pivot` incluye el instante del pivote. Se asume que sí. |
| Coche escoba en Caronte | Diseñado, no implementado: relanzar evaluaciones fallidas o incompletas, apoyado en los IDs deterministas. |
| Event Sourcing de entidades en Estigia | Permitiría recalcular la entity resolution si cambia el algoritmo. |
| Lenguaje del NSM y del control plane de Orthros | Eurytion es Rust ([ADR 0002](docs/adr/0002-lenguajes-por-proyecto.md)). Valorar si el NSM, o Orthros entero, pasa también a Rust por ser sondas distribuidas como binario. Si cambia, un ADR nuevo sustituye al 0002 y esas piezas salen del workspace de uv. |
| Formato declarativo de Parsers, Filters y Aggregations | Interpretable por el motor genérico de Eurytion y versionable dentro de la Policy. Referencia: parsers CBN de SecOps. |
| Protocolo de distribución de Policies | Pull en heartbeat. Referencia: OpAMP (OpenTelemetry). |
| Mecanismo de firma de Policies e identidad de Eurytion | Ligado al enrollment. Puede seguir el patrón de identidades de Hécate (alta con token y, si hace falta, identidades hijas). |
| Implementación de Sensors por sistema operativo | Un adapter por SO y Sensor. |
| Estrategia para Profiles agentless (móviles) | Observación desde la red; MDM / VPN como alternativas. |
| Captura de tráfico para el NSM | Port mirroring / SPAN, network tap o servidor en línea. |
| Bloques compartidos NSM ↔ Eurytion | Reutilizar dentro de Orthros el motor de Parsers, el publisher y el formato de Policy; reporte de salud del NSM al Fleet. |
| Modelo de Echidna | Si adopta el mismo modelo de control plane + agentes que Orthros. |
| Schemas de Entity, Reference Data y Data Tables | Se definirán con el uso, como el esquema UDM. |
| Feeds de inteligencia | Qué fuentes de IOCs y CVEs, con qué frecuencia y con qué ventana de retrohunt. |
| Correlación de CVEs | Requiere inventario de software por activo (Sensor `software` de Eurytion). |
| Interfaz del propietario para Data Tables | Cómo edita el propietario las Data Tables en Estigia. |
| Límite de la puerta de corte | Vueltas o tiempo máximo para que una señal persistente no impida actuar nunca. |
| Ventana de agrupación | Revisar con el uso si 7 días es excesivo. |
| Detalle de Silence | Qué se silencia por defecto, duración y origen. |
| Cases similares | Cómo consulta la Response los Cases similares y reutiliza el trabajo (p. ej. el ticket abierto); carrera entre dos Cases simultáneos. |
| Cadenas de Playbooks | Límite de profundidad por `causation_id` y validación del grafo de triggers. |
| Resultados JSON grandes | Guardarlos fuera del stream de la Response. |
| Fragmentos reutilizables y ejecución en paralelo | Caminos compartidos entre Playbooks; ramas paralelas dentro de un Playbook. |
| Contratos de comando a Estigia | Borrador: `SetEntityTrustLevel`, `AnnotateEntity`, filas de Data Tables. |
| `mode` en la cabecera de la Alert | Hades no lo usa; decidir si se mantiene. |
| Exclusions en Caronte | Filtro sobre campos UDM antes de alertar, con prueba de impacto (SecOps). Sin evaluar. |
| Contrato de `run_playbook` | Payload, entradas y salida al lanzar un Playbook desde otro. |
| Lote de tasks | Un Playbook lanza tasks y otro las recoge con sus `handle`, para que los Playbooks terminen antes. Idea abierta. |
| `endpoint_control` | Categoría para actuar sobre dispositivos de la red (aislar un equipo, matar un proceso); el proveedor podría ser Orthros. Idea abierta. |
| Ténaro | Backend orquestador para la UI (*backend for frontend*), desde donde se gestionan los componentes. |
| Repositorio privado de lo propio | Decidido: un repositorio privado común, separado por ruta (`hades/`, `hermes/`), que se edita desde Ténaro con PRs; lo oficial vive en el repositorio de Cerbero. Nunca van al repositorio secretos, contactos ni valores de variables: viven en la base de datos de la app que corresponda (Hades para los Playbooks e integraciones, Hermes para contactos y plantillas). Falta: si contactos y variables se guardan cifrados; la estructura de carpetas de cada ruta, la batería de pruebas de las PRs (hoy no existe) y cómo llegan los cambios fusionados a Hades y Hermes. |
| Política común de copias de seguridad | Regla 3-2-1, cifrado y restauración probada para todos los almacenes de Cerbero. |
| Detalle de contratos y versionado de Hermes | JSON Schema y OpenAPI 3.1, major en la ruta: el detalle, al implementar. |
| `chat.ask` | Clasificación (consulta con `side_effect`) y comportamiento con cada proveedor, a ajustar al implementar; texto libre más adelante. |
| Permisos por rol | Hoy hay un único rol privilegiado, el admin (Hécate, identidades, invitaciones); owner y maintainer salen de las definiciones. Falta la capa de permisos para el resto. |
| `signal` en otros destinos | Solo `hades.signal` tiene uso; en los demás, cuando su contrato publique señales. |
| Adapter de hardware de Hécate | TPM o YubiKey detrás del `CryptoProvider`: desbloqueo automático y claves que no salen del dispositivo. |
| Rotación automática de secretos | Que la librería del proveedor cree credenciales nuevas por API cuando el proveedor lo permita. |
| Anclaje externo de los checkpoints | Una copia de los checkpoints firmados fuera de Cerbero, para detectar también una reescritura de Hécate. |
| Relación Entidades ↔ Cases | Explorar Cases a través de las entidades que comparten, al estilo de una base de grafos. Idea sin decidir. |
