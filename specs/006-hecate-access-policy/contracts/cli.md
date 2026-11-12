# Contrato: CLI, políticas

Órdenes que esta feature añade a la CLI `hecate`, con los errores y los códigos de salida de las features anteriores
(`error (<código>): <mensaje>` y salida 1). Cada orden llama a una operación del canal local
([control-protocol.md](control-protocol.md)). Decisiones en [research.md](../research.md) (R7).

| Orden | Operación | Salida si va bien |
|---|---|---|
| `hecate policies grant-read <identidad> <secreto>` | `policies.grant_read` | `granted read <secreto> to <identidad> (policy version <n>)`, o `unchanged: <identidad> already reads <secreto>` |
| `hecate policies revoke-read <identidad> <secreto>` | `policies.revoke_read` | `revoked read <secreto> from <identidad> (policy version <n>)`, o `unchanged: <identidad> does not read <secreto>` |
| `hecate policies grant-status <identidad>` | `policies.grant_status` | `granted status to <identidad> (policy version <n>)`, o `unchanged: <identidad> already has status` |
| `hecate policies revoke-status <identidad>` | `policies.revoke_status` | `revoked status from <identidad> (policy version <n>)`, o `unchanged: <identidad> has no status` |
| `hecate policies list` | `policies.list` | Una línea por identidad: `<identidad> version <n> status <yes/no> reads <n>`; sin ninguna, `no policies` |
| `hecate policies get <identidad>` | `policies.get` | `policy <identidad> version <n> status <yes/no>`, una línea `read <secreto>` por excepción y, si hay historial, `history` seguido de una línea `<versión> <instante> <cambio> [<secreto>] by <quién>` por evento |

## Lo que cambia en órdenes que ya existen

- `hecate status` añade la línea `secrets due rotation: <n>`, junto a los recuentos de caducidad.
- `hecate secrets create ... --rotate-every <duración>` acepta solo duraciones ISO 8601 en días o en semanas
  (`P90D`, `P12W`); otro texto da `error (validation): rotate_every must be an ISO 8601 duration in days or weeks,
  such as P90D or P12W`.
