# Feature Specification: Hécate y Orthros en Rust: la decisión documentada

**Feature Branch**: `chore/hecate-orthros-rust`

**Created**: 2026-11-08

**Status**: Draft

**Input**: User description: "Hécate y Orthros pasan a Rust: dejar documentada, completa y coherente la decisión en la rama
chore/hecate-orthros-rust, sin código ni configuración. Incluye la enmienda de la constitución (con su porqué, alternativas
e impacto), los ADRs, DECISIONS, la licencia y lo que haya que corregir en los docs. Implementar aquí es completar la
documentación, las decisiones y la constitución. La feature 003 (identidades) se pausa hasta después de migrar Hécate; la
migración de Hécate desde Python es la siguiente change, con su propio ciclo de Spec Kit."

## Clarifications

### Session 2026-11-08

- Q: ¿Qué cambios pueden llegar a `main` sin el ciclo completo de Spec Kit? → A: Dos excepciones, en su rama y con
  PR, sin carpeta en `specs/`: las correcciones que no cambian el significado de ningún documento ni tocan código ni
  configuración (erratas, enlaces rotos, formato) y las subidas de versión del toolchain o de las dependencias que no
  obligan a cambiar código.
- Q: ¿Qué tipo de rama y de commit lleva un cambio que reescribe código sin cambiar su comportamiento, como la migración
  de Hécate a Rust? → A: Un tipo nuevo, `refactor/`, con commit `refactor:`, para los cambios de código que conservan el
  comportamiento.
- Q: ¿Qué término usan la constitución y los docs para una unidad de trabajo con su ciclo de Spec Kit? → A: «Cambio»,
  para toda unidad de trabajo que entra en `main` por PR (con su ciclo y su carpeta en `specs/`, salvo las dos
  excepciones); «feature» queda para el tipo de rama `feature/` y los cambios que añaden funcionalidad.
- Q: ¿Se puede editar el cuerpo de un ADR aceptado, o la spec de un cambio ya fusionado, cuando una decisión posterior
  los deja desfasados? → A: No: de un ADR aceptado solo cambian su estado y su fila del índice, y el ADR que lo
  complementa lo dice en su propio texto; las specs de cambios fusionados son historia y no se tocan.
- Q: ¿Dónde queda registrado un cambio pausado, como la feature 003? → A: En DECISIONS, en una sección de cambios
  pausados con su carpeta de `specs/`, su rama, el motivo y el cambio al que espera; desde `main` se ve qué está pausado
  y qué números de `specs/` están ocupados.

## User Scenarios & Testing *(mandatory)*

El 2026-11-08 el propietario decidió que Hécate y todo Orthros (Eurytion, NSM y control plane) pasan a Rust y que el resto
de Cerbero sigue en Python. Este cambio deja esa decisión documentada, completa y coherente antes de migrar nada: la
constitución, los ADRs, DECISIONS, la licencia y los docs que haya que corregir. No toca código ni configuración:
implementar aquí es escribir y corregir documentación. La migración de Hécate desde Python es el cambio siguiente, con su
propio ciclo de Spec Kit, y la feature 003 (identidades) queda pausada hasta después de esa migración.

Actores:

- **Propietario**: toma las decisiones, revisa cada fase y es quien migrará después con estas reglas.
- **Agente de desarrollo**: lee la constitución, los ADRs y DECISIONS para planificar e implementar; necesita reglas sin
  ambigüedad, porque una regla ambigua ya llevó a implementar un plan aprobado en conversación sin pasar por Spec Kit.

### User Story 1 - La constitución gobierna la migración sin huecos (Priority: P1)

El propietario quiere que, antes de migrar, la constitución diga qué lenguaje usa cada pieza, cómo se eligen las
herramientas, qué papel tiene la licencia, cómo pasa por Spec Kit cada cambio (también un chore, una migración o un
cambio de documentación), cuáles son las únicas excepciones y cómo se pausa un cambio; y que la enmienda explique qué
cambia, por qué, qué alternativas se descartaron y a qué afecta.

**Why this priority**: todo plan debe cumplir la constitución. Sus huecos causaron la desviación anterior y, sin
cerrarlos, la migración no puede empezar como ciclo de Spec Kit mientras la feature 003 sigue sin fusionar.

