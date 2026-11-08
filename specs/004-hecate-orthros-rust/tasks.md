---

description: "Tareas del cambio 004: la decisión de Rust para Hécate y Orthros, documentada"
---

# Tasks: Hécate y Orthros en Rust: la decisión documentada

**Input**: Design documents from `/specs/004-hecate-orthros-rust/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[quickstart.md](quickstart.md)

**Tests**: las comprobaciones V1 a V11 de [quickstart.md](quickstart.md) hacen de pruebas (principio IV, research R9).
En cada historia se ejecutan primero, y deben fallar las que miden algo pendiente; al terminar la historia, pasan.

**Organization**: las tareas se agrupan por historia de usuario para escribir y comprobar cada una por separado.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US5)
- Rutas relativas a la raíz del repositorio

## Convenciones para todas las tareas

- Solo documentación: no cambia ningún fichero de código, configuración de compilación ni manifiesto (FR-001).
- Documentación en español, con los términos técnicos consolidados en inglés. Los textos nuevos siguen el estilo del
  documento que los recibe (tablas, viñetas, longitud de línea).
- «Cambio» es la unidad de trabajo; «feature», el tipo de rama `feature/` y lo que añade funcionalidad (FR-018).
- No se edita el cuerpo de los ADRs ya fusionados (del 0002 solo el estado, que ya está cambiado) ni las specs 001 y
  002 (FR-019). Los textos que describen la implementación actual se quedan como están (research R12).
- Ningún texto dice que la migración, o la consolidación de docs que trae, se hace en esta rama (FR-008).
- La constitución solo se enmienda con `/speckit-constitution`; la versión sigue siendo 1.2.0, MINOR (research R1).
- El propietario hace el commit, el push y la PR; aquí no se ejecuta ningún comando de git que cambie el estado.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Comprobar que la rama activa es `chore/hecate-orthros-rust` (`git branch --show-current`) y que `.specify/feature.json` apunta a `specs/004-hecate-orthros-rust`
- [X] T002 Ejecutar la línea base de `specs/004-hecate-orthros-rust/quickstart.md` (V1 a V10) y comprobar que coincide con lo marcado como «Antes»: pasan V1, V3 y V9; fallan V2, V4, V5, V6, la primera orden de V7, la tercera de V8 y V10

---

## Phase 2: Foundational (Blocking Prerequisites)

No hay tareas fundacionales: cada historia escribe documentos distintos y no necesita a las demás para comprobarse. Los
enlaces entre ellas (la regla de pausa de la constitución con la sección de DECISIONS, la excepción de Shamir con el
riesgo aceptado) se verifican al final, en la Phase 8.

---

## Phase 3: User Story 1 - La constitución gobierna la migración sin huecos (Priority: P1) 🎯 MVP

**Goal**: la constitución dice qué lenguaje usa cada pieza, cómo se eligen las herramientas, qué papel tiene la
licencia, cómo pasa por Spec Kit cada cambio y cuáles son las excepciones, cómo se pausa un cambio y qué no se edita; y
su entrada 1.2.0 justifica cada bloque.

**Independent Test**: con la constitución sola se responden las preguntas de la lectura guiada de V6 (SC-005) y las del
Independent Test de US1 en la spec; V5 y V6 pasan.

### Tests for User Story 1 ⚠️

- [X] T003 [US1] Ejecutar V5 y V6 de `specs/004-hecate-orthros-rust/quickstart.md` y anotar lo que encuentran antes de enmendar: V5, cinco reglas de `.specify/memory/constitution.md` y dos sitios de `README.md`; V6, ninguna regla

### Implementation for User Story 1

Las tareas T004 a T012 se aplican a `.specify/memory/constitution.md` con `/speckit-constitution`, en una invocación o
en varias; cada una se marca al comprobar su texto en el fichero. El Sync Impact Report temporal describe los cambios
con las palabras nuevas y no cita los textos que se sustituyen («feature de Spec Kit», la rama `chore/hecate-rust`),
para que V2 y V5 no lo encuentren antes de quitarlo en T042.

- [X] T004 [US1] En la regla «Fuentes de verdad» de `.specify/memory/constitution.md`: `specs/<NNN>-<nombre>/` describe un cambio, y `<NNN>` es el número siguiente al mayor de los que hay en `main` y en los cambios pausados de DECISIONS, sea cual sea la rama (FR-005); un ADR ya fusionado solo cambia su estado y su fila del índice, y el ADR que lo complementa o lo sustituye lo dice en su propio texto; la spec de un cambio fusionado no se modifica (FR-019, research R3)
- [X] T005 [US1] En la regla «Orden» de `.specify/memory/constitution.md`, manteniendo que el propietario revisa cada fase: todo cambio sigue el orden, sea cual sea su tipo de rama (también un chore, una migración o un cambio de documentación); ninguna fase se sustituye por un acuerdo en la conversación, porque un plan aprobado en el chat no es un plan.md; solo dos excepciones van en su rama y con PR, sin ciclo ni carpeta en `specs/`: las correcciones que no cambian el significado de ningún documento ni tocan código ni configuración (erratas, enlaces rotos, formato) y las subidas de versión del toolchain o de las dependencias que no obligan a cambiar código; si la subida obliga a cambiar código, o la corrección cambia el significado, sigue el ciclo (FR-005)
- [X] T006 [US1] En la regla «Ramas» de `.specify/memory/constitution.md`: los tipos pasan a ser `feature/`, `fix/`, `refactor/`, `chore/` y `docs/`, con `refactor/` para los cambios de código que conservan el comportamiento, como una migración de lenguaje; un ciclo de Spec Kit puede ir en una rama de cualquier tipo; una rama `feature/` se llama `feature/<NNN>-<nombre>`, con el mismo `<NNN>-<nombre>` que su carpeta en `specs/`, y el resto, `<tipo>/<nombre>` (FR-005, FR-018)
- [X] T007 [US1] En la regla «Una cosa a la vez» de `.specify/memory/constitution.md`: se trabaja en el checkout principal, en una sola rama activa; cada cambio va en su propia rama y no se empieza otro hasta fusionarlo, salvo que el propietario lo pause; un cambio pausado se registra en la sección «Cambios pausados» de DECISIONS con su carpeta de `specs/`, su rama, el motivo y el cambio al que espera; mientras un cambio reanudado no se fusiona, su fila sigue en DECISIONS y solo reserva su número, y la retira la PR que lo fusiona; se mantiene «Sin git worktrees» (FR-006, FR-018)
- [X] T008 [US1] En la regla «Commits» de `.specify/memory/constitution.md`: añadir `refactor/` → `refactor:` a la correspondencia entre el tipo de rama y el tipo del commit de squash (FR-005)
- [X] T009 [US1] En `.specify/memory/constitution.md`, terminología: «dentro de una feature» → «dentro de un cambio» en la regla «Nueva tecnología» y «al cerrar una feature» → «al cerrar un cambio» en «Consolidación»; con T004, T006 y T007, ninguna regla llama ya «feature» a la unidad de Spec Kit. Las entradas 1.0.0 y 1.1.0 del historial no se tocan (FR-018, research R8)
- [X] T010 [US1] En la entrada 1.2.0 del historial de `.specify/memory/constitution.md`, bloque de lenguajes y repositorio: «Hécate, que se porta a Rust en un cambio propio, con paridad de comportamiento de las features 001 y 002» en lugar de la rama `chore/hecate-rust`; «Afecta» lista el ADR 0002 (sustituido por el 0005); DECISIONS, donde se cierra el lenguaje del NSM y del control plane; la feature 003, pausada en DECISIONS y que se replanifica en Rust; y, al consolidar la migración, el README, el README de Orthros, el doc de Hécate, el riesgo de memoria de DECISIONS y el workspace de uv. Ni ARCHITECTURE ni el ADR 0001 (FR-007, FR-008, research R7)
- [X] T011 [US1] En la entrada 1.2.0 de `.specify/memory/constitution.md`, bloque de herramientas y licencia: «Afecta» dice «LICENSE-EXCEPTION y el README (permiso adicional)» en lugar de «LICENSE y README», porque LICENSE no cambia (ADR 0008), y «la política de licencias de cargo-deny, que se crea con la migración» en lugar de «la configuración de cargo-deny» (FR-007, research R7)
- [X] T012 [US1] Añadir a la entrada 1.2.0 de `.specify/memory/constitution.md` el bloque «Flujo», con el título de la entrada ampliado a «Rust para Orthros y Hécate, y flujo de los cambios» y el origen dado por bloque (FR-007, research R7). Origen: el port de Hécate implementado desde un plan aprobado en la conversación, que el propietario descartó, y el clarify y el plan del cambio 004. Qué: las dos excepciones al ciclo; que ningún acuerdo en la conversación sustituye a una fase; la numeración global de `specs/`; la rama `refactor/` y su commit; la pausa de cambios en DECISIONS; el término «cambio»; y que los ADRs y las specs fusionados no se editan. Por qué: el port se implementó sin spec, plan ni tareas; la constitución no decía cómo entra en Spec Kit un cambio que no es una feature, ni permitía empezar otro con la feature 003 sin fusionar; «feature» nombraba a la vez un tipo de rama y cualquier unidad de trabajo; y nada impedía reescribir un ADR aceptado. Alternativas: un ciclo reducido para los chores (descartada: deja sin revisar lo que puede cambiar código o decisiones); todo cambio sin excepciones (descartada: una errata o cada versión estable de Rust necesitarían un ciclo entero); dejar implícito que un acuerdo en el chat no sustituye a una fase (descartada: ya falló una vez); `chore/` o `feature/` para la migración (descartadas: `chore:` es para lo que no toca código ni pruebas, y `feat:` anunciaría una funcionalidad que no existe); registrar la pausa en la spec del cambio pausado (descartada: no se ve desde `main` y obliga a tocar dos ramas); «feature» o «change» como término (descartadas: doble sentido, o un término en inglés que no está consolidado); ADRs vivos que se actualizan (descartada: se pierde qué se decidió y cuándo). Afecta: la sección de flujo, el README («Cómo se desarrolla» y el árbol de `specs/`), DECISIONS (cambios pausados) y la feature 003
- [X] T013 [US1] Verificar en `.specify/memory/constitution.md`, sin cambiarlas, que las restricciones «Lenguajes» y «Herramientas» cumplen FR-002 y FR-003: Rust (edition 2024) para Hécate y Orthros (Eurytion, NSM y control plane) y Python para el resto, con un ADR que sustituya al vigente para cambiarlo, y los criterios para elegir herramientas. La restricción «Licencia» (FR-004) la verifica T031
- [X] T014 [US1] En `README.md`: en «Cómo se desarrolla» (línea 232), «Cada cambio parte de una especificación…» y «Los artefactos de cada cambio…», con una frase para las dos excepciones (las correcciones que no cambian el significado, como erratas, enlaces rotos o formato, y las subidas de versión que no obligan a tocar código van directas a su PR); y en el árbol (línea 220), `# Artefactos de Spec Kit de cada cambio (spec, plan, tareas)` (FR-005, FR-018)
- [X] T015 [US1] Ejecutar V5 y V6 de `specs/004-hecate-orthros-rust/quickstart.md` (V5: solo las entradas 1.0.0 y 1.1.0 del historial; V6: las reglas de T004 a T008) y, con la constitución sola, responder las preguntas del Independent Test de US1: qué lenguaje usa cada pieza; cómo se elige una dependencia; si una licencia puede descartar una herramienta; cómo entra la migración en Spec Kit (la lectura guiada de SC-005); qué cambios pueden hacerse sin el ciclo; qué pasa con la feature 003; y, para cada bloque de la entrada 1.2.0, su porqué, sus alternativas y su impacto

