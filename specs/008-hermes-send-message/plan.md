# Implementation Plan: Base de Hermes y envío de mensajes por Telegram

**Branch**: `feature/008-hermes-send-message` | **Date**: 2026-11-14 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/008-hermes-send-message/spec.md`

## Summary

Hermes pasa de diseñado a construido: su base de servicio y su primera función, `chat.send_message` con Telegram.

- **Servicio y proxy.** FastAPI servido por uvicorn en un socket Unix, detrás de Envoy, que termina el mTLS, reenvía el
  certificado del cliente con su cadena en `X-Forwarded-Client-Cert` y recarga el certificado que Hermes renueva (R1,
  R2, [ADR 0011](../../docs/adr/0011-pila-hermes.md)).
- **Autenticación.** Hermes valida la cadena, la lista de revocación, el SPIFFE ID y el token corto como pide la guía de
  Hécate, guarda los `jti` en su base y recoge el contexto de auditoría del claim `cerbero_audit` (R3).
- **Identidad con Hécate.** Alta con el token que da `hermes enroll`, renovación a los dos tercios repitiendo la misma
  petición, publicaciones refrescadas antes de caducar y secretos con su caché blanda y dura (R4, R5).
- **Persistencia.** Su base en el PostgreSQL común, con SQLAlchemy 2.1 Core asíncrono, psycopg 3 y Alembic:
  configuración, canales, idempotencia, `jti` y auditoría append-only (R6,
  [ADR 0012](../../docs/adr/0012-persistencia-postgresql-comun.md)).
- **Configuración.** Un fichero YAML validado entero, que la CLI aplica en una transacción tras enseñar los cambios y
  que exporta sin secretos ni direcciones; las versiones de plantilla no cambian (R7, R15).
- **Envío.** Plantilla con sustitución propia, mensaje neutral y HTML de Telegram con recorte; reintentos solo seguros,
  `outcome_unknown`, idempotencia y límites por instancia (R8 a R11, R13).
- **Verificación.** El admin pide el código con la CLI, la persona se lo envía al bot y el sondeo de Telegram liga el
  canal (R12).
- **Contrato.** `hermes.v1.json` en `shared/contracts`; el sobre común gana tres campos opcionales en `meta` y pasa a la
  1.1.0 (R16).
- **Pruebas.** El proveedor fake y la batería de conformidad del tipo `chat`, una Hécate real en las pruebas de su
  adaptador y un E2E con Envoy en contenedores (R17, R18).

Las decisiones del propietario están en [research R0](research.md#r0-decisiones-del-propietario).

**Alineado con la spec:** una `Idempotency-Key` repetida con otro cuerpo responde `conflict`, como el contrato común y
Hécate; FR-016 y el escenario 2 de la historia 5 decían `validation` y se alinearon al revisar el plan
([research R11](research.md#r11-idempotencia)).

## Technical Context

**Language/Version**: Python 3.15 con uv 0.13 y `uv_build` (ADR 0005 y constitución)

**Primary Dependencies**: las del ADR 0011, comprobadas en Python 3.15; las versiones exactas quedan en `uv.lock`:

- FastAPI 0.162.7, con Pydantic 2.14.3, y uvicorn 0.54.0;
- httpx 0.28.1;
- cryptography 50.0.2 y PyJWT 2.15.1;
- SQLAlchemy 2.1.8, psycopg 3.3.9 (con `binary` y `pool`) y Alembic 1.20.1;
- PyYAML 6.0.3 y jsonschema 4.26.0;
- Envoy y PostgreSQL, en la última versión estable, fijadas por su imagen en el compose de pruebas.

**Configuration**: variables de entorno sin secretos
([contracts/cli.md](contracts/cli.md#variables-de-entorno)): `HERMES_STATE_DIR`, `HERMES_PROXY_DIR`, `HERMES_RUN_DIR`,
`HERMES_ADMIN_DIR`, `HERMES_HECATE_URL`, `HERMES_DATABASE_URL`, `HERMES_DATABASE_PASSWORD_REF`,
`HERMES_VERIFICATION_CODE_MINUTES`, `HERMES_FAKE_PROVIDERS` y `HERMES_LOG_LEVEL`

**Storage**: su base en el PostgreSQL común (ADR 0012), con ocho tablas y la de Alembic
([data-model.md](data-model.md)); la identidad, en ficheros: el directorio de estado y el del proxy

**Testing**: pytest e hypothesis (ADR 0004), anyio, respx, testcontainers, jsonschema y referencing; una Hécate real con
`HECATE_BIN`; el E2E con docker compose y Envoy; ruff, mypy en modo estricto y pip-audit; la suite de Hécate en macOS y
Linux por el cambio del sobre común

**Target Platform**: Linux (servidor de casa, en contenedores) y macOS (desarrollo)

**Project Type**: servicio con una API interna detrás de un proxy, un canal local de administración y una CLI, en un
proceso

**Performance Goals**: el 95 % de los envíos a un contacto verificado llegan en menos de 5 segundos (SC-001); 100
repeticiones con la misma clave dan como mucho un mensaje (SC-003)

**Limits**:

- cuerpo de 64 KiB y un destinatario por envío;
- fichero de configuración de 1 MiB;
- código de verificación de 30 minutos, de 5 a 1440;
- token corto de 300 s, con 30 s de margen; publicaciones refrescadas cada 240 s;
- caché de secretos de 300 s el blando y 3600 s el duro;
- renovación a los dos tercios de la vida, con reintentos cada 60 s;
- timeout técnico de 10 s; límite de uso de 30 por minuto por defecto, de 1 a 600;
- claves de idempotencia durante 24 horas.

**Constraints**:

- Ningún secreto en logs, eventos ni respuestas, y en disco solo las claves propias de Hermes, en ficheros 0600; los
  valores resueltos, solo en memoria (FR-009).
- Ninguna dirección de contacto en los logs, las respuestas, la auditoría ni lo exportado (FR-030, SC-007).
- Hermes sin puerto TCP: solo Envoy llega a su API (FR-001).
- El proveedor fake nunca en producción (R17).
- Hécate no cambia, salvo la versión del sobre común que fija su prueba de contrato (R16).

**Scale/Scope**: una instancia de Hermes, pocos bots y decenas de contactos y plantillas; 6 historias, 32 requisitos y
10 criterios

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principio / regla | Cumplimiento |
|---|---|
| I. Componentes independientes | Hermes es su miembro del workspace de uv, con su código en `src/hermes` y sus pruebas en `tests/`. Solo depende de `shared/contracts`; habla con Hécate por su API, y sus pruebas usan el binario de Hécate, no su código. El cambio del sobre común es compatible hacia atrás: en Hécate solo cambia la versión que fija su prueba de contrato |
| II. Contrato primero | Los contratos están escritos antes del código ([contracts/](contracts/README.md)), y la API se publica en `hermes.v1.json` con el sobre común, `/v1/capabilities`, la versión mayor en la ruta e `Idempotency-Key` en `send_message`. El sobre común se revisa (1.1.0) porque Hermes necesita `service`, `provider` e `idempotency_key` en `meta`. El canal local usa el sobre y los códigos comunes. Una clave con otro cuerpo responde `conflict`, como el contrato común (R11) |
| III. Hexagonal con intensidad justificada | `domain / application / infrastructure`; el tipo de servicio `chat` es un puerto, y Telegram y el fake son sus adaptadores, como fija docs/hermes.md. Sin Event Sourcing ni CQRS, que en Hermes son de las tasks. La pieza nueva, Envoy, está en Complexity Tracking. Se diseña por categoría: el contrato del tipo `chat`, y el detalle de Telegram en su librería |
| IV. Pruebas primero | TDD en dominio, aplicación, API, canal local, CLI y adaptadores; pruebas de contrato de todas las respuestas; batería de conformidad del tipo `chat` que pasan el fake y Telegram; un fake del tipo `chat` que nunca llega a producción (R17, R18) |
| V. Seguridad por diseño | mTLS con SPIFFE y token corto ligado a la identidad del certificado; ninguna operación sin autenticar, y la salud va por el canal local. Los secretos son referencias `secret://` que resuelve Hécate, también la contraseña de la base; los contactos y las variables solo viven en la base. Cada envío, aplicación y verificación queda atribuido en la auditoría. Dos riesgos aceptados se registran en DECISIONS al cerrar: el token ligado a la identidad que declara el proxy y los secretos en la memoria de Python |
| VI. Degradación elegante | Hermes declara su comportamiento ante cada fallo: Hécate sellada o caída (caché hasta el tiempo duro y últimas publicaciones), su base caída (`unavailable`, sin enviar lo que no puede registrar), Telegram (reintentos solo seguros y `outcome_unknown`) y su propio corte (idempotencia) |
| VII. Referencia | SecOps SOAR para las integraciones, con las desviaciones de Hermes ya en DECISIONS. El proxy toma como referencia la cabecera XFCC de Envoy; el resto del diseño de esta feature es propio |
| Lenguajes | Python 3.15 con uv y `uv_build` (ADR 0005) |
| Herramientas | Dependencias maduras de su ecosistema, comprobadas en Python 3.15 y auditadas con pip-audit; Envoy y PostgreSQL, maduros y probados |
| Licencia | Hermes declara `AGPL-3.0-or-later` en su manifiesto; la licencia no condiciona las dependencias |
| Nueva tecnología vía ADR | El ADR 0011 (FastAPI, Envoy, httpx, cryptography, PyJWT, SQLAlchemy, Alembic y PyYAML) y el ADR 0012 (PostgreSQL común), escritos en este plan antes de usarla. Resuelven «APIs internas en Python y certificado del cliente» y «Persistencia por proyecto», y DECISIONS los recoge al cerrar |
| Operación | Logs JSON a la salida estándar, sin secretos ni datos personales, y configuración por entorno sin secretos: la contraseña de la base va como `secret://`. La excepción de OpenTelemetry está en Complexity Tracking |
| Orden y ramas | Ciclo completo en `feature/008-hermes-send-message`; squash `feat:` |
| Una cosa a la vez | Es el único cambio en curso y no hay cambios pausados |
| Consolidación | Los docs se actualizan al cerrar, con la lista de R20 (FR-032) |