**Independent Test**: con la constitución sola, sin preguntar al propietario, se puede responder: qué lenguaje usa cada
pieza; cómo se elige una dependencia; si una licencia puede descartar una herramienta; cómo entra la migración en Spec Kit
(rama, carpeta de `specs/`, numeración y tipo de commit); qué cambios pueden hacerse sin el ciclo; qué pasa con la
feature 003; y, para cada bloque de la enmienda, su porqué, sus alternativas y su impacto.

**Acceptance Scenarios**:

1. **Given** la constitución enmendada, **When** alguien busca el lenguaje de Hécate o de una pieza de Orthros, **Then**
   encuentra Rust, y Python para el resto, con el ADR que lo decide.
2. **Given** un cambio que no es una feature (un chore, una migración o un cambio de documentación) ni una de las dos
   excepciones, **When** empieza, **Then** la constitución dice que sigue el orden completo de Spec Kit y cómo se nombran
   su rama, su carpeta de `specs/` y su tipo de commit.
3. **Given** un cambio sin fusionar (la feature 003), **When** el propietario decide hacer antes otro cambio, **Then** la
   constitución le permite pausarlo y dice que se registra en DECISIONS.
4. **Given** el historial de enmiendas, **When** se lee la entrada de esta enmienda, **Then** cada bloque tiene qué, por
   qué, alternativas e impacto, y ningún texto dice que la migración se hace en esta rama.
5. **Given** una errata, un enlace roto, un cambio de formato o una subida de versión que no obliga a cambiar código,
   **When** se hace, **Then** la constitución dice que va en su rama y con PR, sin ciclo de Spec Kit ni carpeta en
   `specs/`.
6. **Given** la migración de Hécate, que reescribe el código sin cambiar su comportamiento, **When** empieza, **Then** la
   constitución dice que va en una rama `refactor/` y que su commit de squash es `refactor:`.
7. **Given** un plan acordado en la conversación, **When** se va a implementar, **Then** la constitución dice que ninguna
   fase se sustituye por un acuerdo en la conversación: un plan aprobado en el chat no es un plan.md.

---

### User Story 2 - Cada decisión del cambio de lenguaje tiene su ADR (Priority: P2)

El propietario quiere que cada decisión tomada con el cambio de lenguaje (qué piezas usan Rust, cómo se prueba en Rust,
con qué herramientas se construye Hécate en Rust y el permiso adicional de la licencia) tenga un ADR con contexto,
decisión, alternativas y consecuencias, y que el ADR sustituido quede marcado.

**Why this priority**: los ADRs son la fuente de las decisiones tomadas y la constitución remite a ellos; sin ellos se
pierde por qué se eligió cada cosa.

**Independent Test**: el índice de ADRs lista cada ADR con su estado; cada ADR nuevo tiene las cuatro partes; el
sustituido remite a su sustituto; todos los enlaces llevan a un fichero que existe.

**Acceptance Scenarios**:

1. **Given** el índice de ADRs, **When** se lee, **Then** lista del 0001 al 0008 con su estado, y el 0002 aparece
   sustituido por el 0005.
2. **Given** cada ADR nuevo, **When** se lee, **Then** tiene contexto, decisión, alternativas consideradas y
   consecuencias, y sus consecuencias dicen qué cambia cuando llegue la migración.
3. **Given** el ADR de las herramientas de Hécate, **When** se lee, **Then** recoge la regla de versiones del propietario:
   siempre la última estable, salvo que algo se rompa.
4. **Given** un ADR ya fusionado que una decisión posterior deja desfasado (como el 0001), **When** se revisa, **Then** su
   cuerpo no cambia, y el ADR nuevo dice en su propio texto cómo lo complementa.

---

### User Story 3 - DECISIONS refleja lo decidido, lo abierto y los riesgos (Priority: P3)

El propietario quiere que DECISIONS muestre las decisiones nuevas como tomadas, cierre las cuestiones que el cambio de
lenguaje resuelve, abra las que crea, registre los riesgos aceptados que salieron al analizarlo y recoja los cambios
pausados.

**Why this priority**: DECISIONS es el índice de lo decidido y de lo abierto; una cuestión abierta desfasada invita a
reabrir una decisión cerrada.

