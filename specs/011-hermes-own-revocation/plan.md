# Implementation Plan: Hermes nota la revocación urgente de su propio certificado

**Branch**: `fix/hermes-own-revocation` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/011-hermes-own-revocation/spec.md`

## Summary

Hermes pasa a entender el `auth` de Hécate: su identidad ya no vale y necesita un token de alta nuevo.

- **Un solo paso para las llamadas con su identidad.** Toda llamada de Hermes a Hécate con su certificado pasa por el
  servicio de identidad: la renovación, las publicaciones, una clave, su lista de acceso y, como novedad, la
  resolución de sus secretos, que la caché de secretos y la comprobación de la configuración piden al servicio en lugar
  de al cliente de Hécate (R2).
- **El rechazo.** Un `auth` a una de esas llamadas marca como rechazada la identidad con la que se hizo, si sigue
  siendo la vigente: Hermes no está listo, su salud dice `needs a new enrollment token` y su log, una vez,
  `identity.refused`. La CA sustituida, que también llega como `auth`, sigue como hoy (R3, R5).
- **Sin llamadas con la identidad rechazada.** Mientras dura la marca, el servicio responde él mismo `auth` sin llegar
  a Hécate, así que cada camino falla como hoy ante un rechazo y el registro de accesos de Hécate deja de ganar filas
  de Hermes. El alta, que no usa esa identidad, sí sale (R4).
- **Memoria y alta.** La marca vive solo en memoria: un reinicio la pierde y la primera llamada del arranque vuelve a
  comprobarla. El alta nueva trae otra serie, a la que la marca ya no se aplica, y el almacén ya sustituye todo lo
  demás, así que no hace falta vaciar el directorio de estado. Tras el alta, `hermes serve` vuelve a pedir la
  contraseña de la base, que la caché pudo quitar mientras la identidad no valía (R6, R7).

Las decisiones del propietario están en [research R0](research.md#r0-decisiones-del-propietario). No hace falta
tecnología nueva ni ningún ADR, y no cambia ningún contrato (R11).

## Technical Context

**Language/Version**: Python 3.15 con uv en Hermes ([ADR 0011](../../docs/adr/0011-pila-hermes.md)); Hécate no
cambia

**Primary Dependencies**: las del ADR 0011, sin ninguna nueva

**Configuration**: sin variables nuevas

**Storage**: sin cambios. La marca de rechazo vive en memoria (FR-007); el directorio de estado, el del proxy y la base
de Hermes no cambian ([data-model.md](data-model.md))

**Testing**: pytest, anyio e hypothesis (ADR 0004), con el doble `FakeHecate`, una Hécate real (`HECATE_BIN`), Docker y
el E2E del compose con Envoy; ruff, mypy en modo estricto y pip-audit. La suite, en macOS y en Linux (R9)

**Target Platform**: Linux (servidor de casa, en contenedores) y macOS (desarrollo)

**Project Type**: un servicio con API detrás de un proxy, canal local y CLI (Hermes)

**Performance Goals**: Hermes nota la revocación en la primera llamada rechazada y como mucho en el siguiente refresco
de sus publicaciones, a los 240 s (SC-001); tras el alta, vuelve a estar listo en 2 minutos como mucho (SC-003)

**Limits**:

- una línea `identity.refused` por identidad rechazada, mientras Hermes no se reinicia;
- ninguna llamada nueva a Hécate con la identidad rechazada (SC-006);
- el refresco de las publicaciones sigue cada 240 s y el reintento de un paso del arranque, cada 30 s.

**Constraints**:

- Solo un `auth` de Hécate a una llamada con la identidad cambia su validez; los demás fallos, no (FR-003).
- Un rechazo cuenta solo para la identidad con la que se hizo la llamada (FR-004).
- La CA sustituida sigue igual: Hermes sigue intentando y vuelve a estar listo solo (FR-009).
- Ninguna salida, motivo ni contrato cambia (FR-009).
- mypy estricto; ningún secreto ni certificado en los logs.

**Scale/Scope**: una instancia de Hermes; 3 historias, 10 requisitos y 6 criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Solo cambia Hermes. Hécate y `shared/contracts` no cambian; Hermes sigue hablando con Hécate por su API |
| II. Contrato primero | Ningún contrato cambia ([contracts/](contracts/README.md)): la API de Hermes, su canal local, las salidas de la CLI y la API de Hécate siguen iguales |
| III. Hexagonal con intensidad justificada | La regla vive en la aplicación (`identity.py`), donde ya vive la validez de la identidad; la caché de secretos y la comprobación de la configuración dependen de un port estrecho (`SecretResolver`), no del cliente de Hécate; el adaptador de Hécate no cambia (R2) |
| IV. Pruebas primero | TDD: unitarias del servicio de identidad con `FakeHecate`, integración contra una Hécate real, el escenario del E2E del compose y la suite en macOS y en Linux (R8, R9) |
| V. Seguridad por diseño | Un certificado revocado deja de servir en la primera llamada que Hécate rechaza. Hermes deja de llamar a Hécate con él, así que el registro de accesos de Hécate solo muestra a quien use la clave robada. El log no lleva secretos ni certificados |
| VI. Degradación elegante | Con Hécate sellada o caída, nada cambia: solo un rechazo marca la identidad (FR-003). Declarado el comportamiento ante fallos en la spec y en «Ante fallos» de docs/hermes.md |
| VII. Referencia | SecOps no tiene un equivalente para la identidad entre servicios; el comportamiento sigue la guía de integración de Hécate («si su certificado ya no vale, pedir al admin un token de alta nuevo») |
| Lenguajes y herramientas | Python 3.15; sin dependencias nuevas |
| Nueva tecnología vía ADR | No hay tecnología nueva ni decisión abierta que se resuelva aquí: la fila de DECISIONS es un diseño sin construir (R10) |
| Operación | Logs estructurados a stdout, sin variables nuevas. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `fix/hermes-own-revocation`; squash `fix:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Consolidación | Los docs se actualizan al cerrar con la lista de R10 (FR-010) |