**Resultado:** pasa, con las excepciones justificadas en Complexity Tracking.

**Revisión tras el diseño (fase 1):** sigue pasando.

- **Datos:** ocho tablas y la de Alembic, sin streams de eventos; la auditoría es append-only por un disparador.
- **Contrato:** las tres operaciones siguen el contrato común, y el cambio de `meta` es compatible hacia atrás. La spec
  se alineó con el contrato común: una clave repetida con otro cuerpo responde `conflict`.
- **Decisiones abiertas:** solo se resuelven las que decidió el propietario, el proxy y la persistencia, con sus ADR, y
  el detalle de contratos y versionado de Hermes, que es el objeto de la feature. El despliegue en el home lab, las
  copias de seguridad y la infraestructura de observabilidad siguen abiertos.

## Project Structure

### Documentation (this feature)

```text
specs/008-hermes-send-message/
├── spec.md              # /speckit-specify y /speckit-clarify
├── plan.md              # Este fichero
├── research.md          # Fase 0
├── data-model.md        # Fase 1
├── quickstart.md        # Fase 1: validación de extremo a extremo
├── contracts/
│   ├── README.md        # Qué contratos hay y dónde se publican
│   ├── api.md           # La API de Hermes v1
│   ├── proxy.md         # Envoy y la cabecera del certificado
│   ├── admin-channel.md # Operaciones del canal local
│   ├── cli.md           # Órdenes de la CLI hermes y variables de entorno
│   ├── config-file.md   # Formato del fichero de configuración
│   ├── telegram.md      # Librería de Telegram
│   └── conformance.md   # Batería de conformidad del tipo chat
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
docs/adr/
├── 0011-pila-hermes.md                    # Escrito en este plan
├── 0012-persistencia-postgresql-comun.md  # Escrito en este plan
└── README.md                              # Índice con el 0011 y el 0012
shared/contracts/src/cerbero_contracts/openapi/
├── common.v1.json                         # 1.1.0: service, provider e idempotency_key en meta
└── hermes.v1.json                         # Nuevo: chat v1, capacidades y prueba de conexión
hecate/crates/infrastructure/tests/contract_schema.rs  # Fija la versión 1.1.0 del sobre común
hermes/
├── pyproject.toml                         # Dependencias del ADR 0011 y la entrada `hermes`
├── Dockerfile                             # Imagen de pruebas: python:3.15-slim, UID 10001
├── deploy/                                # Entorno de pruebas (R21)
│   ├── compose.yaml                       # PostgreSQL, Hécate, Hermes y Envoy
│   ├── prepare.sh                         # Genera la contraseña de PostgreSQL en .secrets/
│   ├── .gitignore                         # .secrets/
│   ├── envoy/                             # envoy.yaml y los recursos SDS (contracts/proxy.md)
│   ├── postgres/                          # Crea el usuario y la base de Hermes
│   └── hecate/Dockerfile                  # Hécate compilada para el E2E
├── src/hermes/
│   ├── domain/
│   │   ├── shared/                        # ErrorCode, nombres, referencias y Clock
│   │   ├── configuration/                 # Configuración y sus entidades; validación y diferencia
│   │   ├── messages/                      # Mensaje neutral y sustitución de las plantillas
│   │   ├── verification/                  # Código de verificación
│   │   ├── idempotency/                   # Clave de idempotencia y huella de la petición
│   │   └── services/chat/                 # Contrato del tipo chat y puerto ChatProvider
│   ├── application/
│   │   ├── ports.py                       # Repositorios, Hécate, proveedores, auditoría y reloj
│   │   ├── authentication.py              # Certificado reenviado y token corto
│   │   ├── identity.py                    # Alta, renovación y publicaciones
│   │   ├── secrets.py                     # SecretCache
│   │   ├── send_message.py                # chat.send_message
│   │   ├── configuration.py               # Plan, aplicar y exportar
│   │   ├── verification.py                # Pedir y recoger códigos
│   │   ├── capabilities.py                # Capacidades, prueba de conexión y estado de las instancias
│   │   └── health.py                      # live y ready
│   └── infrastructure/
│       ├── api/                           # FastAPI: rutas, sobre, cabecera del proxy y autenticación
│       ├── admin/                         # App del canal local y CLI hermes
│       ├── persistence/                   # Tablas, repositorios y migraciones de Alembic
│       ├── hecate/                        # Cliente httpx con la verificación TLS y almacén de identidad
│       ├── providers/
│       │   ├── telegram/                  # manifest.json, adaptador, sondeo y dibujo
│       │   └── fake/                      # manifest.json y adaptador en memoria
│       ├── config_file/                   # Cargador YAML seguro y modelos estrictos
│       ├── logging.py                     # Logs JSON con enmascarado
│       └── main.py                        # Composición: serve y CLI
└── tests/
    ├── unit/                              # Dominio y aplicación, con dobles y reloj inyectado
    ├── integration/                       # PostgreSQL, Telegram con respx, Hécate real y canal local
    ├── contract/                          # OpenAPI y batería de conformidad del tipo chat
    ├── e2e/                               # Compose con Envoy y el cliente de referencia
    └── support/                           # Cliente de referencia, Hécate de pruebas y respuestas grabadas
```

**Structure Decision:** el esqueleto de docs/hermes.md (`domain`, `application` e `infrastructure`, con `api` y
`providers`), con un módulo por concepto del dominio. Los casos de uso viven en la aplicación y dependen de puertos; la
API, el canal local, la base, Hécate y los proveedores son adaptadores. `tests/` conserva `unit` e `integration` y gana
`contract`, `e2e` y `support`. `deploy/` es solo el entorno de pruebas.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Pieza nueva: Envoy delante de Hermes | uvicorn no pasa a FastAPI el certificado del cliente, que Hermes necesita para ligar el token corto a su identidad | uvicorn adaptado depende de detalles internos que cambian entre versiones, y aiohttp saca a Hermes de FastAPI (decisión del propietario, ADR 0011) |
| Sin instrumentación OpenTelemetry | El colector y el backend siguen abiertos en DECISIONS | Instrumentar ahora exportaría a ningún sitio; se incorpora en todos los componentes cuando se cierre esa decisión |
