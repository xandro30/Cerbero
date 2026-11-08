# Research: Hécate y Orthros en Rust: la decisión documentada

Fase 0 del plan. No quedan incógnitas técnicas: las decisiones de lenguaje, herramientas y licencia las tomó el
propietario y están en los ADRs 0005 a 0008. Aquí se resuelve qué falta en los documentos, dónde va cada regla nueva y
cómo se comprueba. La base es el inventario de los docs del 2026-11-08 (R2).

## R0. Ajustes de la spec aprobados en el plan

- **Decision:** el propietario aprobó tres ajustes de la spec al revisar los docs: FR-011 deja de listar ARCHITECTURE;
  FR-015 cubre todos los docs que dicen que Hécate usa «primitivas de librerías auditadas»; y FR-005 añade que ninguna
  fase se sustituye por un acuerdo en la conversación. La spec ya los recoge en FR-005, FR-011, FR-015, el escenario 7
  de US1 y los escenarios 1 y 2 de US5.
- **Rationale:** ARCHITECTURE no nombra lenguajes ni la estructura del repositorio, así que la migración no tiene nada
  que cambiar en él. En cambio, ARCHITECTURE, el README y DECISIONS repiten la afirmación general de librerías
  auditadas, que choca con el riesgo aceptado de Shamir. La regla del chat es la causa del port descartado, y hasta
  ahora solo se deducía de «todo cambio sigue el orden».
- **Alternatives considered:** dejar FR-011 y FR-015 como estaban (una tarea sin nada que hacer y cuatro afirmaciones
  falsas fuera del alcance); dar por evidente la regla del chat (ya falló una vez).

## R1. Versión de la enmienda

- **Decision:** todo entra en la 1.2.0, que aún no está commiteada. Su entrada del historial gana un tercer bloque,
  «Flujo», con su propio origen, y la versión sigue siendo MINOR.
- **Rationale:** una enmienda por PR. La 1.2.0 no se ha publicado, así que no hay un estado intermedio que versionar,
  y añadir reglas o ampliar el alcance de una sección es MINOR según el gobierno de la constitución.
- **Alternatives considered:** una 1.3.0 para las reglas de proceso (separa los orígenes, pero son dos versiones en una
  sola PR y la 1.2.0 nunca existiría sola en `main`).

## R2. Inventario: qué está hecho y qué falta

| Requisito | Estado el 2026-11-08 | Falta |
|---|---|---|
| FR-001, SC-003 | El código y la configuración son iguales que en `main` | Comprobarlo al cerrar (quickstart V1) |
| FR-002, FR-003, FR-004 | En la 1.2.0: Repositorio, Lenguajes, Herramientas y Licencia | Nada |
| FR-005 | No está | Reglas de Orden, Ramas, Commits y numeración (R3) |
| FR-006 | No está, y «Una cosa a la vez» lo impide | Regla de pausa (R3) y sección en DECISIONS (R4) |
| FR-007 | La entrada 1.2.0 tiene dos bloques y el Sync Impact Report sigue arriba | Bloque «Flujo», correcciones (R7) y quitar el informe antes del commit (R10) |
| FR-008 | El Sync Impact Report y la entrada 1.2.0 citan `chore/hecate-rust` | Reescribirlos sin rama (R7) |
| FR-009, FR-010 | ADRs 0005 a 0008 con las cuatro partes; el 0002, sustituido; el índice, al día; la regla de versiones, en el 0007 | Nada: solo la lectura de coherencia |
| FR-011 | El ADR 0005 no lista los docs de la migración ni cita el 0001 | Añadirlo (R6) |
| FR-012 | Decisiones tomadas, cuestión del NSM cerrada, la de las APIs en Python abierta y el riesgo de Shamir: hechos | La sección de cambios pausados con la 003 (R4) |
| FR-013 | El riesgo de memoria describe Python; el ADR 0005 dice que «se reescribe» sin decir cuándo | Precisar que se reescribe al consolidar la migración (R6) |
| FR-014 | LICENSE-EXCEPTION y el README, con su enlace y el del ADR 0008: hechos | Nada |
| FR-015 | Siete afirmaciones de librerías auditadas sin la excepción de Shamir | Corregirlas (R5) |
| FR-016 | Ningún doc describe código en Rust | Comprobarlo (quickstart V3) |
| FR-017, SC-006 | Sin verificar | Verificar los enlaces al cerrar (quickstart V9) |
| FR-018 | La constitución (cinco reglas) y el README (dos sitios) llaman «feature» a la unidad de Spec Kit | Normalizar a «cambio» (R8) |
| FR-019 | No está | Regla en «Fuentes de verdad» (R3) |