**Resultado:** pasa, con la excepción justificada en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** un campo en memoria en el servicio de identidad, que sustituye al booleano de la renovación rechazada;
  ningún fichero, tabla ni migración.
- **Contratos:** ninguno cambia; la línea `identity.refused` es un evento de log, no un contrato.
- **Decisiones abiertas:** ninguna se resuelve de forma implícita; la fila «Revocación urgente de Hermes» de DECISIONS
  se retira porque pasa a estar construida.

## Project Structure

### Documentation (this feature)

```text
specs/011-hermes-own-revocation/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1: la validez de la identidad y sus transiciones
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   └── README.md        # Lo que no cambia y la línea de log nueva
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
hermes/src/hermes/
├── application/
│   ├── ports.py                        # SecretResolver: el port estrecho de la resolución
│   ├── identity.py                     # _as_hermes, la marca de rechazo, resolve e identity.refused
│   ├── secrets.py                      # SecretCache sobre un SecretResolver
│   └── configuration.py                # HecateSecretChecker sobre un SecretResolver
└── infrastructure/
    └── main.py                         # La caché y la comprobación sobre el servicio; la contraseña tras cada alta
hermes/tests/
├── support/fake_hecate.py              # Los rechazos de una revocación urgente y el recuento de llamadas
├── unit/
│   ├── application/
│   │   ├── test_identity_state.py      # El rechazo, sus caminos, el bloqueo, el alta, el reinicio y la carrera
│   │   └── test_secret_cache.py        # La caché sobre el servicio con la identidad rechazada
│   └── test_serve_enrollment.py        # La contraseña de la base tras el alta
├── integration/hecate/test_client.py   # La revocación urgente contra una Hécate real, sus accesos y el alta nueva
└── e2e/test_flow.py                    # El escenario en el compose
docs/
├── hermes.md                           # «Ante fallos»: la fila de la identidad
└── hermes/
    ├── operacion.md                    # Tareas de fondo, «La identidad de Hermes deja de valer» y compose de pruebas
    ├── cli.md                          # El motivo needs a new enrollment token
    └── seguridad.md                    # «La identidad de Hermes» y «Lo que no cubre»
hermes/README.md                        # Lo que recorre el E2E
DECISIONS.md                            # Se retira la fila «Revocación urgente de Hermes»
```

**Structure Decision:** la estructura de la 008. La regla es de la aplicación, en el servicio de identidad, y el
adaptador de Hécate y el almacén de la identidad no cambian. Las pruebas siguen la organización de `hermes/tests/`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
