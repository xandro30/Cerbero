# Implementation Plan: Hécate documentada antes de pausarla

**Branch**: `docs/hecate-reference` | **Date**: 2026-11-12 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/007-hecate-reference/spec.md`

## Summary

Hécate queda pausada con su documentación lista para usarla y retomarla. El cambio no toca código: reorganiza y amplía
los documentos.

- **Alto nivel.** `docs/hecate.md` deja la plantilla de 14 apartados y pasa a contar por qué existe Hécate, qué hace y
  qué no, quién la usa y cómo encaja todo. Introduce cada concepto y cada uso en un párrafo con su enlace (R2).
- **Documentos específicos.** Nueve en `docs/hecate/`: sellado, secretos, identidades, acceso, auditoría y seguridad
  (diseño), e integración, CLI y operación (uso). Cada apartado actual tiene su destino en un mapa, para que nada se
  pierda (R1, R3).
- **Lo que falta hoy.**
  - la guía de integración con el lado del componente, sin código (R4);
  - la referencia de la CLI con las seis variables de entorno, que no aparecen en ningún doc vigente (R5);
  - los procedimientos de operación, que el documento actual da por escritos (R6);
  - el documento de seguridad (R7) y el README de desarrollo (R8).
- **Estado.** Lo diseñado y sin construir pasa a «Abiertas» de DECISIONS, con el despliegue en el home lab, y el README
  dice lo que Hécate ya hace (R9).
- **Constitución 1.2.1.** Aclara que el diseño de un componente puede tener documentos de detalle en
  `docs/<componente>/` (R10).

Cada dato de los documentos de uso se contrasta con el código y el OpenAPI. Las comprobaciones de
[quickstart.md](quickstart.md) fallan sobre `main` y pasan al terminar, y un recorrido real con el binario release sigue
los procedimientos tal como están escritos (R11). Las decisiones del propietario están en
[research R0](research.md#r0-decisiones-del-propietario).

## Technical Context

**Language/Version**: Markdown con diagramas Mermaid; documentación en español con los términos técnicos consolidados
en inglés (constitución, «Idioma»)

**Primary Dependencies**: Spec Kit 1.3.1; la aclaración de la constitución se escribe con `/speckit-constitution`, como
exige su gobierno

**Storage**: N/A (documentos del repositorio)

**Testing**:
- las comprobaciones de [quickstart.md](quickstart.md): las órdenes de la CLI frente a su ayuda, las variables frente a
  `config.rs`, las operaciones frente al OpenAPI, los enlaces y las anclas, la ausencia de código y las líneas de estado;
- el recorrido de los procedimientos con el binario release (R11).

**Target Platform**: el render de Markdown de GitHub, con Mermaid; quien lee: el propietario, quien desarrolla otro
componente y el agente de desarrollo

**Project Type**: documentación de un componente y gobierno del monorepo

**Performance Goals**: N/A. El documento de alto nivel se lee en menos de 10 minutos (SC-001)

**Constraints**:
- ni código, ni configuración de compilación, ni manifiestos (FR-001);
- los documentos de diseño no dicen nada del estado, y las referencias de uso describen solo lo que existe (FR-013);
- los docs de otros componentes mantienen su plantilla (FR-016);
- las specs fusionadas no se tocan (FR-017);
- ningún dato del documento actual se pierde (FR-004).

**Scale/Scope**:
- un documento de alto nivel, nueve específicos y `hecate/README.md`;
- tres documentos que ganan líneas: DECISIONS, el README y la constitución;
- los enlaces de ARCHITECTURE que haga falta;
- 32 órdenes finales de la CLI, 6 variables de entorno, 8 operaciones de la API y 11 procedimientos.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | No toca código ni la frontera de ningún componente. La guía de integración describe el contrato, no una librería compartida |
| II. Contrato primero | El contrato sigue siendo el OpenAPI de `shared/contracts`; la guía remite a él y no lo duplica (FR-005) |
| III. Hexagonal con intensidad justificada | No aplica: no hay código. El README de desarrollo describe las capas reales |
| IV. Pruebas primero | No hay código que probar. Su equivalente son las comprobaciones del quickstart, que se escriben antes que los documentos, fallan sobre `main` y pasan al terminar, más el recorrido real (R11) |
| V. Seguridad por diseño | Ningún secreto ni valor de variable: la referencia nombra las variables y sus valores por defecto, ninguno secreto. El recorrido usa frases de prueba en un directorio temporal, fuera del repositorio |
| VI. Degradación elegante | El documento de alto nivel mantiene el comportamiento ante fallos (FR-002), y la operación, sus procedimientos |
| VII. Referencia | Las desviaciones frente a OpenBao / Vault, step-ca y SPIFFE no cambian; el documento de seguridad enlaza a ellas |
| Lenguajes, herramientas y licencia | Sin cambios |
| Tecnología nueva vía ADR | No hay tecnología nueva; las comprobaciones usan las herramientas del sistema |
| Specs sin tecnología | La spec nombra ficheros, la CLI y el OpenAPI porque son el contenido de la documentación (Assumptions) |
| Idioma y nombres | Docs en español; los nombres de los ficheros, en español y sin tildes (R1); los nombres de la mitología, solo para componentes |
| Fuentes de verdad y consolidación | Los documentos de detalle en `docs/hecate/` quedan dentro de la regla con la aclaración 1.2.1 (R10) |
| Docs sin futuro | El estado vive solo en DECISIONS y en el README; ningún documento tiene roadmap (FR-013) |
| Orden y fases | Ciclo completo: specify, clarify, plan, tasks, analyze, implement y converge |
| Ramas y commits | Rama `docs/hecate-reference` (una rama que no es `feature/` no lleva número); squash `docs:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Gobierno | La aclaración, con `/speckit-constitution`: PATCH a la 1.2.1, con qué, por qué, alternativas y a qué afecta (R10) |

