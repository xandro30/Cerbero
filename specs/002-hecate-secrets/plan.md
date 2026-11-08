# Implementation Plan: Secretos de Hécate

**Branch**: `feature/002-hecate-secrets` | **Date**: 2026-11-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-hecate-secrets/spec.md`

## Summary

Guardar, versionar y resolver secretos de terceros en Hécate. Cada secreto es un aggregate event-sourced con sus
versiones y su ciclo de vida (`active`, `deprecated`, `revoked`, `destroyed`); el valor se cifra por capas (raíz →
clave de ámbito → clave de versión) y el material cifrado vive fuera de los eventos para permitir el borrado
criptográfico. El admin escribe y gestiona por el canal local sin ver nunca un valor, solo una huella con clave. La
resolución por identidad es un caso de uso interno, sin canal, hasta la feature de identidades. Decisiones en
[research.md](research.md).

## Technical Context

**Language/Version**: Python 3.15 (miembro `cerbero-hecate`, sobre lo construido en la feature 001)

**Primary Dependencies**: las de la feature 001 (`cryptography` para AES-256-GCM y HKDF, y la librería estándar para HMAC; `pynacl`;
`pycryptodome`); ninguna nueva

**Configuration**: las variables de la feature 001; el periodo de gracia (24 h) y el máximo de versiones (10) son
ajustes de `SecretsService`, sin variable de entorno nueva

**Storage**: SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)) con `PRAGMA secure_delete` para que una clave borrada no quede en páginas libres (FR-009): tabla `events` append-only para
los aggregates `Secret` y `Key`, y tablas `secret_material` y `key_material` para el material cifrado

**Testing**: pytest + hypothesis ([ADR 0004](../../docs/adr/0004-pruebas-pytest-hypothesis.md))

**Target Platform**: Linux (servidor de casa); desarrollo en macOS

**Project Type**: servicio local con CLI de administración (el de la feature 001)

**Performance Goals**: resolver 10 secretos en menos de 1 s con Hécate desbloqueada (SC-001); no hay derivación de
clave en la resolución, solo AES-GCM

**Limits**: valor no vacío de hasta 64 KiB; `max_versions` de 2 a 100; `rotate_every` de 1 a 64 caracteres imprimibles; hasta 100 referencias por resolución

**Constraints**: ningún valor ni clave en claro en disco, eventos ni logs; atomicidad ante cortes; el admin nunca lee
valores; los rechazos de resolución no revelan si un secreto existe

**Scale/Scope**: decenas de identidades y cientos de secretos; un admin

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Solo toca `hecate/`; la resolución no depende de ningún otro componente |
| II. Contrato primero | Las operaciones nuevas del canal local están en [contracts/control-protocol.md](contracts/control-protocol.md) antes del código; sobre y códigos comunes (`not_found` ya es común). Es un canal local de administración, así que no publica API Contracts ni `/capabilities` (constitución 1.1.0). La resolución no tiene canal y no cruza componentes todavía |
| III. Hexagonal con intensidad justificada | `domain / application / infrastructure`. Event Sourcing en `Secret` y `Key`, como fija la tabla de intensidad de ARCHITECTURE para Hécate |
| IV. Pruebas primero | TDD; propiedades con hypothesis: cualquier byte alterado o cualquier dato movido de ruta, versión o identidad impide descifrar; transiciones del ciclo de vida; una identidad nunca resuelve fuera de su espacio |
| V. Seguridad por diseño | Ningún valor en repositorio, entorno, eventos ni logs; huella con clave; destruir exige prueba; los rechazos no revelan existencia |
| VI. Degradación elegante | Sellada, todo responde `conflict` salvo `status`; la gracia se calcula al leer, sin depender de que Hécate esté en marcha |
| VII. Referencia | OpenBao KV v2 (versiones, `destroy` y metadatos) y Vault Transit (cifrado por capas); el diseño propio ya está en DECISIONS |
| Tecnología nueva vía ADR | Ninguna: SQLite (ADR 0003) y las librerías de la feature 001 |
| Licencia | Sin dependencias nuevas |
| Operación (OpenTelemetry) | Excepción justificada en Complexity Tracking (la misma que en la feature 001) |

**Resultado:** pasa, con la excepción ya justificada.

**Revisión tras el diseño (fase 1):** sigue pasando. El modelo, el contrato y la guía no añaden dependencias ni
superficies. El registro de accesos se amplía con columnas opcionales y una migración al arrancar (research R9).

## Project Structure

### Documentation (this feature)

```text
specs/002-hecate-secrets/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── control-protocol.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Fase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
hecate/
├── src/hecate/
│   ├── domain/
│   │   ├── events.py              # codificación común de eventos (tipo, datos JSON, fecha)
│   │   ├── secrets/               # ruta, Secret (eventos y estado), versiones, transiciones, tipos de valor
│   │   ├── keys/                  # Key (clave de ámbito) y sus eventos
│   │   └── sealing/               # existente: se amplía el registro de accesos y los errores (NotFound)
│   ├── application/
│   │   ├── ports.py               # + EventStore, MaterialStore
│   │   ├── secrets/               # SecretsService: create, add_version, list, get, revoke, restore, promote, destroy, resolve
│   │   └── sealing/service.py     # + raíz y verificación de prueba como operaciones públicas
│   └── infrastructure/
│       ├── store/                 # + eventos y material cifrado en SQLite, migración del registro de accesos
│       ├── control/server.py      # + operaciones secrets.*
│       └── cli/main.py            # + hecate secrets create|put|list|get|revoke|restore|promote|destroy
└── tests/
    ├── unit/                      # dominio de secretos y SecretsService con dobles
    ├── integration/               # SQLite, cifrado por capas, borrado criptográfico, no fuga, cortes
    └── contract/                  # operaciones secrets.* y status ampliado
```

**Structure Decision**: el mismo miembro `hecate/`; los secretos son un módulo nuevo junto al de sellado, con el que
colaboran a través de las operaciones públicas de `SealingService` (research R6).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | Igual que en la feature 001: no hay llamadas entre componentes y el colector sigue abierto | Se incorpora con la API mTLS interna |
| Material cifrado fuera del event store | El borrado criptográfico (FR-009) exige eliminar la clave de una versión, y el event store es append-only | Guardarlo en los eventos haría imposible destruir una versión |
