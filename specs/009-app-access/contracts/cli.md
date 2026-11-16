# Contrato: CLI, listas de acceso y salud de Hermes

Decisiones en [research.md](../research.md) (R2 y R4).

## `hecate access`

Órdenes que esta feature añade a la CLI `hecate`, con los errores y los códigos de salida de las features anteriores
(`error (<código>): <mensaje>` y salida 1). Cada orden llama a una operación del canal local
([control-protocol.md](control-protocol.md)).

| Orden | Operación | Salida si va bien |
|---|---|---|
| `hecate access grant <identidad> <app>` | `access.grant` | `granted <identidad> access to <app> (access version <n>)`, o `unchanged: <identidad> already calls <app>` |
| `hecate access revoke <identidad> <app>` | `access.revoke` | `revoked <identidad> access to <app> (access version <n>)`, o `unchanged: <identidad> does not call <app>` |
| `hecate access list` | `access.list` | Una línea por app: `<app> version <n> callers <n>`; sin ninguna, `no access lists` |
| `hecate access get <app>` | `access.get` | `access <app> version <n>`, una línea `caller <identidad>` por identidad y, si hay historial, `history` seguido de una línea `<versión> <instante> <cambio> <identidad> by <quién>` por evento |

## `hermes health`

Gana un motivo, en el orden del arranque (identidad, lista de acceso, base y configuración):

| Salida | Cuándo |
|---|---|
| `not ready: access list not loaded` | Hermes no tiene ninguna copia de su lista de acceso: ni guardada ni obtenida de Hécate (FR-009) |

La operación `health` del canal local de Hermes devuelve ese motivo en `reason`; el resto de su contrato no cambia
([specs/008-hermes-send-message/contracts/admin-channel.md](../../008-hermes-send-message/contracts/admin-channel.md)).