**Independent Test**: en DECISIONS, cada ADR nuevo está entre las decisiones tomadas; ninguna cuestión abierta contradice
un ADR; el riesgo aceptado de Shamir aparece; los riesgos de la implementación actual siguen como están; la feature 003
figura entre los cambios pausados.

**Acceptance Scenarios**:

1. **Given** DECISIONS, **When** se leen las decisiones tomadas, **Then** están los cuatro ADRs nuevos con su enlace.
2. **Given** las cuestiones abiertas, **When** se busca el lenguaje del NSM y del control plane, **Then** ya no aparece
   como abierta, y sí la nueva sobre las APIs internas en Python y el certificado del cliente.
3. **Given** los riesgos aceptados, **When** se leen, **Then** está el de Shamir sin auditoría pública, y los que se
   refieren a la implementación actual en Python la siguen describiendo.
4. **Given** los cambios pausados, **When** se leen, **Then** está la feature 003 con su carpeta de `specs/`, su rama, el
   motivo y el cambio al que espera, la migración de Hécate.

---

### User Story 4 - La licencia permite la pila elegida (Priority: P4)

El propietario quiere que la licencia del proyecto permita enlazar las librerías criptográficas elegidas, derivadas de
OpenSSL, con el texto del permiso adicional junto a la licencia y enlazado desde el README y la constitución.

**Why this priority**: sin el permiso adicional, la librería criptográfica elegida podría chocar con la licencia; es
pequeño, pero bloquea la migración.

**Independent Test**: el texto del permiso adicional está junto a la licencia, el README lo enlaza junto con su ADR, y la
regla de licencia de la constitución lo menciona.

**Acceptance Scenarios**:

1. **Given** el repositorio, **When** se busca la licencia, **Then** la licencia y su permiso adicional están juntos, y el
   README enlaza ambos y el ADR que lo decide.

---

### User Story 5 - Los docs vigentes dicen la verdad y no adelantan la migración (Priority: P5)

El propietario quiere que los docs que describen la implementación actual la sigan describiendo, sin afirmaciones falsas
y sin presentar como hecha una migración que no ha empezado.

**Why this priority**: los docs describen el diseño vigente y se actualizan al consolidar; una afirmación falsa o una
descripción prematura engañan a quien los lee.

**Independent Test**: ningún doc afirma que se usa una implementación de Shamir auditada; ningún doc describe como
existente código en Rust de Hécate u Orthros.

**Acceptance Scenarios**:

1. **Given** el doc de Hécate, ARCHITECTURE, el README o DECISIONS, **When** dicen con qué librerías se construye Hécate,
   **Then** no presentan como auditada la de Shamir y remiten al riesgo aceptado.
2. **Given** el README y el README de Orthros, **When** se leen, **Then** siguen describiendo la estructura actual, y las
   consecuencias del ADR de lenguajes los listan como actualizaciones de la migración.

---

### Edge Cases

- Un texto que habría que cambiar por la decisión pero describe la implementación actual se queda como está hasta que la
  migración lo consolide, y figura en las consecuencias del ADR que lo decide.
- Una regla o decisión que falte al documentar la decide el propietario y queda en su ADR o en la constitución; no se
  deja implícita.
- Las decisiones de diseño del intento de port descartado (detalles de la capa de aplicación, dependencias nuevas) no se
  documentan aquí: son materia del plan de la migración.
- Si dos documentos vigentes se contradicen, prevalece la constitución y el otro se corrige en este cambio. Un ADR ya
  fusionado o la spec de un cambio fusionado no se corrigen: son el registro de su fecha.
- El Sync Impact Report de la enmienda es temporal y se quita antes del commit.
- La numeración de `specs/` es global entre ramas: este cambio es el 004 porque la feature 003 existe en su propia rama;
  este cambio la registra en DECISIONS como cambio pausado.
- Una subida de versión que obliga a cambiar código, o una corrección que cambia el significado de un documento, deja de
  ser una excepción y sigue el ciclo completo.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Este cambio NO DEBE añadir ni modificar código, configuración de compilación ni manifiestos de dependencias;
  solo la constitución, los ADRs, DECISIONS, el README, los docs y el texto del permiso adicional de la licencia.
- **FR-002**: La constitución DEBE asignar el lenguaje de cada pieza: Rust (edition 2024) para Hécate y para Orthros
  (Eurytion, NSM y control plane), Python para el resto; y DEBE exigir un ADR que sustituya al vigente para cambiarlo.