**Checkpoint**: la constitución responde sola todas las preguntas de proceso de la migración.

---

## Phase 4: User Story 2 - Cada decisión del cambio de lenguaje tiene su ADR (Priority: P2)

**Goal**: cada decisión tiene su ADR completo; el ADR 0005 lista lo que se actualiza al consolidar la migración y dice
cómo complementa al 0001; ningún ADR ya fusionado cambia su cuerpo.

**Independent Test**: V8 pasa: los ADRs fusionados y las specs 001 y 002 son iguales que en `main`, salvo el estado del
0002, y el ADR 0005 enlaza el 0001 y lista los documentos de la migración.

### Tests for User Story 2 ⚠️

- [X] T016 [US2] Ejecutar V8 de `specs/004-hecate-orthros-rust/quickstart.md` y anotar que la tercera orden no da salida en `docs/adr/0005-rust-para-orthros-y-hecate.md`

### Implementation for User Story 2

- [X] T017 [US2] En `docs/adr/0005-rust-para-orthros-y-hecate.md`, viñeta «Repositorio» de las consecuencias: añadir que complementa al ADR 0001 (enlazado a `0001-monorepo-uv-workspaces.md`), que sigue vigente para los proyectos en Python y no se edita (FR-011, FR-019)
- [X] T018 [US2] En `docs/adr/0005-rust-para-orthros-y-hecate.md`, nueva viñeta de consecuencias, «Al consolidar la migración», con los documentos que se actualizan entonces: el README (el árbol del repositorio y el párrafo de los proyectos en Python, con su referencia al ADR 0002), el README de Orthros (la tabla de lenguajes), el doc de Hécate (las librerías del `CryptoProvider` en los apartados 1 y 7 y el riesgo de memoria del apartado 12) y el riesgo de memoria de DECISIONS (FR-011, research R6)
- [X] T019 [US2] En `docs/adr/0005-rust-para-orthros-y-hecate.md`, viñeta «Riesgo de memoria»: el riesgo aceptado de DECISIONS se reescribe en esos términos al consolidar la migración (FR-013)
- [X] T020 [P] [US2] Revisar `docs/adr/0006-pruebas-rust-cargo-proptest.md`: las cuatro partes, los enlaces, que sus consecuencias dicen qué cambia cuando llegue la migración (US2, escenario 2) y ningún texto que diga que la migración se hace en esta rama; corregir solo lo que contradiga la spec (FR-008, FR-009)
- [X] T021 [P] [US2] Revisar `docs/adr/0007-pila-hecate-rust.md`: las cuatro partes, la regla de versiones (la última estable del toolchain y de las dependencias, salvo que algo se rompa), los enlaces y que sus consecuencias dicen qué cambia cuando llegue la migración (US2, escenario 2); corregir solo lo que contradiga la spec (FR-009, FR-010)
- [X] T022 [P] [US2] Revisar `docs/adr/0008-excepcion-openssl-agpl.md`: las cuatro partes, los enlaces y que sus consecuencias dicen qué cambia cuando llegue la migración (US2, escenario 2); corregir solo lo que contradiga la spec (FR-009)
- [X] T023 [P] [US2] Revisar `docs/adr/README.md`: lista del 0001 al 0008 con su estado y el 0002 sustituido por el 0005 (FR-009)
- [X] T024 [US2] Ejecutar V8 de `specs/004-hecate-orthros-rust/quickstart.md`: las dos primeras órdenes, como antes; la tercera, con el enlace al 0001 y la lista de documentos

