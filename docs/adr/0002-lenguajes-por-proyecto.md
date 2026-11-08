# 0002 — Rust para Eurytion; Python para el resto

Estado: sustituida por [0005](0005-rust-para-orthros-y-hecate.md) · Fecha: 2026-11-01

## Contexto

Los proyectos de Cerbero no tienen por qué usar el mismo lenguaje. Eurytion es distinto del resto: corre en todos los equipos de la casa, en varios sistemas operativos, y debe ocupar poco y distribuirse como un binario. El resto son servicios en el servidor de casa, con APIs (FastAPI en Hermes) y dominio rico.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Rust para Eurytion, Python para el resto** | Agente ligero y un binario por sistema operativo; los servicios aprovechan la experiencia en Python | Dos lenguajes y dos cadenas de herramientas |
| Todo en Python | Un solo lenguaje | Un agente más pesado en cada equipo y más difícil de distribuir |
| Todo en Rust | Un solo lenguaje, rendimiento | Más lento de desarrollar en los servicios con dominio rico |

## Decisión

**Eurytion en Rust; el control plane y el NSM de Orthros, Echidna, Estigia, Caronte, Hades, Hermes, Hécate y Ténaro en Python** (3.15, gestionado con uv). Además, Eurytion es la ocasión de practicar Rust en un proyecto real.

## Consecuencias

- Eurytion es un crate de Cargo en `orthros/eurytion`; el resto son miembros del workspace de uv (ver [0001](0001-monorepo-uv-workspaces.md)).
- Eurytion consume los mismos contratos que el resto (UDM, Policy): su formato debe poder usarse desde Rust y desde Python.
