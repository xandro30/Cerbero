# Contrato: canal local de Hécate, listas de acceso

Operaciones que esta feature añade al canal local, con el transporte, la petición y el sobre de las features 001 y 002
([specs/001-hecate-bootstrap/contracts/control-protocol.md](../../001-hecate-bootstrap/contracts/control-protocol.md)).
Decisiones en [research.md](../research.md) (R1, R2 y R6).

Las cuatro exigen Hécate desbloqueada (FR-002): sellada, responden `conflict`, como el resto del canal. Cada una deja
una fila en el registro de accesos con `subject: admin`. El admin no pasa versión esperada.

| `op` | `args` | `data` si va bien |
|---|---|---|
| `access.grant` | `identity` (quien llama), `app` | `identity`, `app`, `version`, `changed` |
| `access.revoke` | `identity`, `app` | `identity`, `app`, `version`, `changed` |
| `access.list` | — | `apps`: lista de `{app, callers, version}`, donde `callers` es cuántas identidades la llaman |
| `access.get` | `app` | `app`, `callers`, `version`, `history`: lista de `{version, change, caller, at, by}` |

- **`changed`:** `false` cuando la orden no cambia nada (conceder lo que ya está concedido, retirar lo que no lo está);
  la versión no se mueve y no se escriben eventos, pero la orden deja su fila.
- **`callers`** en `access.get`: las identidades que pueden llamar a la app, ordenadas por nombre.
- **`change`:** `caller_granted` o `caller_revoked`. `at` es un instante como el resto del canal (`isoformat` en UTC).
- **`access.list`:** solo las apps con algún cambio, también si ya no tienen a nadie, ordenadas por nombre.
- **Identidad o app sin alta:** se puede conceder antes de que se den de alta; basta con que los nombres sean válidos y
  no reservados (FR-005).
- **`access.get`** de una app sin cambios devuelve la lista vacía: `callers` vacío, `version: 0` e `history` vacío.

## Errores

| Caso | Código |
|---|---|
| Identidad o app mal formada (también con `/`, como una identidad hija) o reservada (`hecate`, `admin`), una app que se concede a sí misma, argumentos que faltan o sobran | `validation` |
| Hécate sellada | `conflict` |