**Checkpoint**: los ADRs explican cada decisión y lo que se actualizará al migrar.

---

## Phase 5: User Story 3 - DECISIONS refleja lo decidido, lo abierto y los riesgos (Priority: P3)

**Goal**: DECISIONS muestra las decisiones tomadas, las abiertas, los riesgos aceptados y los cambios pausados.

**Independent Test**: V7 pasa: la sección de cambios pausados lista la feature 003, y el riesgo de memoria sigue
describiendo Python.

### Tests for User Story 3 ⚠️

- [X] T025 [US3] Ejecutar V7 de `specs/004-hecate-orthros-rust/quickstart.md` y anotar que `DECISIONS.md` no tiene la sección de cambios pausados

### Implementation for User Story 3

- [X] T026 [US3] Añadir al final de `DECISIONS.md`, tras «Abiertas» y separada con `---` como las demás, la sección `## Cambios pausados` con la tabla `| Cambio | Rama | Motivo | Espera a |` y la fila de la feature 003: `003-hecate-identities` (sin enlace, porque la carpeta solo existe en su rama), `feature/003-hecate-identities`, «Hécate pasa a Rust (ADR 0005) y la feature se planifica sobre la Hécate migrada», con el ADR enlazado a `docs/adr/0005-rust-para-orthros-y-hecate.md`, y «Que se fusione la migración de Hécate» (FR-006, FR-012, research R4)
- [X] T027 [US3] En la introducción de `DECISIONS.md` (línea 3), añadir una frase: los cambios pausados registran los cambios sin fusionar que el propietario aparcó, para que nadie reutilice su número de `specs/` (research R4)
- [X] T028 [US3] Verificar en `DECISIONS.md`, sin cambiarlo salvo que haya una contradicción: los ADRs 0005 a 0008 entre las decisiones tomadas, con enlace; ninguna cuestión abierta sobre el lenguaje del NSM y del control plane; abierta la de las APIs internas en Python y el certificado del cliente; el riesgo de Shamir entre los aceptados; el riesgo de memoria tal como está; y que ninguna cuestión abierta sobre Orthros o Hécate contradice el ADR 0005 (FR-012, FR-013, research R12)
- [X] T029 [US3] Ejecutar V7 de `specs/004-hecate-orthros-rust/quickstart.md`: la sección con la fila de la feature 003