## R3. Dónde va cada regla nueva de la constitución

- **Decision:** todas van en «Flujo de desarrollo y documentación», en la regla que ya trata cada tema:
  - **Fuentes de verdad:** `specs/<NNN>-<nombre>/` describe un cambio, y `<NNN>` es el número siguiente al mayor de
    los que hay en `main` y en los cambios pausados de DECISIONS, sea cual sea la rama (FR-005). Un ADR ya fusionado
    solo cambia su estado y su fila del índice, y el ADR que lo complementa o lo sustituye lo dice en su propio texto;
    la spec de un cambio fusionado no se modifica (FR-019).
  - **Orden:** todo cambio sigue el orden, sea cual sea su tipo de rama: también un chore, una migración o un cambio de
    documentación. Ninguna fase se sustituye por un acuerdo en la conversación: un plan aprobado en el chat no es un
    plan.md. Solo dos excepciones van en su rama y con PR, sin ciclo ni carpeta: las correcciones que no cambian el
    significado de ningún documento ni tocan código ni configuración, y las subidas de versión que no obligan a cambiar
    código; si lo hacen, siguen el ciclo (FR-005).
  - **Ramas:** se añade `refactor/`, para los cambios de código que conservan el comportamiento, como una migración de
    lenguaje. Un ciclo puede ir en una rama de cualquier tipo; solo `feature/` lleva el número de su carpeta
    (`feature/<NNN>-<nombre>`), como hasta ahora (FR-005).
  - **Una cosa a la vez:** cada cambio va en su propia rama y no se empieza otro hasta fusionarlo, salvo que el
    propietario lo pause. El cambio pausado se registra en DECISIONS (R4) y su fila sale de ahí con la PR que lo
    fusiona (FR-006).
  - **Commits:** `refactor/` → `refactor:` (FR-005).
  - **Terminología:** «cambio» donde hoy dice «feature» para la unidad de Spec Kit (R8, FR-018).
- **Rationale:** cada regla queda junto a las que ya regulan lo mismo, así que un agente que lee «Ramas» u «Orden»
  encuentra la regla completa sin buscar en otra sección. No hace falta ninguna sección nueva.
- **Alternatives considered:** una sección nueva «Cambios» con todas las reglas (duplicaría Orden, Ramas y Commits);
  explicar la numeración solo en el README (no es donde el agente busca las reglas al planificar).

## R4. La sección de cambios pausados en DECISIONS

- **Decision:** una sección `## Cambios pausados` al final de DECISIONS, tras las abiertas, con la tabla
  `| Cambio | Rama | Motivo | Espera a |`. Su primera fila es la feature 003: `003-hecate-identities`,
  `feature/003-hecate-identities`, que Hécate pasa a Rust (ADR 0005) y la feature se planifica sobre la Hécate migrada,
  y que espera a que se fusione la migración de Hécate. La introducción de DECISIONS menciona la sección. La carpeta va
  sin enlace, porque solo existe en su rama (FR-017).
- **Rationale:** DECISIONS vive en `main` y es el índice de lo abierto; la regla de numeración la lee para no reutilizar
  un número (FR-005).
- **Alternatives considered:** una fila en «Abiertas» (mezcla las decisiones de diseño con el estado del trabajo); la
  spec del cambio pausado, en su rama (no se ve desde `main`).

## R5. La excepción de Shamir (FR-015)

