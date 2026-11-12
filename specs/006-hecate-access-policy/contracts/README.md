# Contratos de la feature 006

| Contrato | Qué fija | Se publica en |
|---|---|---|
| [api.md](api.md) | Las dos operaciones nuevas de la API mTLS v1: resolver secretos y consultar el estado | `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` |
| [control-protocol.md](control-protocol.md) | Las operaciones `policies.*` del canal local, el campo nuevo de `status` y el formato de `rotate_every` en `secrets.create` | Aquí y, al cerrar, en docs/hecate.md |
| [cli.md](cli.md) | Las órdenes `hecate policies` y la línea nueva de `hecate status` | Aquí y, al cerrar, en docs/hecate.md |
| [formats.md](formats.md) | La referencia a un secreto, `rotate_every` como duración y los campos de un secreto resuelto | Aquí |

El sobre, los códigos comunes, el transporte y la autenticación son los de la feature 003
([specs/003-hecate-identities/contracts/api.md](../../003-hecate-identities/contracts/api.md)).
