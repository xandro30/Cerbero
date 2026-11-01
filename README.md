# 🐕‍🦺 Cerbero

> **Plataforma Integral de Ciberseguridad Doméstica y Monitoreo Perimetral**

**Cerbero** es un ecosistema de seguridad diseñado para proteger el hogar digital y físico. Replica a escala doméstica el modelo de una plataforma **SecOps** (**SIEM / UDM / Detection Engine / SOAR**), dividiendo la recolección, la normalización, la detección y la respuesta en proyectos desacoplados que pueden activarse de forma independiente.

Como Cerbero, guardián de las puertas del Inframundo, el sistema en su conjunto vigila que nada entre sin ser visto. Cada proyecto es una pieza de ese reino.

**Diferencia de valor:** una plataforma SecOps corporativa integra herramientas profesionales de terceros ya licenciadas. Cerbero construye sus propias fuentes: los recolectores se desarrollan y adaptan a medida y hablan el modelo de datos unificado de forma nativa.

**Documentación:** [Arquitectura](ARCHITECTURE.md) · [Glosario](GLOSSARY.md) · [Decisiones](DECISIONS.md)

**Componentes:** [Orthros](docs/orthros.md) · [Echidna](docs/echidna.md) · [Estigia](docs/estigia.md) · [Caronte](docs/caronte.md) · [Hades](docs/hades.md) · [Hermes](docs/hermes.md) · [Hécate](docs/hecate.md) · [Ténaro](docs/tenaro.md)

---

## 🏛️ Arquitectura del Sistema

```
┌─────────────────────────┐               ┌─────────────────────────┐
│         ORTHROS         │               │         ECHIDNA         │
│     (SIEM Virtual)      │               │      (SIEM Físico)      │
│ Red, Tráfico & Packets  │               │ Sensores, Puertas & IoT │
└────────────┬────────────┘               └────────────┬────────────┘
             │                                         │
             └────────────────────┬────────────────────┘
                                  │ (Eventos UDM)
                                  ▼
                    ┌───────────────────────────┐
                    │          ESTIGIA          │
                    │   (Unified Data Model)    │◀──────────┐
                    │  Eventos, Entidades & Río │           │
                    └─────────────┬─────────────┘           │
                                  │ (Flujo de eventos UDM)  │
                                  ▼                         │
                    ┌───────────────────────────┐           │
                    │          CARONTE          │           │ (Consultas
                    │    (Detection Engine)     │           │  de contexto)
                    │    Reglas sobre el UDM    │           │
                    └─────────────┬─────────────┘           │
                                  │ (Alertas)               │
                                  ▼                         │
                    ┌───────────────────────────┐           │
                    │           HADES           │───────────┘
                    │       (SOAR Engine)       │
                    │ Casos, Playbooks & Verdict│
                    └─────────────┬─────────────┘
                                  │ (API: consultas y tasks)
                                  ▼
                    ┌───────────────────────────┐
                    │          HERMES           │
                    │ (Librerías de terceros)   │
                    │ Contratos por tipología   │
                    └───────────────────────────┘

        ┌───────────────────────────┐
        │          HÉCATE           │  ◀── secretos, certificados y firma de la auditoría
        │   (Secretos y claves)     │      para todos los proyectos (red interna, mTLS)
        └───────────────────────────┘
```

| Proyecto | Rol SecOps | Hace | No hace |
|---|---|---|---|
| **Orthros** | SIEM de red + gestión de agentes | Gestionar el Fleet de agentes (Eurytion) y sus Policies, recopilar en cada host y en la red, reducir y parsear a UDM en origen, enviar | Detectar con reglas, decidir, actuar |
| **Echidna** | SIEM físico | Recopilar, analizar a nivel de sensor, parsear a UDM, enviar | Detectar con reglas, decidir, actuar |
| **Estigia** | UDM + Entity graph + Threat Intel | Ser dueña del esquema, validar, resolver entidades, enriquecer, mantener feeds de inteligencia y Data Tables, almacenar, servir consultas, emitir el flujo | Parsear formatos de origen, detectar, actuar |
| **Caronte** | Detection Engine | Evaluar reglas YARA-L sobre eventos UDM y detecciones, emitir Detections y notificar Alertas | Dar veredictos, agrupar o suprimir Alertas |
| **Hades** | SOAR | Agrupar Alertas en Casos, orquestar la respuesta con Playbooks, dar el Verdict, modificar el contexto de las entidades | Recopilar, parsear, detectar |
| **Hermes** | Integraciones del SOAR | Envolver servicios de terceros (mensajería, tickets, enrichment, respuesta activa) con contratos estándar por tipología | Orquestar, decidir |
| **Ténaro** | Consola (backend de la UI) | Ser la entrada de las personas: autenticarlas, reunir las vistas y pasar las acciones a cada proyecto | Detectar, decidir u orquestar |
| **Hécate** | Secret manager + PKI | Guardar los secretos de terceros, emitir las identidades de los componentes, gestionar y rotar las claves, firmar la auditoría | Exponerse a internet, inventar criptografía |