- **FR-003**: La constitución DEBE fijar cómo se eligen las herramientas: por seguridad, garantías y rendimiento; en Rust,
  todo el código propio en Rust y dependencias seguras y maduras aunque envuelvan C, sin C propio ni bindings a mano; en
  Python, por legibilidad, mantenibilidad y escalabilidad.
- **FR-004**: La constitución DEBE decir que la licencia no condiciona la elección de herramientas (si la mejor tiene una
  licencia incompatible, se revisa la del proyecto con un ADR) y DEBE recoger el permiso adicional de enlace con OpenSSL y
  sus derivadas.
- **FR-005**: La constitución DEBE decir que todo cambio sigue el orden de Spec Kit, también los chores, las migraciones y
  los cambios de documentación, con dos únicas excepciones que van en su rama y con PR, sin ciclo ni carpeta en `specs/`:
  las correcciones que no cambian el significado de ningún documento ni tocan código ni configuración (erratas, enlaces
  rotos, formato) y las subidas de versión del toolchain o de las dependencias que no obligan a cambiar código. DEBE
  decir también que ninguna fase se sustituye por un acuerdo en la conversación (un plan aprobado en el chat no es un
  plan.md); que un ciclo puede ir en una rama de cualquier tipo; que la carpeta de `specs/` lleva una numeración
  global, independiente del nombre de la rama: el número siguiente al mayor de los que hay en `main` y en los cambios
  pausados de DECISIONS; que el tipo del commit sigue al de la rama; y que hay un tipo de rama `refactor/`, con commit
  `refactor:`, para los cambios de código que conservan el comportamiento, como la migración de Hécate.
- **FR-006**: La constitución DEBE permitir que el propietario pause un cambio sin fusionar para hacer antes otro, y DEBE
  decir que se registra en DECISIONS, en una sección de cambios pausados con su carpeta de `specs/`, su rama, el motivo
  y el cambio al que espera; la feature 003 queda pausada hasta que se fusione la migración de Hécate.
- **FR-007**: La entrada de la enmienda en el historial DEBE recoger, por bloque, qué cambia, por qué, qué alternativas se
  descartaron y a qué afecta; la versión DEBE seguir semver y el Sync Impact Report DEBE quitarse antes del commit.
- **FR-008**: Ningún documento DEBE decir que la migración, o la consolidación de docs que trae, se hace en esta rama.
- **FR-009**: Cada decisión del cambio de lenguaje DEBE tener un ADR con contexto, decisión, alternativas y
  consecuencias: lenguajes (sustituye al 0002), pruebas en Rust, herramientas de Hécate en Rust y permiso adicional de
  la licencia. El ADR sustituido DEBE remitir a su sustituto y el índice DEBE listarlos todos con su estado.
- **FR-010**: El ADR de las herramientas de Hécate DEBE recoger la regla de versiones del propietario: la última versión
  estable del toolchain y de las dependencias, salvo que algo se rompa.
- **FR-011**: Las consecuencias del ADR de lenguajes DEBEN listar los documentos que cambian cuando la migración se
  consolide: el README, el README de Orthros, el doc de Hécate y el riesgo de memoria de DECISIONS; y DEBEN decir cómo
  complementan al ADR 0001 (workspaces de Cargo fuera del de uv), que no se edita.
- **FR-012**: DECISIONS DEBE listar las decisiones nuevas como tomadas, con su enlace; DEBE cerrar la cuestión del lenguaje
  del NSM y del control plane; DEBE mantener abierta la de las APIs internas en Python y el certificado del cliente; DEBE
  registrar como riesgo aceptado que ninguna implementación de Shamir tiene una auditoría pública; y DEBE registrar la
  feature 003 entre los cambios pausados.
- **FR-013**: Los riesgos aceptados que describen la implementación actual DEBEN seguir describiéndola hasta la
  migración.
- **FR-014**: El repositorio DEBE contener el texto del permiso adicional junto a la licencia, y el README DEBE enlazarlo
  junto con su ADR.
- **FR-015**: Ningún doc DEBE presentar como auditada la implementación de Shamir: donde el doc de Hécate, ARCHITECTURE, el
  README o DECISIONS dicen que Hécate usa primitivas de librerías auditadas, DEBE constar la excepción de Shamir y remitir
  al riesgo aceptado.