**Checkpoint**: DECISIONS es el índice al día de lo decidido, lo abierto, los riesgos y lo pausado.

---

## Phase 6: User Story 4 - La licencia permite la pila elegida (Priority: P4)

**Goal**: el permiso adicional está junto a la licencia y enlazado desde el README, la constitución y su ADR.

**Independent Test**: `LICENSE` es igual que en `main`; `LICENSE-EXCEPTION` existe; el README enlaza el permiso y el
ADR 0008; la regla de licencia de la constitución lo menciona.

Este trabajo ya está escrito; la historia solo lo verifica.

- [X] T030 [P] [US4] Verificar `LICENSE-EXCEPTION`: es el permiso adicional de la sección 7 de la AGPLv3 para enlazar o combinar Cerbero con OpenSSL y con sus derivadas (BoringSSL, AWS-LC, ring), en inglés y con el titular de los derechos; y que `git diff --stat main -- LICENSE` no da salida (FR-014, ADR 0008)
- [X] T031 [P] [US4] Verificar que el apartado «Licencia» de `README.md` enlaza `LICENSE-EXCEPTION` y el ADR 0008, y que la restricción «Licencia» de `.specify/memory/constitution.md` dice que la licencia no condiciona la elección de herramientas y menciona el permiso con su ADR (FR-004, FR-014)