---

## 🪓 Módulos del Sistema

### 1. 🌐 ORTHROS — SIEM Virtual (Seguridad de Red)
*En la mitología, el perro de dos cabezas, hermano de Cerbero, que guardaba el rebaño de Gerión junto al pastor Euritión.*

**Orthros** vigila el dominio digital con sus dos cabezas: una mira **desde dentro de cada equipo** y la otra mira **la red entera**. Todo lo que observa lo entrega a Estigia ya normalizado en UDM.

* **Piezas:**
  * **Eurytion (agentes):** el pastor que convive con el rebaño. Un agente instalado en cada equipo (Linux, macOS, Windows...) con Sensors enchufables: procesos, conexiones por proceso, DNS y descubrimiento de red (ARP). Filtra, agrega y parsea a UDM en origen, y envía directamente a Estigia.
  * **Control plane:** gestiona el rebaño (Fleet). Asigna a cada equipo un Profile según su tipología y le publica una Policy firmada: qué Sensors activar (Flags), qué descartar o resumir (Filters, Aggregations) y cómo parsear (Parsers). Cambiar la Policy no exige redesplegar los agentes.
  * **NSM (Network Security Monitoring):** análisis central del tráfico de toda la red, a partir de una copia del tráfico del router. Reduce los paquetes a flujos, DNS y sesiones antes de enviarlos.

* **Responsabilidades:**
  * Descubrimiento de activos en la red.
  * Telemetría de cada equipo: procesos y comunicaciones.
  * Análisis del tráfico de red. Su salida son **eventos**, nunca Alertas.
  * Observación desde la red de los equipos donde no se puede instalar agente (móviles).
  * Parseo a UDM en origen y envío a Estigia, con buffer local si Estigia no está disponible. Los logs crudos no salen de su origen.

---

### 2. 🚪 ECHIDNA — SIEM Físico (Seguridad Perimetral)
*En la mitología, la madre de los guardianes y de la fuerza terrenal.*

**Echidna** gestiona los eventos del entorno físico y domótico de la vivienda y los entrega a Estigia normalizados en UDM.

* **Responsabilidades:**
  * Lectura de sensores magnéticos de apertura en puertas y ventanas.
  * Ingesta de telemetría de presencia (PIR), vibración y rotura de cristal.
  * Integración con protocolos domóticos (MQTT, Zigbee, Z-Wave).
  * Cambios de estado de la casa y sus zonas (*Armado / Desarmado / Mantenimiento*).
  * Parseo al esquema UDM y envío a Estigia, con buffer local si Estigia no está disponible.

---

### 3. 🌊 ESTIGIA — Unified Data Model
*En la mitología, el río del Inframundo por el que deben pasar todas las almas.*

**Estigia** es el cauce obligatorio de datos del sistema y la memoria común de Cerbero. Define el lenguaje en el que hablan todos los proyectos, sabe qué hay en la casa y conoce las amenazas del exterior. Todo lo que llega al río se relaciona y se enriquece antes de seguir su curso.

