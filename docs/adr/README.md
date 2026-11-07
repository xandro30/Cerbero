# ADR — Decisiones de arquitectura

Una decisión significativa por fichero (`NNNN-titulo.md`). Las decisiones abiertas viven en [DECISIONS](../../DECISIONS.md); al tomarse, se escriben aquí y DECISIONS enlaza el ADR.

| ADR | Decisión | Estado |
|---|---|---|
| [0001](0001-monorepo-uv-workspaces.md) | Monorepo con proyectos independientes en un workspace de uv | Aceptada |
| [0002](0002-lenguajes-por-proyecto.md) | Rust para Eurytion; Python para el resto | Aceptada |
| [0003](0003-persistencia-hecate-sqlite.md) | Persistencia de Hécate en SQLite | Aceptada |
| [0004](0004-pruebas-pytest-hypothesis.md) | Pruebas en Python con pytest e hypothesis | Aceptada |

## Plantilla

```markdown
# NNNN — Título

Estado: propuesta | aceptada | sustituida por NNNN · Fecha: AAAA-MM-DD

## Contexto
Qué problema hay que resolver y qué lo condiciona.

## Opciones
Las alternativas consideradas, con sus pros y contras.

## Decisión
Qué se elige y por qué.

## Consecuencias
Qué implica, qué se gana y qué se acepta perder.
```
