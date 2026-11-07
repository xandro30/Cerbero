# Cerbero Constitution

Cerbero es una réplica local de una plataforma SecOps para aprender y proteger la casa. Esta
constitución recoge las reglas que no se negocian al especificar, planificar e implementar.
El diseño vive en [ARCHITECTURE](../../ARCHITECTURE.md) y en [docs/](../../docs/); aquí solo
está lo que todo plan debe cumplir.

## Core Principles

### I. Componentes independientes; solo se comparte el contrato

- Cada componente (Orthros, Echidna, Estigia, Caronte, Hades, Hermes, Hécate, Ténaro) DEBE
  poder desplegarse, arrancar y evolucionar por separado. Activar o desactivar uno NO DEBE
  obligar a modificar los demás.
- Lo único compartido es `shared/contracts` (`cerbero-contracts`). Un componente puede
  depender de los contratos, pero NO DEBE depender del código de otro componente.
- Cada componente es un bounded context con su propio modelo; la traducción ocurre en su
  frontera.

**Por qué:** es lo que permite construir Cerbero pieza a pieza y que la caída de una no
arrastre al resto.

### II. Contrato primero

- Ningún cambio que cruce componentes se implementa sin su contrato escrito antes: esquema
  UDM, contrato de Alert, API Contract (OpenAPI) o contrato de tipo de servicio de Hermes.
- El esquema UDM lo define y versiona Estigia. Cada evento declara su versión y los cambios
  DEBEN ser compatibles hacia atrás.
- Las APIs internas de todos los componentes comparten estructura de respuesta y códigos de
  error, publican sus API Contracts y su `/capabilities`, llevan la versión mayor en la ruta
  y exigen `Idempotency-Key` en las peticiones con efecto. No hay excepciones: si un
  componente (p. ej. Echidna al diseñarse) necesita algo que el contrato común no cubre, se
  revisa el contrato común.
- El contrato común aplica a las APIs **entre componentes**. Un canal local de
  administración de un componente (p. ej. el socket Unix de Hécate), sin superficie de red y
  cuyo único cliente es la CLI del propio componente, DEBE usar el sobre y los códigos de
  error comunes, pero no publica API Contracts ni `/capabilities` ni exige `Idempotency-Key`;
  su seguridad viene del acceso local.
- Quien publica eventos tras cambiar su estado DEBE usar Transactional Outbox. Todo
  consumidor DEBE ser idempotente.

**Por qué:** los contratos son la única dependencia entre componentes; si se rompen, se
rompe la independencia del principio I.

### III. Arquitectura hexagonal con intensidad justificada

- Todo componente sigue el esqueleto `domain / application / infrastructure` (paquetes en
  Python, módulos en Rust) con la regla de dependencia
  `infrastructure → application → domain`. El dominio NO DEBE conocer brokers,
  bases de datos ni protocolos.
- DDD táctico, CQRS y Event Sourcing se aplican solo donde los fija la tabla de intensidad de
  ARCHITECTURE. Aplicarlos en otro sitio, o añadir piezas, proyectos o capas, DEBE
  justificarse en el Complexity Tracking del plan.
- Se diseña por categoría (esqueleto y contrato de la categoría); el detalle de un proveedor
  concreto solo se aborda al implementar ese proveedor.

**Por qué:** mismo estilo en todos los componentes para moverse entre ellos sin fricción, y
complejidad solo donde el dominio la paga.

### IV. Pruebas primero (NO NEGOCIABLE)

- TDD: la prueba se escribe y falla antes de implementar (Red-Green-Refactor).
- Cada comportamiento cubre happy path, error path y casos límite, en el nivel que
  corresponda: unit, integración, API o E2E.
- Todo contrato tiene pruebas de contrato. Todo proveedor de Hermes pasa las pruebas de
  conformidad de su tipo de servicio.
- Cada tipo de servicio de Hermes tiene un proveedor `fake` para la CI y los entornos de
  prueba, de modo que los Playbooks se prueban sin tocar servicios reales. Un `fake` NUNCA
  se despliega en producción.
- Una tarea no está terminada si sus pruebas no pasan.

**Por qué:** Cerbero es un sistema de seguridad; un fallo silencioso es peor que no tenerlo.

### V. Seguridad por diseño

- Ningún secreto se guarda como valor en código, configuración ni repositorio: se referencia
  con `secret://` y lo resuelve Hécate.
- Ningún repositorio guarda contactos ni valores de variables: viven en la base de datos de la
  aplicación que los usa.
- Las llamadas entre componentes van por mTLS con identidad SPIFFE y un JWT corto firmado por
  quien llama y ligado al canal. Ninguna API interna acepta llamadas sin autenticar.