- **Decision:** en las siete afirmaciones se mantiene lo que es cierto (no se inventa criptografía; las primitivas
  vienen de librerías auditadas) y se añade, en la misma línea, que Shamir es la excepción, sin auditoría pública, con
  su riesgo aceptado. Son el doc de Hécate (líneas 18 y 23 del apartado 1 y 355 del 13), ARCHITECTURE (196), el README
  (181) y DECISIONS (la desviación de la línea 93 y el descarte de la 108). El resumen de riesgos del doc de Hécate
  (apartado 12, línea 339) gana el de Shamir.
- **Rationale:** AES-GCM, HKDF, HMAC y Argon2id sí vienen de librerías auditadas; solo Shamir carece de auditoría
  pública, en Python y en Rust (ADR 0007, DECISIONS). Corregir la afirmación en lugar de borrarla conserva una
  propiedad cierta y relevante.
- **Alternatives considered:** quitar «auditadas» en todas partes (pierde una propiedad cierta); corregir solo la línea
  23 del doc de Hécate (deja las demás en contradicción con el riesgo).

## R6. Lo que se actualiza al consolidar la migración (FR-011, FR-013)

- **Decision:** las consecuencias del ADR 0005 ganan una viñeta con los documentos que se actualizan al consolidar la
  migración: el README (el árbol del repositorio y el párrafo de los proyectos en Python, con su referencia al ADR
  0002), el README de Orthros (la tabla de lenguajes), el doc de Hécate (las librerías del `CryptoProvider` en los
  apartados 1 y 7, y el riesgo de memoria del apartado 12) y el riesgo de memoria de DECISIONS. La viñeta
  «Repositorio» añade que complementa al ADR 0001, que sigue vigente para los proyectos en Python y no se edita. La
  viñeta «Riesgo de memoria» dice que el riesgo se reescribe al consolidar la migración.
- **Rationale:** el primer caso límite de la spec y FR-013: lo que describe la implementación actual se queda como está
  y figura en las consecuencias del ADR que lo cambia. Así la migración sabe qué actualizar sin adelantarlo aquí.
- **Alternatives considered:** actualizar ya esos documentos (describirían código en Rust que no existe, FR-016).

## R7. La entrada 1.2.0 del historial

- **Decision:** tres bloques, cada uno con qué, por qué, alternativas y afecta (FR-007):
  - **Lenguajes y Repositorio** (ya existe): «Hécate, que se porta a Rust en un cambio propio, con paridad de
    comportamiento» en lugar de la rama. «Afecta» lista lo que de verdad cambia: el ADR 0002, sustituido; DECISIONS,
    donde se cierra el lenguaje del NSM y del control plane; la feature 003, pausada y replanificada en Rust; y, al
    consolidar la migración, el README, el README de Orthros, el doc de Hécate, el riesgo de memoria de DECISIONS y el
    workspace de uv. Ni ARCHITECTURE ni el ADR 0001 (R0, FR-019).
  - **Herramientas y Licencia** (ya existe): «LICENSE-EXCEPTION y el README» en lugar de «LICENSE y README», porque
    LICENSE no cambia (ADR 0008); la política de licencias de cargo-deny se crea con la migración.
  - **Flujo** (nuevo). Qué: las reglas de R3. Por qué: un plan aprobado en la conversación se implementó sin spec ni
    tareas y el propietario lo descartó; la constitución no decía cómo entra en Spec Kit un cambio que no es una
    feature, ni permitía empezar otro con la 003 sin fusionar; «feature» nombraba a la vez un tipo de rama y cualquier
    unidad de trabajo; y nada impedía reescribir un ADR aceptado. Alternativas: un ciclo reducido para los chores; todo
    cambio sin excepciones; «feature» o «change» como término; ADRs vivos que se actualizan. Afecta: la sección de
    flujo, el README, DECISIONS y la feature 003. Origen: el port descartado y el `/speckit-clarify` del cambio 004.
- **Rationale:** FR-007 y FR-008. Corregir los dos bloques existentes en lugar de reescribirlos conserva la
  justificación que el propietario ya revisó.
- **Alternatives considered:** reescribir la entrada desde cero (se pierde lo revisado).

