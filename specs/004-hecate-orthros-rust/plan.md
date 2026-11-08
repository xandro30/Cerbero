# Implementation Plan: Hécate y Orthros en Rust: la decisión documentada

**Branch**: `chore/hecate-orthros-rust` | **Date**: 2026-11-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/004-hecate-orthros-rust/spec.md`

## Summary

Este cambio deja documentada, completa y coherente la decisión de pasar Hécate y Orthros a Rust, sin tocar código ni
configuración. Parte de lo que ya existe sin commitear (la constitución 1.2.0, los ADRs 0005 a 0008, DECISIONS, el
README y LICENSE-EXCEPTION) y lo completa en tres frentes:

- **Constitución 1.2.0**, con `/speckit-constitution`: las reglas de flujo que pide la spec (todo cambio sigue Spec Kit
  salvo dos excepciones, sin que un acuerdo en la conversación sustituya a una fase; numeración global de `specs/`;
  rama `refactor/`; pausa de cambios registrada en DECISIONS; el término «cambio»; ADRs y specs fusionados que no se
  editan) y la corrección de su entrada en el historial. Todo entra en la 1.2.0, aún sin publicar (research R1, R3, R7).
- **Decisiones:** el ADR 0005 lista lo que se actualiza al consolidar la migración y cómo complementa al 0001; DECISIONS
  gana la sección de cambios pausados con la feature 003 (R4, R6).
- **Docs vigentes:** el README, ARCHITECTURE, DECISIONS y el doc de Hécate dejan de presentar como auditada la
  implementación de Shamir, y la constitución y el README llaman «cambio» a la unidad de trabajo. Lo que describe la
  implementación actual se queda como está (R5, R8, R12).

Las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas: fallan antes de escribir y pasan al terminar
(R9). Durante el plan, el propietario aprobó tres ajustes de la spec (R0).

## Technical Context

**Language/Version**: Markdown; documentación en español con los términos técnicos consolidados en inglés
(constitución, «Idioma»)

**Primary Dependencies**: Spec Kit 1.3.1 (flujo y plantillas); la enmienda se escribe con `/speckit-constitution`, como
exige el gobierno de la constitución

**Storage**: N/A (documentos del repositorio)

**Testing**: las comprobaciones de [quickstart.md](quickstart.md): búsquedas de texto, comparación con `main` y
verificación de enlaces, antes y después de escribir

**Target Platform**: el render de Markdown de GitHub y el agente de desarrollo, que lee la constitución, los ADRs y
DECISIONS al planificar

**Project Type**: documentación y gobierno del monorepo

**Performance Goals**: N/A

**Constraints**: ni código, ni configuración de compilación, ni manifiestos (FR-001); de los ADRs ya fusionados solo
cambia el estado del 0002, y las specs 001 y 002 no se tocan (FR-019); los textos que describen la implementación
actual siguen como están hasta la migración (FR-013, FR-016); ningún texto dice que la migración se hace en esta rama
(FR-008)

**Scale/Scope**: once documentos que cambian (la constitución, el README, ARCHITECTURE, DECISIONS, el doc de Hécate y
el ADR 0005) o se revisan (los ADRs 0006 a 0008, el índice de ADRs y LICENSE-EXCEPTION)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | No toca código ni la frontera de ningún componente |
| II. Contrato primero | Ningún cambio cruza componentes; los contratos siguen en `shared/contracts` |
| III. Hexagonal con intensidad justificada | No aplica: no hay código |
| IV. Pruebas primero | No hay código que probar. Su equivalente: las comprobaciones de quickstart se escriben antes que los docs, fallan sobre el estado actual y tienen que pasar al terminar (R9) |
| V. Seguridad por diseño | Ningún secreto, contacto ni valor de variable; LICENSE-EXCEPTION solo nombra al titular de los derechos |
| VI. Degradación elegante | No aplica: no hay comportamiento en ejecución |
| VII. Referencia | Ninguna desviación cambia; los riesgos aceptados ganan el de Shamir |
| Lenguajes, Herramientas y Licencia | Este cambio las documenta; ya están en la 1.2.0 y en los ADRs 0005 a 0008 |
| Tecnología nueva vía ADR | Cada decisión tiene su ADR (0005 a 0008); no se añade tecnología |
| Specs sin tecnología | La spec nombra Rust, OpenSSL y los tipos de rama porque son el contenido de las decisiones que documenta (notas de la checklist) |
| Idioma y nombres | Docs en español; ramas y tipos de commit en inglés (`refactor/`); «cambio» es un término de proceso, no del dominio |
| Consolidación | Solo se escriben decisiones (constitución, ADRs, DECISIONS) y correcciones de afirmaciones falsas; ningún doc de componente describe la migración como hecha (FR-016) |
| Orden y fases | Ciclo completo: specify, clarify, plan, tasks, analyze, implement y converge |
| Ramas y commits | **Excepción:** la regla vigente llama `feature/<NNN>-<nombre>` a la rama de un ciclo de Spec Kit, y este va en `chore/hecate-orthros-rust`. Justificada en Complexity Tracking; este mismo cambio permite un ciclo en cualquier tipo de rama (FR-005). Squash `chore:` (R11) |
| Una cosa a la vez | **Excepción:** la feature 003 sigue sin fusionar. Justificada en Complexity Tracking; este mismo cambio añade la regla de pausa (FR-006) y la registra |
| Gobierno | Enmienda con `/speckit-constitution`, justificada por bloque (qué, por qué, alternativas y afecta) y con semver (FR-007, R10) |

**Resultado:** pasa, con las dos excepciones justificadas.

**Revisión tras el diseño (fase 1):** sigue pasando. El diseño no añade código, configuración ni dependencias: las
reglas nuevas van en las reglas de flujo que ya existen (R3) y la sección de cambios pausados va en DECISIONS (R4). Las
dos excepciones siguen siendo «Una cosa a la vez» y «Ramas», que este cambio resuelve con sus propias reglas: la pausa y
el ciclo en cualquier tipo de rama. Los ajustes de la spec aprobados en el plan (R0) no abren ninguna excepción nueva.

## Project Structure

### Documentation (this feature)

```text
specs/004-hecate-orthros-rust/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: comprobaciones de validación
├── checklists/
│   └── requirements.md  # Checklist de calidad de la spec
└── tasks.md             # /speckit-tasks (no lo crea este comando)
```

Sin `contracts/`: el cambio no expone interfaces. Los formatos de documento que otros leen (la sección de cambios
pausados y la tabla de tipos de rama) están en [data-model.md](data-model.md).

### Documentos del repositorio

```text
.specify/memory/constitution.md   # 1.2.0: reglas de flujo (R3), historial (R7), «cambio» (R8)
README.md                         # «Cómo se desarrolla» y árbol de specs/ (R8); Shamir (R5); licencia (hecha)
ARCHITECTURE.md                   # Hécate: excepción de Shamir (R5)
DECISIONS.md                      # cambios pausados (R4); Shamir en la desviación y en lo descartado (R5)
LICENSE-EXCEPTION                 # revisión (hecho)
docs/
├── hecate.md                     # Shamir en los apartados 1, 12 y 13 (R5)
└── adr/
    ├── README.md                 # revisión del índice (hecho)
    ├── 0002-lenguajes-por-proyecto.md        # solo el estado (hecho)
    ├── 0005-rust-para-orthros-y-hecate.md    # docs de la migración, ADR 0001, riesgo de memoria (R6)
    ├── 0006-pruebas-rust-cargo-proptest.md   # revisión
    ├── 0007-pila-hecate-rust.md              # revisión
    └── 0008-excepcion-openssl-agpl.md        # revisión