- **FR-016**: Los docs que describen la implementación actual NO DEBEN describir como existente código en Rust de Hécate
  ni de Orthros hasta que la migración se consolide.
- **FR-017**: Todo enlace entre los documentos de este cambio DEBE llevar a un fichero o sección que existe.
- **FR-018**: La constitución y los docs DEBEN llamar «cambio» a toda unidad de trabajo que entra en `main` por PR, con
  su ciclo de Spec Kit y su carpeta en `specs/` salvo las dos excepciones de FR-005, y reservar «feature» para el tipo de
  rama `feature/` y los cambios que añaden funcionalidad.
- **FR-019**: La constitución DEBE decir que de un ADR ya fusionado solo cambian su estado y su fila del índice, que el
  ADR que lo complementa o lo sustituye lo dice en su propio texto, y que las specs de cambios fusionados no se
  modifican. Este cambio no edita ningún ADR fusionado salvo el estado del 0002; los ADRs 0005 a 0008 son suyos y se
  pueden corregir hasta que se fusione.

### Key Entities *(include if feature involves data)*

- **Constitución**: las reglas que no se negocian; tiene una versión semver y un historial de enmiendas con qué, por qué,
  alternativas e impacto.
- **ADR**: una decisión tomada, con estado (aceptada o sustituida), contexto, decisión, alternativas y consecuencias; un
  índice los lista todos. Una vez fusionado, solo cambian su estado y su fila del índice.
- **DECISIONS**: las decisiones tomadas (con su ADR), las cuestiones abiertas, las desviaciones, los riesgos aceptados y
  los cambios pausados.
- **Permiso adicional de la licencia**: excepción a la licencia del proyecto que permite enlazar ciertas librerías.
- **Doc de componente**: describe el diseño vigente de un componente; se actualiza al consolidar lo implementado.
- **Cambio** (la entrada lo llama «change»): unidad de trabajo que entra en `main` por PR desde su rama; salvo las dos
  excepciones de FR-005, tiene su ciclo de Spec Kit y su carpeta en `specs/` con numeración global. Una feature es un
  cambio que añade funcionalidad.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Una lectura completa de los documentos de este cambio no encuentra ninguna contradicción entre ellos. No
  cuentan los ADRs ya fusionados ni las specs de cambios fusionados, que son el registro de su fecha, ni los textos que
  describen la implementación actual hasta que la migración los consolide.
- **SC-002**: El 100 % de las decisiones del cambio de lenguaje tiene su ADR con las cuatro partes, en el índice y en
  DECISIONS.
- **SC-003**: Al cerrar la rama, ningún fichero de código, configuración de compilación o manifiesto de dependencias
  difiere de `main`.
- **SC-004**: Ningún documento afirma que existe código en Rust de Hécate u Orthros ni que la migración se hace en esta
  rama.
- **SC-005**: El propietario puede empezar la migración de Hécate como ciclo de Spec Kit sin ninguna pregunta de proceso
  que la constitución deje sin respuesta: rama, carpeta, numeración, tipo de commit y cambio pausado.
- **SC-006**: El 100 % de los enlaces de los documentos de este cambio llevan a un fichero o sección que existe.

## Assumptions

- Las decisiones las tomó el propietario el 2026-11-08: Rust para Hécate y Orthros, cómo se eligen las herramientas, la
  licencia y el permiso de OpenSSL, las herramientas de Hécate en Rust y la regla de versiones. Este cambio las documenta;
  no las reabre.
- «Rust» y los nombres de librerías aparecen porque son el contenido de las decisiones documentadas, no una elección de
  implementación de este cambio.
- La documentación que ya existe sin commitear en la rama (constitución 1.2.0, ADRs 0005 a 0008, el estado del ADR 0002,
  el índice de ADRs, DECISIONS, README y permiso adicional de la licencia) es el punto de partida: este cambio la revisa
  y la completa.
- La migración de Hécate (features 001 y 002) es el cambio siguiente, el 005, con su propio ciclo; la conversión de Orthros
  y la feature 003 van después. Las herramientas concretas de Orthros se deciden al migrarlo (diseño por categoría).
- La rama se llama `chore/hecate-orthros-rust` y su commit de squash es `chore:`.
- No hay dependencias externas: todo el trabajo está en documentos del repositorio.
