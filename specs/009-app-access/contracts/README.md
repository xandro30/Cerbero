# Contratos de la feature 009

| Contrato | Qué fija | Se publica en |
|---|---|---|
| [api.md](api.md) | La operación nueva de la API mTLS de Hécate, `GET /v1/callers`, y lo que cambia en la API de Hermes | `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` (1.1.0) y las descripciones de `hermes.v1.json` (1.0.1) |
| [control-protocol.md](control-protocol.md) | Las operaciones `access.*` del canal local de Hécate | Aquí y, al cerrar, en docs/hecate/acceso.md y docs/hecate/cli.md |
| [cli.md](cli.md) | Las órdenes `hecate access` y el motivo nuevo de `hermes health` | Aquí y, al cerrar, en docs/hecate/cli.md y docs/hermes.md |

El sobre, los códigos comunes, el transporte y la autenticación son los de la feature 003 en Hécate
([specs/003-hecate-identities/contracts/api.md](../../003-hecate-identities/contracts/api.md)) y los de la feature 008
en Hermes ([specs/008-hermes-send-message/contracts/api.md](../../008-hermes-send-message/contracts/api.md)).
