# Contratos del arreglo 011

Ningún contrato cambia (FR-009). Los vigentes son los de la feature 008
([contracts/](../../008-hermes-send-message/contracts/README.md)), con el motivo de la salud que añadió la 009.

| Contrato | Por qué no cambia |
|---|---|
| La API de Hermes (`shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`) | Con la identidad rechazada responde `unavailable`, como hoy sin una identidad válida |
| El canal local ([admin-channel.md](../../008-hermes-send-message/contracts/admin-channel.md)) | `identity.enroll` acepta el alta en cuanto la identidad no vale y responde `conflict` si es válida, como hoy; `health` da el mismo motivo, `needs a new enrollment token` |
| La CLI ([cli.md](../../008-hermes-send-message/contracts/cli.md)) | `hermes enroll` y `hermes health` escriben lo mismo y salen con los mismos códigos |
| La API de Hécate (`shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`) | Hécate no cambia; Hermes solo interpreta su `auth` |

## La línea de log nueva

No es un contrato, pero es la línea que la operación dice que busque el admin
([research R5](../research.md#r5-la-línea-del-log)):

| Evento | Nivel | Campos | Cuándo |
|---|---|---|---|
| `identity.refused` | warning | `serial`, la serie del certificado rechazado, y `operation`, la llamada que Hécate rechazó: `renew`, `revocations`, `keys`, `key`, `callers` o `resolve` | Una vez por identidad mientras Hermes no se reinicia: cuando Hécate rechaza con `auth` la primera llamada hecha con ella; tras un reinicio, la vuelve a dejar |

Como todas las de Hermes, es una línea JSON en la salida estándar, con `ts`, `level`, `logger`, `event` y sus campos
([research R19 de la 008](../../008-hermes-send-message/research.md#r19-logs-y-observabilidad)), sin secretos ni
certificados.
