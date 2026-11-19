# Implementation Plan: Hermes documentado en un documento de alto nivel y documentos específicos

**Branch**: `docs/hermes-reference` | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/010-hermes-reference/spec.md`

## Summary

Hermes pasa a la estructura de documentación de Hécate. El cambio no toca código: reorganiza, amplía y contrasta los
documentos.

- **Alto nivel.** `docs/hermes.md` deja la plantilla de 14 apartados y pasa a contar por qué existe Hermes, qué hace y
  qué no, quién lo usa y cómo encaja todo. Introduce cada concepto y cada uso en un párrafo con su enlace (R2).
- **Documentos específicos.** Nueve en `docs/hermes/`: contratos, llamadas, comunicación, configuración, proveedores y
  seguridad (diseño), e integración, CLI y variables de entorno, y operación (uso). Cada apartado actual tiene su
  destino en un mapa, para que nada se pierda (R1, R3).
- **Lo que falta hoy:**
  - la guía de integración para quien llama a Hermes, sin código (R4);
  - la referencia de la CLI con el detalle de cada orden, que hoy está en los contratos de la 008 (R5);
  - los procedimientos de operación, con las órdenes de `hermes` y de `hecate` y el compose de pruebas (R6);
  - el documento de seguridad con lo que se exige al proxy (R7) y el README de desarrollo (R8).
- **Estado.** DECISIONS ya recoge lo que falta de Hermes; sus tres enlaces a apartados de `docs/hermes.md` pasan a los
  documentos nuevos, y el README sigue diciendo lo que Hermes ya hace (R9). La constitución no cambia (R10).

Cada dato de los documentos se contrasta con el código, los OpenAPI y la CLI real. Las comprobaciones de
[quickstart.md](quickstart.md) que miran los documentos nuevos fallan sobre `main`, y todas pasan al terminar; un
recorrido con el compose de pruebas sigue los procedimientos tal como están escritos: el happy path con el bot de
Telegram de pruebas del propietario y el resto con el proveedor fake (R11). Las decisiones del propietario están en
[research R0](research.md#r0-decisiones-del-propietario).

## Technical Context

**Language/Version**: Markdown con diagramas Mermaid; documentación en español con los términos técnicos consolidados en
inglés (constitución, «Idioma»)

**Primary Dependencies**: Spec Kit 1.3.1; sin dependencias nuevas

**Storage**: N/A (documentos del repositorio)

**Testing**:

- las comprobaciones de [quickstart.md](quickstart.md): las órdenes frente a la ayuda de la CLI, las variables frente a
  `settings.py`, las operaciones frente al OpenAPI, los enlaces y las anclas, la ausencia de código, los enlaces de
  DECISIONS, el token del bot de pruebas y las palabras de tiempo;
- el recorrido de los procedimientos con el compose de `hermes/deploy/` (R11).

**Target Platform**: el render de Markdown de GitHub, con Mermaid; quien lee: el propietario, quien desarrolla un
componente que llama a Hermes, quien desarrolla una librería y el agente de desarrollo

**Project Type**: documentación de un componente

**Performance Goals**: N/A. El documento de alto nivel se lee en menos de 10 minutos (SC-001)

**Constraints**:

- ni código, ni configuración, ni contratos, ni manifiestos (FR-001);
- los documentos de diseño no dicen nada del estado, y las referencias de uso describen solo lo que existe (FR-013);
- los docs de otros componentes mantienen su plantilla (FR-018);
- las specs fusionadas no se tocan (FR-019);
- ningún apartado ni dato del documento actual se pierde (FR-004);
- ningún documento lleva el token del bot de pruebas ni otro secreto (constitución V).

**Scale/Scope**:

- un documento de alto nivel, nueve específicos y `hermes/README.md`;
- tres enlaces de DECISIONS y, si hace falta, los de ARCHITECTURE;
- 6 órdenes finales de la CLI, 11 variables de entorno, 3 operaciones de la API y 10 procedimientos.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | No toca código ni la frontera de ningún componente. La guía de integración describe el contrato de Hermes y enlaza a la de Hécate para la identidad |
| II. Contrato primero | El contrato sigue siendo el OpenAPI de `shared/contracts`; la guía remite a él y no lo duplica (FR-006) |
| III. Hexagonal con intensidad justificada | No aplica: no hay código. El README de desarrollo describe las capas reales |
| IV. Pruebas primero | No hay código que probar. Su equivalente son las comprobaciones del quickstart, que se escriben antes que los documentos y fallan sobre `main`, más el recorrido (R11) |
| V. Seguridad por diseño | Ningún secreto ni valor sensible en los documentos: las variables, con sus valores por defecto, ninguno secreto. El token del bot de pruebas vive en un fichero que git ignora, se pasa por la entrada estándar y una comprobación busca que no llegue a ningún documento |
| VI. Degradación elegante | El documento de alto nivel mantiene el comportamiento ante fallos (FR-002), y la operación, sus procedimientos |
| VII. Referencia | Las desviaciones de Hermes frente a SecOps y las del acceso por app no cambian; el alto nivel las resume y enlaza a DECISIONS |
| Lenguajes, herramientas y licencia | Sin cambios |
| Tecnología nueva vía ADR | No hay tecnología nueva; las comprobaciones usan las herramientas del repositorio |
| Specs sin tecnología | La spec nombra ficheros, la CLI y el OpenAPI porque son el contenido de la documentación (Assumptions) |
| Idioma y nombres | Docs en español; los nombres de los ficheros, en español y sin tildes (R1); los nombres de la mitología, solo para componentes |
| Fuentes de verdad y consolidación | Los documentos de detalle en `docs/hermes/` quedan dentro de la regla de la 1.2.1 (R10) |
| Docs sin futuro | El estado vive solo en DECISIONS y en el README; ningún documento tiene roadmap (FR-013, FR-015) |
| Orden y fases | Ciclo completo: specify, clarify, plan, tasks, analyze, implement y converge |
| Ramas y commits | Rama `docs/hermes-reference`; squash `docs:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Gobierno | Sin enmienda de la constitución (R10) |