**Checkpoint**: la licencia permite la pila elegida y se encuentra desde el README.

---

## Phase 7: User Story 5 - Los docs vigentes dicen la verdad y no adelantan la migración (Priority: P5)

**Goal**: ningún doc presenta como auditada la implementación de Shamir, y ninguno describe código en Rust ni adelanta
la migración.

**Independent Test**: V3 y V4 pasan.

### Tests for User Story 5 ⚠️

- [X] T032 [US5] Ejecutar V3 y V4 de `specs/004-hecate-orthros-rust/quickstart.md` y anotar las siete afirmaciones que encuentra V4 (README 181, ARCHITECTURE 196, DECISIONS 93 y 108, doc de Hécate 18, 23 y 355) y que su segunda orden no da salida

### Implementation for User Story 5

En cada afirmación se mantiene lo que es cierto (no se inventa criptografía; las primitivas vienen de librerías
auditadas) y se añade, en la misma línea, que Shamir es la excepción, «sin auditoría pública», con un enlace a su
riesgo aceptado en `DECISIONS.md#riesgos-aceptados`; dentro de DECISIONS basta con mencionar el riesgo (research R5).

- [X] T033 [P] [US5] En `docs/hecate.md`, con enlace al riesgo aceptado (`../DECISIONS.md#riesgos-aceptados`) en cada línea corregida: la 18 (tabla de lo que no hace), la 23 («Cómo se construye»: `cryptography` y `PyNaCl`, auditadas; `PyCryptodome`, para Shamir, sin auditoría pública) y la 355 (preguntas frecuentes); y añadir al resumen «Riesgos aceptados» del apartado 12 (línea 339) que ninguna implementación de Shamir tiene una auditoría pública (FR-015)
- [X] T034 [P] [US5] En `ARCHITECTURE.md`, línea 196 (Hécate, «Qué hace»): la excepción de Shamir, con enlace al riesgo aceptado en `DECISIONS.md#riesgos-aceptados` (FR-015)
- [X] T035 [P] [US5] En `README.md`, línea 181 (presentación de Hécate): la excepción de Shamir, sin auditoría pública, con enlace al riesgo aceptado en `DECISIONS.md#riesgos-aceptados` (FR-015)
- [X] T036 [P] [US5] En `DECISIONS.md`: la desviación de la línea 93 («Hécate se construye en Cerbero, con primitivas de librerías auditadas…») y el descarte de la 108 («Solo primitivas de librerías auditadas»), con la excepción de Shamir y una referencia a su riesgo aceptado en la misma página (FR-015)
- [X] T037 [US5] Verificar con `git diff main -- README.md orthros/README.md docs/hecate.md` que solo cambian las líneas de T014, T033 y T035 y el apartado «Licencia» del README: el árbol y el párrafo de proyectos de `README.md`, `orthros/README.md` y el apartado 7 de `docs/hecate.md` siguen como en `main` (FR-016, research R12)
- [X] T038 [US5] Ejecutar V3 y V4 de `specs/004-hecate-orthros-rust/quickstart.md`: ninguna salida en V3 ni en la primera orden de V4; en la segunda, el resumen de riesgos del apartado 12