**Resultado:** pasa, sin excepciones.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Sin código:** el diseño no añade código, configuración ni dependencias. Los scripts de comprobación y del recorrido
  viven fuera del repositorio (R11).
- **Constitución:** la única enmienda es la aclaración PATCH, que no cambia ninguna regla.
- **Datos:** las diferencias con el código que ya se conocen (los parámetros que el documento actual llama
  configurables, la retención de la auditoría y el árbol de módulos) se resuelven describiendo el código (R12).
- **Contrato:** la guía de integración no añade nada que no esté en él.

## Project Structure

### Documentation (this feature)

```text
specs/007-hecate-reference/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1: los documentos, sus enlaces y los formatos
├── quickstart.md        # Fase 1: comprobaciones y recorrido
├── checklists/
│   └── requirements.md  # Checklist de calidad de la spec
└── tasks.md             # /speckit-tasks (no lo crea este comando)
```

Sin `contracts/`: el cambio no expone interfaces. El contrato de la API ya es el OpenAPI de `shared/contracts`, y los
formatos que otros leen (el marco de un documento específico, las filas de DECISIONS y la entrada de la constitución)
están en [data-model.md](data-model.md).

### Documentos del repositorio

```text
docs/
├── hecate.md                       # Alto nivel (R2)
└── hecate/
    ├── sellado.md                  # Diseño: raíz, frase, partes, desbloqueo y sellado
    ├── secretos.md                 # Diseño: cifrado por sobre, versiones, huella, metadatos y rotación de las claves de ámbito
    ├── identidades.md              # Diseño: CA, alta, renovación, revocación y certificado propio
    ├── acceso.md                   # Diseño: Access Policy, resolución y permiso de estado
    ├── auditoria.md                # Diseño: registro de accesos y checkpoints
    ├── seguridad.md                # Amenazas, mecanismos y riesgos (R7)
    ├── integracion.md              # Uso: el lado del componente (R4)
    ├── cli.md                      # Uso: lanzar, requisitos, variables y órdenes (R5)
    └── operacion.md                # Uso: puesta en marcha y procedimientos (R6)
hecate/README.md                    # Desarrollo (R8)
DECISIONS.md                        # Siete filas nuevas en «Abiertas» (R9)
README.md                           # La línea de Hécate en «Estado del Proyecto» (R9)
ARCHITECTURE.md                     # Solo los enlaces que remiten a un detalle de Hécate (R13)
.specify/memory/constitution.md     # 1.2.1: dónde vive el diseño de un componente (R10)
```

No cambian: el código y los manifiestos de `hecate/` y de `shared/contracts`, el OpenAPI, GLOSSARY salvo un enlace que
haga falta, los docs de los demás componentes salvo un enlace que haga falta, los ADR y las specs fusionadas.

**Structure Decision**: no hay código; el cambio vive en documentos. El diseño de Hécate pasa de un solo fichero a un
documento de alto nivel con sus documentos específicos en `docs/hecate/`, que la aclaración 1.2.1 de la constitución
reconoce. El desarrollo queda en el README del propio componente, junto a su código.

## Complexity Tracking

Sin excepciones: el Constitution Check pasa entero.