**Resultado:** pasa, sin excepciones.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Sin código:** el diseño no añade código, configuración ni dependencias. Los scripts de comprobación y del recorrido
  viven fuera del repositorio (R11).
- **Secretos:** el bot de pruebas se usa solo en el happy path, con su token en `hermes/deploy/.secrets/`, que git ya
  ignora (R0).
- **Datos:** la diferencia ya conocida con el código (la prueba de conexión tras rotar un token, que no tiene orden en
  la CLI) se resuelve describiendo lo que existe (R12).
- **Contrato:** la guía de integración no añade nada que no esté en él.

## Project Structure

### Documentation (this feature)

```text
specs/010-hermes-reference/
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
formatos que otros leen (el marco de un documento específico, el de un procedimiento y los enlaces de DECISIONS) están
en [data-model.md](data-model.md).

### Documentos del repositorio

```text
docs/
├── hermes.md                       # Alto nivel (R2)
└── hermes/
    ├── contratos.md                # Diseño: taxonomía, capas, sobre, contrato de una función y chat.send_message
    ├── llamadas.md                 # Diseño: síncrona y task, idempotencia, reintentos, resultado desconocido y timeouts
    ├── comunicacion.md             # Diseño: contactos, plantillas, mensaje neutral, verificación y chat.ask
    ├── configuracion.md            # Diseño: el fichero, el plan, la aplicación, la exportación y las instancias
    ├── proveedores.md              # Diseño: librerías, manifiesto, conformidad, fake y Telegram
    ├── seguridad.md                # Amenazas, mecanismos, proxy, datos y riesgos (R7)
    ├── integracion.md              # Uso: el lado de quien llama (R4)
    ├── cli.md                      # Uso: CLI y variables de entorno (R5)
    └── operacion.md                # Uso: puesta en marcha, procedimientos y compose de pruebas (R6)
hermes/README.md                    # Desarrollo (R8)
DECISIONS.md                        # Los tres enlaces a apartados de docs/hermes.md (R9)
ARCHITECTURE.md                     # Solo los enlaces que remitan a un detalle de Hermes (R13)
```

No cambian: el código, los manifiestos y el despliegue de pruebas de `hermes/`, los OpenAPI de `shared/contracts`, el
README salvo que falte algo de lo construido, GLOSSARY y los docs de los demás componentes salvo un enlace que haga
falta, los ADR, la constitución y las specs fusionadas.

**Structure Decision**: no hay código; el cambio vive en documentos. El diseño de Hermes pasa de un solo fichero a un
documento de alto nivel con sus documentos específicos en `docs/hermes/`, como Hécate, dentro de la regla de la
constitución 1.2.1. El desarrollo queda en el README del propio componente, junto a su código.

## Complexity Tracking

Sin excepciones: el Constitution Check pasa entero.
