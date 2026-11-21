# Implementation Plan: El contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

**Branch**: `fix/hermes-call-contract` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/012-hermes-call-contract/spec.md`

## Summary

La clave de idempotencia de Hermes pasa a guardar solo lo que se envió o pudo enviarse, y el contrato de Hermes dice
cómo es el token corto y qué recibe una repetición.

- **La clave se libera tras un fallo sin envío.** Un fallo que garantiza que no salió nada, reintentable o no, borra la
  fila reservada de la clave en la misma transacción que su entrada de auditoría, como si fallara antes del proveedor.
  Quien llama repite con la misma clave, y Hades reintenta y relanza como está diseñado (R2).
- **Un fallo inesperado al enviar es un resultado desconocido.** Responde `timeout` con `outcome_unknown` en el sobre,
  con HTTP 200, y queda en la clave; `internal` queda para lo inesperado antes del proveedor (R3).
- **El contrato.** `hermes.v1.json` 1.0.2 describe el `kid` de la cabecera del token, la regla de la repetición y las
  respuestas 200, 409 y 500 del envío, sin cambiar esquemas ni operaciones (R4).
- **La decisión y los documentos.** El ADR 0014 recoge lo que guarda la clave; la guía de integración, el diseño de las
  llamadas, la tabla de códigos, el diseño de Hades y el glosario dan la misma regla de repetición; DECISIONS la pasa a
  las tomadas y recoge su referencia, frente a Stripe y al borrador de la IETF; el diseño de alto nivel y el README de
  Hermes nombran el ADR, y el README de Hécate gana la fila de la 009 (R5 a R7).

Las decisiones del propietario están en [research R0](research.md#r0-decisiones-del-propietario). No hace falta
tecnología nueva (R11).

## Technical Context

**Language/Version**: Python 3.15 con uv en Hermes ([ADR 0011](../../docs/adr/0011-pila-hermes.md)); Hécate no
cambia

**Primary Dependencies**: las del ADR 0011, sin ninguna nueva

**Configuration**: sin variables nuevas

**Storage**: la tabla `idempotency_keys` de la base de Hermes, sin cambios de esquema ni migración; cambia qué filas
quedan ([data-model.md](data-model.md))

**Testing**: pytest, anyio e hypothesis (ADR 0004), con los dobles de `hermes/tests/support`, el PostgreSQL de usar y
tirar en Docker y el E2E del compose con Envoy; ruff, mypy en modo estricto y pip-audit. La suite, en macOS y en Linux
(R8, R10)

**Target Platform**: Linux (servidor de casa, en contenedores) y macOS (desarrollo)

**Project Type**: un servicio con API detrás de un proxy, canal local y CLI (Hermes), y su contrato en
`shared/contracts`

**Performance Goals**: sin cambios: liberar la clave es una transacción, como completarla

**Limits**: la clave sigue viviendo 24 horas y la limpieza sigue cada 5 minutos

**Constraints**:

- Ningún envío completado ni con resultado desconocido se vuelve a enviar con su clave (FR-004).
- Cada intento que llega al proveedor deja su entrada de auditoría (FR-005).
- No cambian la clasificación de los fallos de cada proveedor, el reintento interno, la vida de las claves, la API de
  Hécate, el canal local ni las salidas de la CLI (FR-012).
- mypy estricto; ningún secreto ni dirección en los logs ni en la auditoría.

**Scale/Scope**: una instancia de Hermes; 5 historias, 12 requisitos y 6 criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Cambian Hermes y su contrato en `shared/contracts`; Hades no está construido y su diseño solo nombra la regla de Hermes. Hécate no cambia |
| II. Contrato primero | El contrato cambia antes que el código: `hermes.v1.json` 1.0.2 describe el `kid` y la repetición, con sus pruebas de contrato ([contracts/](contracts/README.md)). `common.v1.json` no cambia |
| III. Hexagonal con intensidad justificada | La regla vive en la aplicación (`send_message.py` e `idempotency.py`) y en el port `IdempotencyRepository`; el adaptador de PostgreSQL implementa `release` en una transacción. Sin piezas nuevas (R2) |
| IV. Pruebas primero | TDD: unitarias de la idempotencia y del envío, integración del repositorio y de la API (también sobre PostgreSQL), contrato y la suite en macOS y en Linux (R8, R10) |
| V. Seguridad por diseño | Nada se envía dos veces: lo que pudo salir sigue guardado. Cada intento queda atribuido en la auditoría, también el que libera la clave. La autenticación no cambia; el contrato solo describe el `kid` que ya exige |
| VI. Degradación elegante | Los reintentos solo cuando son seguros: la clave se libera solo cuando el fallo garantiza que no salió nada. Con la base caída al liberar, la fila sigue reservada y la repetición responde un resultado desconocido, como tras un corte |
| VII. Referencia | SecOps SOAR no tiene un equivalente; la referencia es la idempotencia de Stripe, con el borrador de la IETF (R9), y DECISIONS la recoge en sus desviaciones, con las dos diferencias del borrador (R6) |
| Lenguajes y herramientas | Python 3.15; sin dependencias nuevas |
| Nueva tecnología vía ADR | No hay tecnología nueva. Se resuelve la decisión abierta del reintento de Hades, de forma explícita y con el ADR 0014 (R7) |
| Operación | Logs estructurados a stdout, sin variables nuevas. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `fix/hermes-call-contract`; squash `fix:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Consolidación | Los docs se actualizan al cerrar con la lista de R6 (FR-009 a FR-011), y la decisión resuelta pasa al ADR 0014 |

