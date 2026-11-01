# 0001 — Monorepo con proyectos independientes en un workspace de uv

Estado: aceptada · Fecha: 2026-11-01

## Contexto

Cerbero son varios proyectos que se despliegan por separado (Orthros, Echidna, Estigia, Caronte, Hades, Hermes, Hécate, Ténaro) y que solo comparten contratos (UDM, sobre común, API Contracts), nunca dominio. Hay que decidir si viven en un repositorio o en varios.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Monorepo con un workspace de uv** | Un sitio para la documentación y los contratos; cambios que cruzan proyectos en un solo commit; un único `uv.lock` | Hay que vigilar que nadie importe el dominio de otro proyecto |
| Un repositorio por proyecto | Independencia máxima | Contratos publicados y versionados entre repos desde el primer día; más mantenimiento para un proyecto personal |

## Decisión

**Monorepo con un workspace de uv.** La raíz es un workspace virtual (sin paquete propio) y cada proyecto Python es un miembro con su `pyproject.toml`, su código en `src/<módulo>` con el esqueleto hexagonal (`domain / application / infrastructure`) y sus pruebas en `tests/`. Eurytion, en Rust, es un crate de Cargo dentro de `orthros/eurytion` (ver [0002](0002-lenguajes-por-proyecto.md)).

```text
cerbero/
├── pyproject.toml          # workspace de uv (virtual)
├── shared/contracts/       # cerbero-contracts: lo único compartido
├── orthros/
│   ├── control-plane/      # Python
│   ├── eurytion/           # Rust (Cargo)
│   └── nsm/                # Python
├── echidna/  estigia/  caronte/  hades/  hermes/  hecate/  tenaro/
└── docs/                   # documentación por componente y ADR
```

## Consecuencias

- **Lo único compartido es `shared/contracts`** (`cerbero-contracts`). Un proyecto puede depender de los contratos, nunca del código de otro proyecto.
- Cada miembro se puede construir y desplegar por separado (`uv build --package <nombre>`).
- Si algún día un proyecto necesita su propio repositorio, se puede extraer: ya es un paquete independiente.
