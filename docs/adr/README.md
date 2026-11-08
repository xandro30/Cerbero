# ADR — Decisiones de arquitectura

Una decisión significativa por fichero (`NNNN-titulo.md`). Las decisiones abiertas viven en [DECISIONS](../../DECISIONS.md); al tomarse, se escriben aquí y DECISIONS enlaza el ADR.

| ADR | Decisión | Estado |
|---|---|---|
| [0001](0001-monorepo-uv-workspaces.md) | Monorepo con proyectos independientes en un workspace de uv | Aceptada |
| [0002](0002-lenguajes-por-proyecto.md) | Rust para Eurytion; Python para el resto | Sustituida por 0005 |
| [0003](0003-persistencia-hecate-sqlite.md) | Persistencia de Hécate en SQLite | Aceptada |
| [0004](0004-pruebas-pytest-hypothesis.md) | Pruebas en Python con pytest e hypothesis | Aceptada |
| [0005](0005-rust-para-orthros-y-hecate.md) | Rust para Orthros y Hécate; Python para el resto | Aceptada |
| [0006](0006-pruebas-rust-cargo-proptest.md) | Pruebas en Rust con cargo test y proptest | Aceptada |
| [0007](0007-pila-hecate-rust.md) | Pila de Hécate en Rust | Aceptada |
| [0008](0008-excepcion-openssl-agpl.md) | Permiso adicional de enlace con OpenSSL en la AGPL | Aceptada |

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