**Checkpoint**: los docs vigentes no afirman nada falso ni describen lo que aún no existe.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T039 [P] Normalizar «change» → «cambio» en las notas de `specs/004-hecate-orthros-rust/checklists/requirements.md` (FR-018)
- [X] T040 Ejecutar V1, V2 y V9 de `specs/004-hecate-orthros-rust/quickstart.md`: ninguna salida en V1 ni en V2, y `OK` en V9
- [X] T041 Hacer la lectura de coherencia V11 de `specs/004-hecate-orthros-rust/quickstart.md` (SC-001, SC-002) y corregir cualquier contradicción en los documentos de este cambio
- [X] T042 Quitar el Sync Impact Report del principio de `.specify/memory/constitution.md` (FR-007) y ejecutar V10: `# Cerbero Constitution` en la primera línea y la versión 1.2.0
- [X] T043 Ejecutar otra vez V1 a V10 de `specs/004-hecate-orthros-rust/quickstart.md` y comprobar que pasan todas
- [X] T044 Proponer al propietario el mensaje del commit de squash (`chore:`, en inglés, con una línea por bloque en el cuerpo y sin `Co-Authored-By`) y la descripción de la PR, que recojan los ficheros de este cambio (`.specify/memory/constitution.md`, `README.md`, `ARCHITECTURE.md`, `DECISIONS.md`, `LICENSE-EXCEPTION`, `docs/hecate.md`, `docs/adr/` y `specs/004-hecate-orthros-rust/`); el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias.
- **Foundational (Phase 2)**: vacía.
- **Historias (Phases 3 a 7)**: dependen solo del Setup; se pueden hacer en cualquier orden, pero con un solo agente
  conviene seguir la prioridad (US1 → US5), porque comparten ficheros: `README.md` (T014, T031 y T035), `DECISIONS.md`
  (T026 a T028 y T036) y `.specify/memory/constitution.md` (T004 a T013 y T031).
- **Polish (Phase 8)**: después de todas las historias. T039 es independiente; T040 → T041 → T042 → T043 → T044, en
  ese orden: el Sync Impact Report se quita cuando ya no quedan cambios en la constitución.

### Within Each User Story

- La comprobación que abre cada historia (T003, T016, T025, T032) se ejecuta antes que sus tareas, y la que la cierra
  (T015, T024, T029, T038), después.
- En US1, T004 a T012 tocan el mismo fichero y van en orden; T013 los verifica; T014 es otro fichero.
- En US2, T017 a T019 tocan el ADR 0005 y van en orden; T020 a T023 son revisiones en paralelo.

### Parallel Opportunities

- US2: T020, T021, T022 y T023 (un ADR o el índice cada una).
- US4: T030 y T031 (solo lectura).
- US5: T033, T034, T035 y T036 (un documento cada una).
- Polish: T039 con cualquier otra.

---

## Parallel Example: User Story 5

```bash
Task: "T033: en docs/hecate.md, la excepción de Shamir con enlace al riesgo aceptado en las líneas 18, 23 y 355, y el riesgo en el apartado 12"
Task: "T034: en ARCHITECTURE.md, línea 196, la excepción de Shamir con enlace al riesgo aceptado"
Task: "T035: en README.md, línea 181, la excepción de Shamir con enlace al riesgo aceptado"
Task: "T036: en DECISIONS.md, líneas 93 y 108, la excepción de Shamir con referencia a su riesgo en la misma página"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Phase 1: Setup y línea base.
2. Phase 3: US1. La constitución es la que gobierna la migración; con ella sola se puede empezar el cambio 005.
3. **Revisión del propietario:** las preguntas del Independent Test de US1, incluida la lectura guiada de SC-005.

### Siguientes incrementos

1. US2 (ADRs), US3 (DECISIONS), US4 (licencia) y US5 (docs vigentes), cada una con su comprobación al abrir y al cerrar.
2. Phase 8: comprobaciones finales, lectura de coherencia, Sync Impact Report fuera y mensaje de commit.

El cambio se entrega entero en una sola PR con un commit de squash; los incrementos son puntos de revisión, no entregas
separadas.