**Resultado:** pasa, con la excepción justificada en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** la misma tabla, sin migración; una transición nueva, la liberación, y un invariante: una fila completada
  guarda un envío completado o un resultado desconocido ([data-model.md](data-model.md)).
- **Contratos:** solo descripciones y la versión de parche; el sobre, los códigos y las operaciones no cambian.
- **Decisiones abiertas:** se resuelve una, la del reintento, con su ADR; el diseño de Hades no extiende la regla a
  Estigia, para no decidir de forma implícita cómo guarda sus claves (R6).

## Project Structure

### Documentation (this feature)

```text
specs/012-hermes-call-contract/
├── spec.md              # /speckit-specify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1: la fila de una clave y sus transiciones
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   └── README.md        # Los cambios de hermes.v1.json y la regla de repetición
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
shared/contracts/src/cerbero_contracts/openapi/
└── hermes.v1.json                      # 1.0.2: el kid, la repetición y las respuestas 200, 409 y 500 del envío
hermes/src/hermes/
├── application/
│   ├── ports.py                        # IdempotencyRepository.release
│   ├── idempotency.py                  # Reservation.release
│   └── send_message.py                 # Liberar tras un fallo sin envío; el fallo inesperado como timeout
└── infrastructure/
    ├── api/
    │   └── routes_chat.py              # El docstring del módulo: lo que guarda la clave
    └── persistence/
        └── idempotency_repository.py   # release: borrar la fila y escribir la auditoría en una transacción
hermes/tests/
├── support/
│   ├── doubles.py                      # MemoryIdempotency.release
│   └── network_api.py                  # NetworkApi recibe el repositorio de las claves
├── unit/application/
│   ├── test_idempotency.py             # La liberación y las peticiones a la vez
│   ├── test_send_message.py            # Qué se libera, qué se completa y el fallo inesperado
│   └── test_authentication.py          # Un token sin kid responde auth, como hoy
├── integration/
│   ├── persistence/test_idempotency_repository.py   # release en una transacción
│   └── api/
│       ├── test_idempotency.py         # La repetición tras cada fallo sin envío, otro cuerpo y el fallo inesperado
│       └── test_idempotency_postgres.py   # Las historias 1 y 3 sobre PostgreSQL
└── contract/test_openapi_schema.py     # La 1.0.2, el kid, la repetición y la 409
hermes/README.md                        # La fila de la 012, el ADR 0014 y las claves de network_api.py
docs/
├── hades.md                            # La misma clave al reintentar y la regla de Hermes
├── hermes.md                           # La clave frente a SecOps y el ADR 0014 en Tomadas
├── adr/
│   ├── 0014-clave-de-idempotencia-hermes.md
│   └── README.md
└── hermes/
    ├── integracion.md                  # La Idempotency-Key, el conflict 409 y la tabla de errores
    ├── llamadas.md                     # La idempotencia, los reintentos seguros y el resultado desconocido
    └── contratos.md                    # conflict, internal y timeout en la tabla de códigos
GLOSSARY.md                             # Idempotency Key
DECISIONS.md                            # La referencia frente a Stripe; el ADR 0014 en Tomadas; fuera la fila abierta
hecate/README.md                        # La fila de la 009
```

**Structure Decision:** la estructura de la 008. La regla es de la aplicación, en el envío y la reserva de la clave, y
el adaptador de PostgreSQL solo gana la transacción de la liberación. La prueba de la API sobre PostgreSQL va en un
fichero propio, con la marca `docker`, para que la pasada de Linux sin Docker la deje fuera, y usa el arnés
`NetworkApi` de `support/`, que gana el repositorio de las claves como parámetro (R8). Las demás pruebas siguen la
organización de `hermes/tests/`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