## R8. Terminología (FR-018)

- **Decision:** «cambio» en las cinco reglas de la constitución que hoy dicen «feature» para la unidad de Spec Kit
  («Nueva tecnología», «Fuentes de verdad», «Consolidación», «Ramas» y «Una cosa a la vez») y en dos sitios del README
  (el árbol de `specs/` y «Cómo se desarrolla»). «Feature» queda para el tipo de rama `feature/` y para los cambios que
  añaden funcionalidad, como las features 001 a 003. Las entradas 1.0.0 y 1.1.0 del historial conservan su redacción:
  son el registro de su fecha, con el mismo criterio que FR-019.
- **Rationale:** es lo que decidió el clarify, y la constitución ya decía que una carpeta de `specs/` «describe un
  cambio».
- **Alternatives considered:** reescribir también el historial antiguo (falsea lo que se aprobó entonces).

## R9. Cómo se valida un cambio de documentación (principio IV)

- **Decision:** las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas. Se ejecutan antes de escribir,
  cuando las que miden algo pendiente fallan, y otra vez al terminar, cuando tienen que pasar todas. Cubren lo que se
  puede buscar o comparar (ramas citadas, términos, afirmaciones de Shamir, enlaces, diff con `main`); lo demás (SC-001
  y SC-005) es una lectura guiada con preguntas concretas.
- **Rationale:** el principio IV pide una prueba que falle antes de implementar. En documentos, su equivalente es una
  comprobación repetible, que además da a `/speckit-converge` evidencia en lugar de memoria.
- **Alternatives considered:** solo la revisión manual (no se puede repetir ni deja evidencia).

## R10. Herramienta de la enmienda y cierre

- **Decision:** la constitución se enmienda en la implementación con `/speckit-constitution`, como exige su gobierno,
  con el contenido de R3, R7 y R8. El Sync Impact Report temporal que genera se quita en la última tarea, antes de
  proponer al propietario el mensaje de commit (FR-007). El propietario hace el commit, el push y la PR.
- **Rationale:** el gobierno de la constitución y las reglas de trabajo del propietario.
- **Alternatives considered:** editar la constitución a mano (se salta el procedimiento de enmienda).

## R11. Rama y commit de este cambio

- **Decision:** `chore/hecate-orthros-rust`, como eligió el propietario, con commit de squash `chore:`. Con la regla
  nueva (FR-005, R3) cumple la de ramas: `<tipo>/<nombre>`, porque solo `feature/` lleva número. Con la vigente, que
  llama `feature/<NNN>-<nombre>` a la rama de un ciclo de Spec Kit, es una excepción, justificada en el Complexity
  Tracking del plan. El cambio es el 004 porque la feature 003 existía en su rama; con la sección de R4, la migración
  de Hécate será el 005.
- **Rationale:** FR-005 y la decisión del propietario.
- **Alternatives considered:** `docs/` (las PR #1 y #2 usaron `docs:` para cambios de la constitución); el propietario
  eligió `chore/` para este cambio.

## R12. Lo que no cambia, y por qué

| Documento | Por qué se queda |
|---|---|
| ADRs 0001, 0003 y 0004 (cuerpo) | Son el registro de su fecha (FR-019); el 0005 dice cómo complementa al 0001 |
| Specs 001 y 002 | Registros de cambios fusionados (FR-019) |
| Árbol y párrafo de proyectos del README, con su referencia al ADR 0002 | Describen la implementación actual; figuran en el ADR 0005 (R6) |
| README de Orthros | Describe la implementación actual; figura en el ADR 0005 |
| Doc de Hécate: librerías del `CryptoProvider` y organización interna | Describen la implementación actual; solo cambia la afirmación de auditoría (R5) |
| Riesgo de memoria de DECISIONS | Describe la implementación actual (FR-013) |
| Entradas 1.0.0 y 1.1.0 del historial | Registros de su fecha (R8) |
| Cuestiones abiertas de DECISIONS sobre Orthros y Hécate | Ninguna contradice el ADR 0005; se repasan en la lectura de coherencia |