- La única superficie pública es Ténaro, tras un proxy inverso con TLS. Hécate solo es
  interna. Las personas se autentican con passkeys.
- Todo cambio de estado queda atribuido a quien lo pidió (componente, Playbook o persona).
- Las desviaciones de seguridad se registran como riesgo aceptado en DECISIONS.

**Por qué:** una plataforma de seguridad que se puede atacar desde dentro no protege nada.

### VI. Degradación elegante

- El camino principal `Orthros/Echidna → Estigia → Caronte → Hades` es asíncrono por eventos.
  Ningún componente bloquea su trabajo esperando a otro.
- Si un componente cae, los demás siguen con lo que tienen y se ponen al día al volver: las
  cabezas bufferizan, los consumidores retoman y las tasks no terminales se reanudan.
- Todo componente nuevo o cambiado DEBE declarar su comportamiento ante fallos, y los
  reintentos solo se hacen cuando son seguros.

**Por qué:** en un home lab las caídas son normales; Cerbero debe sobrevivirlas sin perder
eventos.

### VII. SecOps como referencia; desviaciones explícitas

- Cada problema se contrasta primero con cómo lo resuelve Google SecOps (documentación
  oficial) cuando SecOps tiene un equivalente.
- Apartarse de la referencia es legítimo, pero DEBE quedar registrado en las desviaciones de
  DECISIONS con su motivo; por ejemplo, Hermes gobierna las integraciones de forma más
  estricta que SecOps SOAR.
- Donde SecOps no tiene equivalente, DECISIONS nombra la referencia que se toma (Hécate:
  OpenBao / Vault, step-ca y SPIFFE) o deja constancia de que el diseño es propio.
- La diferencia de base se mantiene: las cabezas producen UDM de forma nativa y el dato
  original no viaja a Estigia.

**Por qué:** el objetivo es aprender SecOps; lo que se haga distinto tiene que ser una
decisión, no un descuido.

## Restricciones técnicas y de diseño

- **Repositorio:** monorepo con un workspace virtual de uv ([ADR 0001](../../docs/adr/0001-monorepo-uv-workspaces.md)).
  Cada componente Python es un miembro con su `pyproject.toml`, su código en `src/<módulo>` y
  sus pruebas en `tests/`.
- **Lenguajes:** cada pieza usa el lenguaje que le asigna su ADR
  ([0002](../../docs/adr/0002-lenguajes-por-proyecto.md)): Rust (edition 2024) para Eurytion y
  Python 3.15 con uv y `uv_build` para el resto. Pasar otra pieza a Rust exige un ADR. Los
  contratos DEBEN poder usarse desde ambos lenguajes.
- **Nueva tecnología** (broker, persistencia, framework, formato de contratos): se decide en
  un ADR antes de usarla en un plan. Las decisiones abiertas de DECISIONS no se resuelven de
  forma implícita dentro de una feature.
- **Nombres:** nombres de la mitología griega solo para aplicaciones y componentes. Los
  conceptos del dominio usan los términos SecOps descriptivos del [GLOSSARY](../../GLOSSARY.md).
- **Licencia:** `AGPL-3.0-or-later` ([LICENSE](../../LICENSE)). Todo paquete la declara en su
  manifiesto, y una dependencia nueva DEBE tener una licencia compatible con ella.
- **Idioma:** código, identificadores, ramas y mensajes de commit en inglés; documentación en
  español, manteniendo en inglés los términos técnicos consolidados.
- **Operación:** OpenTelemetry para trazas y métricas, logs estructurados a stdout y
  configuración por entorno (12-factor). Los
  secretos nunca van en variables de entorno: siempre como referencia `secret://`.

## Flujo de desarrollo y documentación

- **Fuentes de verdad:**
  - `docs/<componente>.md` y ARCHITECTURE describen el diseño vigente, sin MVP, roadmap ni
    futuro.
  - `specs/<NNN-feature>/` describe un cambio: qué se quiere y cómo se va a hacer.
  - DECISIONS recoge las desviaciones y lo abierto; los ADR, las decisiones tomadas.
- **Consolidación:** al cerrar una feature, lo que cambia el diseño se lleva a
  `docs/<componente>.md` (y a ARCHITECTURE si es transversal), y lo que resolvió una
  decisión abierta pasa a un ADR. Los docs se actualizan solo cuando lo implementado está
  consolidado.
- **Orden:** specify → clarify si hay dudas → plan → tasks → analyze → implement → converge. Cada
  fase la revisa el propietario antes de pasar a la siguiente.
- **Specs:** describen qué y por qué, sin tecnología. Lo que no está claro se marca como
  `NEEDS CLARIFICATION` y no se inventa.
