# Implementation Plan: Arranque de Hécate

**Branch**: `feature/001-hecate-bootstrap` | **Date**: 2026-11-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-hecate-bootstrap/spec.md`

## Summary

Poner en marcha el ciclo de sellado de Hécate: inicializar (raíz, frase del admin y tres partes de
recuperación), arrancar siempre sellada, desbloquear con la frase o con dos partes, sellar a mano y cambiar la
frase o las partes demostrándolo de nuevo. Un proceso de Hécate guarda la raíz solo en memoria y atiende a la
CLI del admin por un socket Unix local; el estado persistente (la raíz cifrada dos veces, el contador de
intentos y el registro de accesos) vive en SQLite. Detalle de las decisiones en [research.md](research.md).

## Technical Context

**Language/Version**: Python 3.15 (miembro `cerbero-hecate` del workspace de uv)

**Primary Dependencies**: `cryptography` (AES-256-GCM), `pynacl` (Argon2id), `pycryptodome` (Shamir);
librería estándar para la CLI (`argparse`, `getpass`) y el canal local (`asyncio`)

**Configuration**: variables de entorno `HECATE_DATA_DIR` (obligatoria), `HECATE_SOCKET_PATH` (por defecto, `control.sock` en el directorio de datos) y `HECATE_KDF_PROFILE` (`sensitive` por defecto o `moderate`; ver [research.md](research.md), R2); no hay ningún perfil más débil

**Storage**: SQLite en el directorio de datos de Hécate ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md))

**Testing**: pytest + hypothesis ([ADR 0004](../../docs/adr/0004-pruebas-pytest-hypothesis.md))

**Target Platform**: Linux (servidor de casa); desarrollo en macOS

**Project Type**: servicio local con CLI de administración

**Performance Goals**: desbloqueo en pocos segundos (dominado por Argon2id) y de principio a fin por debajo de
1 minuto (SC-002); inicialización completa por debajo de 10 minutos (SC-001)

**Constraints**: la raíz, la frase y las partes nunca se guardan ni se registran; atomicidad ante cortes de luz
(SC-008); un solo proceso y una sola instancia; sin superficie de red

**Scale/Scope**: un admin, una instancia, decenas de intentos al día como máximo

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Solo toca `hecate/`; no depende de ningún otro componente ni de `shared/contracts` |
| II. Contrato primero | El protocolo del canal local está escrito en [contracts/control-protocol.md](contracts/control-protocol.md) antes del código; respuestas con el sobre común. Propone añadir `conflict` a los códigos comunes (R7). Es un canal local de administración: según el principio II (constitución 1.1.0) usa el sobre y los códigos comunes y no publica API Contracts ni `/capabilities` ni exige `Idempotency-Key`; las operaciones con efecto se protegen por estado (inicializar dos veces se rechaza) |
| III. Hexagonal con intensidad justificada | `domain / application / infrastructure`; `CryptoProvider`, repositorio y registro de accesos como ports. Event Sourcing queda para los cinco aggregates de Hécate, que no entran en esta feature; el registro de accesos es append-only |
| IV. Pruebas primero | TDD con pytest; propiedades con hypothesis (cualquier par de partes recupera, una sola nunca, cualquier byte alterado impide descifrar) |
| V. Seguridad por diseño | Ningún secreto en código, configuración, variables de entorno ni repositorio; sin superficie de red (socket Unix con permisos del usuario de Hécate); cada intento queda registrado |
| VI. Degradación elegante | Reiniciar deja Hécate sellada; el comportamiento ante fallos está en la spec (casos límite, SC-008) |
| VII. Referencia | OpenBao (sellado, Shamir); las desviaciones (frase más Shamir de recuperación) ya están en DECISIONS |
| Tecnología nueva vía ADR | SQLite (ADR 0003), pytest + hypothesis (ADR 0004). Librerías criptográficas: `cryptography` y PyNaCl ya previstas; PyCryptodome justificada en R4 |
| Licencia | Dependencias Apache-2.0, BSD, MIT y MPL-2.0: compatibles con AGPL-3.0-or-later |
| Operación (OpenTelemetry) | Excepción justificada en Complexity Tracking |

**Resultado:** pasa, con una excepción justificada.

**Revisión tras el diseño (fase 1):** sigue pasando. El protocolo, el modelo y la guía de comprobación no añaden
dependencias ni superficies; el único cambio a un contrato común es la propuesta de `conflict` (R7), que se
lleva al contrato común al consolidar.

## Project Structure

### Documentation (this feature)

```text
specs/001-hecate-bootstrap/
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1
├── contracts/
│   └── control-protocol.md
├── checklists/
│   └── requirements.md
└── tasks.md             # Fase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
hecate/
├── pyproject.toml                 # dependencias y script `hecate`
├── src/hecate/
│   ├── domain/
│   │   └── sealing/               # estado de sellado, protección de la raíz, partes, política de intentos
│   ├── application/
│   │   ├── ports.py               # CryptoProvider, SealRepository, AccessLog, Clock
│   │   └── sealing/               # inicializar, desbloquear, sellar, cambiar frase, repartir partes, estado
│   └── infrastructure/
│       ├── crypto/                # adapter de software de CryptoProvider
│       ├── store/                 # SQLite: protección de la raíz, contador, registro de accesos
│       ├── control/               # servidor del socket Unix y protocolo
│       └── cli/                   # comandos del admin
└── tests/
    ├── unit/                      # dominio y casos de uso con dobles de los ports
    ├── integration/               # adapters reales (SQLite, criptografía, socket)
    └── contract/                  # protocolo del canal local
```

**Structure Decision**: un solo miembro del workspace, `hecate/`, con el esqueleto hexagonal que fija el ADR
0001. El proceso (`hecate serve`) y la CLI (`hecate status|init|unseal|seal|...`) son el mismo paquete con dos
puntos de entrada.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry en esta feature | No hay llamadas entre componentes que trazar y el colector y el backend siguen abiertos en DECISIONS; los intentos ya quedan en el registro y en logs estructurados a stdout | Añadir el SDK ahora obligaría a elegir exportador sin colector; se incorpora con la API mTLS interna |