* **Responsabilidades:**
  * **Esquema UDM:** dueña y versionadora del esquema común de eventos y entidades, que es el contrato entre todos los proyectos.
  * **Validación:** los eventos que no cumplen el esquema van a cuarentena y no entran al río.
  * **Inventario de entidades (entity resolution):** distintas fuentes ven el mismo activo de formas distintas (MAC, IP, hostname, dirección Zigbee...) y Estigia las reconcilia en una entidad única, guardando su historia.
  * **Enriquecimiento en la ingesta:** cada evento se relaciona con su entidad y con el contexto del momento en que ocurrió (confianza del activo, estado de la casa, coincidencias con listas de amenazas).
  * **Inteligencia externa:** mantiene actualizadas, mediante procesos batch, listas de IOCs (dominios de phishing, IPs maliciosas...) y CVEs. Cuando entra un IOC nuevo, lo busca también en los eventos recientes.
  * **Data Tables:** tablas para registros temporales (white-list de invitados, equipos de la familia, horarios...), con caducidad opcional e historial de cambios. Las mantienen el propietario y Hades, y Caronte y Hades las relacionan con los eventos.
  * **Almacenamiento y consulta:** guarda los eventos normalizados. Caronte y Hades la consultan para correlacionar e investigar. Los logs crudos no salen de su origen.
  * **Flujo:** emite el río de eventos UDM que consume Caronte.
  * **Gobierno del dato:** solo Hades, tras analizar un caso, modifica el contexto de las entidades. El propietario vierte directamente sus Data Tables al río, y es responsable de no contaminarlo. Ningún cambio reescribe la historia.

---

### 4. 🚣 CARONTE — Detection Engine
*En la mitología, el barquero del Estigia que decide quién cruza hacia el Inframundo.*

**Caronte** consume el flujo de Estigia y evalúa reglas de detección sobre eventos ya normalizados. Solo cruza quien paga el óbolo, es decir, quien cumple una regla.

* **Responsabilidades:**
  * Evaluación de **reglas YARA-L** sobre eventos UDM y el contexto de las entidades en el momento en que ocurrieron. Es reactivo: cada evaluación la dispara algo que ha pasado de verdad.
  * Emisión de **Detections**: cada regla cumplida es un hecho que vuelve a Estigia y que otras reglas pueden combinar. Por ejemplo, varias señales débiles sobre el mismo equipo se combinan en una detección de más peso.
  * Ausencias con plazo ("se abre la puerta y nadie mete el código en 5 minutos") mediante **bombas**: un temporizador que explota si nadie lo desactiva a tiempo.
  * Notificación de **Alertas** a Hades cuando una regla tiene suficiente peso para actuar.
  * Contención de ráfagas (por ejemplo, un DDoS) para no colapsar el sistema. Caronte **no** decide si una Alerta es un ataque real ni cómo agruparla: eso es de Hades.

---

### 5. ⚡ HADES — SOAR Engine (Orquestación & Respuesta)
*En la mitología, el dios que gobierna el Inframundo y dicta sentencia.*

**Hades** es el núcleo de decisión y respuesta. Recibe Alertas, las investiga y decide.

* **Responsabilidades:**
  * **Casos:** agrupación de las Alertas de una regla sobre la misma entidad, durante una ventana de tiempo.
  * **Playbooks:** cada regla declara el suyo. Describen en YAML cómo se investiga, se actúa y se cierra el caso, y se versionan como las reglas. Hablan con los demás componentes con los mismos verbos (consultar, cambiar, encargar y recoger, avisar).
  * **Integraciones:** código propio para lo que no se resuelve con parámetros, que extiende las librerías de Hermes sin sobrescribirlas.
  * **Investigación:** enriquecimiento del caso consultando a Estigia (activo, segmento, historial de eventos) y a servicios externos, normalmente a través de Hermes.
  * **Aprobación humana:** pasos que esperan la respuesta del propietario por el canal (por ejemplo, aprobar el bloqueo de un dispositivo).
  * **Verdict:** amenaza real, positivo benigno (legítimo, como la tablet de un familiar), falso positivo o indeterminado. Lo puede dar una persona, la automatización o un sistema externo, y se puede corregir.
  * **Contexto:** actualiza lo que se sabe de las entidades y las Data Tables, con la opción de revertir.
  * **Eventos:** todo lo que pasa queda registrado y categorizado, y cualquier evento puede disparar otro Playbook (por ejemplo, avisar de un fallo).

---

### 6. 🪽 HERMES — Integraciones externas
*En la mitología, el mensajero que viaja entre los mundos.*

**Hermes** reúne las librerías que envuelven servicios de terceros con **contratos estándar por tipología**: un Playbook que avisa por chat funciona igual con Telegram o con otro proveedor.

* **Responsabilidades:**
  * **Contratos por tipo de servicio:** chat, email, ticketing, threat intel, control de red... con una estructura de respuesta y unos errores comunes.
  * **Librerías de proveedor:** cada una implementa esos contratos para un servicio concreto y absorbe sus cambios de API.
  * **Ejecución:** llamadas síncronas y tasks que Hades consulta después, con reintentos seguros e idempotencia.
  * **Comunicación:** directorio de contactos, plantillas y preguntas con respuesta por el canal (por ejemplo, aprobar un bloqueo).
  * **Auditoría:** registro de lo que se cambia fuera y de los datos que salen de casa.