- **Planes:** pasan el Constitution Check antes de investigar y otra vez tras diseñar.
- **Git (GitHub Flow):** `main` siempre estable; todo cambio se hace en una rama corta que
  entra en `main` por PR en GitHub con squash merge, también con un solo desarrollador. No se
  hace commit directo a `main`.
- **Ramas:** tipos `feature/`, `fix/`, `chore/` y `docs/`. Una feature de Spec Kit se llama
  `feature/<NNN>-<nombre>`, con el mismo `<NNN>-<nombre>` que su carpeta en `specs/`; el resto,
  `<tipo>/<nombre>`.
- **Una cosa a la vez:** se trabaja en el checkout principal, en una sola rama activa; cada
  feature de Spec Kit va en su propia rama y no se empieza otra hasta fusionarla. Sin git
  worktrees.
- **Commits:** Conventional Commits en inglés. El commit de squash usa el tipo que
  corresponde a la rama (`feature/` → `feat:`, `fix/` → `fix:`, `chore/` → `chore:`,
  `docs/` → `docs:`).

## Governance

- Esta constitución prevalece sobre cualquier otra práctica del proyecto. No duplica el
  diseño: si ARCHITECTURE o un doc de componente contradicen un principio, se corrige uno de
  los dos de forma explícita.
- **Enmiendas:** con `/speckit-constitution`, revisadas y aprobadas por el propietario. Si
  una enmienda invalida planes o specs en curso, se revisan antes de continuar.
- **Justificación obligatoria:** toda enmienda DEBE quedar justificada por escrito en el
  historial de abajo: qué cambia, por qué, qué alternativas se descartaron y qué artefactos
  afecta. Lo mismo vale para cualquier regla de buenas prácticas: cambiar una decisión tomada
  exige un ADR nuevo que sustituya al anterior con su contexto y sus opciones. Un cambio sin
  justificación no se aprueba.
- **Versionado semántico:** MAJOR al eliminar o redefinir un principio; MINOR al añadir un
  principio o sección o ampliar de forma material su alcance; PATCH para aclaraciones y
  redacción.
- **Cumplimiento:** el Constitution Check del plan es una puerta obligatoria; toda excepción
  se justifica en el Complexity Tracking. `/speckit-analyze` verifica spec, plan y tasks
  contra estos principios antes de implementar.

### Historial de enmiendas

- **1.0.0 (2026-11-06) — Ratificación.** Siete principios, restricciones, flujo y gobierno,
  derivados de ARCHITECTURE, DECISIONS, los ADR 0001-0002 y las convenciones del proyecto.
- **1.0.0 (2026-11-07) — Sin worktrees (PR #2).** Qué: "un worktree por rama" pasa a "una
  cosa a la vez": una rama activa en el checkout principal y una feature de Spec Kit por rama.
  Por qué: los ficheros de un worktree fuera del proyecto eran difíciles de revisar, el
  propietario hace el seguimiento y su plan no permite sesiones en paralelo. Alternativas:
  worktrees dentro del repositorio en una carpeta ignorada (descartada: aislamiento que no se
  necesita sin trabajo en paralelo); seguir fuera y añadirlos al IDE (descartada: fricción en
  cada rama). Afecta: Flujo de desarrollo. Se hizo sin subir la versión; esta entrada corrige
  la omisión y queda recogida en la 1.1.0.
- **1.1.0 (2026-11-07) — Alcance del contrato común y justificación obligatoria.** Origen: el
  `/speckit-analyze` de la feature 001 (hallazgo D1) y la petición del propietario.
  - Principio II. Qué: el contrato común aplica a las APIs entre componentes; los canales
    locales de administración usan el sobre y los códigos comunes, sin API Contracts,
    `/capabilities` ni `Idempotency-Key`. Por qué: un canal local no cruza componentes, no
    tiene superficie de red y su único cliente es la CLI del propio componente; exigirle el
    contrato completo no protege nada, y sin la aclaración cada feature tendría que
    reinterpretar el principio. Alternativas: dejar la interpretación en cada plan
    (descartada: la constitución no debe depender de interpretaciones); exigir el contrato
    completo también en local (descartada: coste sin beneficio). Afecta: plan de la feature
    001 (Constitution Check, principio II).
  - Governance. Qué: justificación escrita obligatoria para toda enmienda y para cambiar una
    regla de buenas prácticas, con este historial. Por qué: la constitución es la referencia
    no negociable; sin el porqué, una enmienda futura no se puede evaluar ni revertir con
    criterio. Alternativas: justificar solo en la PR (descartada: queda fuera del documento
    que se consulta). Afecta: el procedimiento de enmienda.

**Version**: 1.1.0 | **Ratified**: 2026-11-06 | **Last Amended**: 2026-11-07