```

No cambian: el código, `pyproject.toml`, `uv.lock`, `orthros/` y `hecate/` (con sus README), los ADRs 0001, 0003 y
0004, y las specs 001 y 002 (R12).

**Structure Decision**: no hay código; el cambio vive en documentos. Las reglas nuevas van en la sección «Flujo de
desarrollo y documentación» de la constitución, junto a las que ya regulan cada tema (R3), y los cambios pausados, en
una sección propia al final de DECISIONS (R4).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Se empieza el cambio 004 con la feature 003 sin fusionar («Una cosa a la vez») | El propietario pausa la 003 hasta migrar Hécate, porque su plan se rehace en Rust. Este cambio añade la regla de pausa (FR-006) y registra la 003 en DECISIONS | Terminar antes la 003 la construiría sobre la Hécate en Python que se va a sustituir |
| Ciclo de Spec Kit en una rama `chore/` sin número («Ramas»: la regla vigente pide `feature/<NNN>-<nombre>`) | El propietario eligió `chore/hecate-orthros-rust`: es un cambio de documentación y gobierno, no una funcionalidad. Este cambio añade la regla que permite un ciclo en cualquier tipo de rama (FR-005) | `feature/004-hecate-orthros-rust` presentaría como funcionalidad un cambio que no la añade (FR-018) |