---

### 7. 🗝️ HÉCATE — Secretos y claves
*En la mitología, la guardiana de las llaves y las puertas del inframundo.*

**Hécate** custodia lo que abre las puertas: los secretos de terceros y las claves de Cerbero. La construimos nosotros, pero con criptografía de librerías auditadas.

* **Responsabilidades:**
  * **Secretos de terceros:** cifrados por capas, con versiones, rotación y borrado criptográfico. Cada proyecto solo lee los suyos.
  * **Identidad de los proyectos:** una CA interna que emite certificados de pocos días para que se hablen entre sí con mTLS.
  * **Arranque seguro:** arranca sellada y se desbloquea con la frase del propietario; si se olvida, se recupera con partes repartidas (Shamir).
  * **Auditoría a prueba de manipulación:** firma los registros de todos los proyectos, para que nadie pueda reescribir lo que pasó.

### 8. 🕳️ TÉNARO — La entrada a Cerbero
*En la mitología, la cueva por la que se entraba al inframundo desde el mundo de los vivos: por ella bajó Heracles a buscar a Cerbero.*

**Ténaro** es la puerta por la que una persona entra en Cerbero. Autentica con passkeys (sin contraseñas), reúne lo que muestra cada proyecto y les pasa lo que la persona hace, dejando constancia de quién lo hizo. Es lo único de Cerbero que se expone hacia fuera.

---

## ❓ Decisiones Abiertas

Las decisiones de diseño y tecnología pendientes se registran en [DECISIONS.md](DECISIONS.md). La correlación entre dominios (red + físico) queda fuera de alcance por ahora.

---

## 🛠️ Estructura del Repositorio

```text
cerbero/
├── pyproject.toml   # Workspace de uv (Python)
├── shared/
│   └── contracts/   # Contratos compartidos: UDM, sobre común, API Contracts
├── orthros/
│   ├── control-plane/  # Profiles, Policies y Fleet (Python)
│   ├── eurytion/       # Agente de cada equipo (Rust)
│   └── nsm/            # Análisis del tráfico de la red (Python)
├── echidna/         # SIEM Físico (Perímetro)
├── estigia/         # Unified Data Model
├── caronte/         # Detection Engine
├── hades/           # SOAR Engine
├── hermes/          # Librerías de terceros con contrato
├── hecate/          # Secretos y claves
├── tenaro/          # Backend de la UI
└── docs/            # Documentación por componente y ADR
```

Cada proyecto Python tiene su `pyproject.toml`, su código en `src/<módulo>` con el esqueleto hexagonal (`domain`, `application`, `infrastructure`) y sus pruebas en `tests/`. Ver [ADR 0001](docs/adr/0001-monorepo-uv-workspaces.md) y [ADR 0002](docs/adr/0002-lenguajes-por-proyecto.md).

La visión transversal está en [ARCHITECTURE.md](ARCHITECTURE.md), y el detalle de cada proyecto en su documento de [docs/](docs/).

---

## 🚀 Estado del Proyecto

- [x] Definición de arquitectura funcional, reparto de responsabilidades y nomenclatura.
- [x] Arquitectura de software: principios, bounded contexts y comunicación entre proyectos.
- [x] Diseño conceptual de **Orthros** (Eurytion, control plane, NSM).
- [x] Diseño conceptual de **Estigia** (flujo de ingesta, enriquecimiento, inteligencia externa, Data Tables).
- [x] Diseño conceptual de **Caronte** (reglas YARA-L, detecciones compuestas, bombas, eventos tardíos, contención de ruido).
- [x] Diseño conceptual de **Hades** (Casos, Playbooks, Verdict, eventos, comandos).
- [x] Diseño conceptual de **Hermes** (contratos por tipología, librerías de proveedor, tasks, comunicación).
- [x] Diseño conceptual de **Hécate** (secretos, claves, identidad de los proyectos, auditoría).
- [ ] Diseño conceptual de **Echidna**.
- [ ] Diseño del esquema **UDM** en Estigia.
- [x] Documentación por componente en `docs/`.

---

## 🛡️ Licencia

Proyecto personal de ciberseguridad doméstica y código abierto para entornos *Home Lab*.
