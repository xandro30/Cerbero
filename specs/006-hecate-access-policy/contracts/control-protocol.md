# Contrato: canal local, políticas

Operaciones que esta feature añade al canal local, con el transporte, la petición y el sobre de las features 001 y 002
([specs/001-hecate-bootstrap/contracts/control-protocol.md](../../001-hecate-bootstrap/contracts/control-protocol.md)),
y lo que cambia en dos operaciones que ya existen. Decisiones en [research.md](../research.md) (R3, R4, R5 y R7).

Las operaciones `policies.*` exigen Hécate desbloqueada (FR-014): sellada, responden `conflict`, como el resto del
canal. Cada una deja una fila en el registro de accesos con `subject: admin` y la identidad de la política. El admin
no pasa versión esperada (FR-008).

## Políticas

| `op` | `args` | `data` si va bien |
|---|---|---|
| `policies.grant_read` | `identity`, `secret` (ruta sin `@N`) | `identity`, `secret`, `version`, `changed` |
| `policies.revoke_read` | `identity`, `secret` | `identity`, `secret`, `version`, `changed` |
| `policies.grant_status` | `identity` | `identity`, `version`, `changed` |
| `policies.revoke_status` | `identity` | `identity`, `version`, `changed` |
| `policies.list` | — | `policies`: lista de `{identity, reads, status, version}` |
| `policies.get` | `identity` | `identity`, `reads`, `status`, `version`, `history`: lista de `{version, change, secret, at, by}` |

- **`changed`:** `false` cuando la orden no cambia nada (conceder lo que ya tiene, retirar lo que no tiene); la versión
  no se mueve y no se escriben eventos, pero la orden deja su fila (FR-008).
- **`reads`:** las rutas de otras identidades que puede resolver, ordenadas. Sus propios secretos no aparecen: los lee
  por la regla por defecto.
- **`change`:** `read_granted`, `read_revoked`, `status_granted` o `status_revoked`; `secret` es `null` en los del
  permiso de estado. `at` es un instante como el resto del canal (`isoformat` en UTC).
- **`policies.list`:** solo las identidades con algún cambio de política, ordenadas por nombre.
- **Identidad sin alta:** se le puede conceder una lectura o el permiso de estado antes de darse de alta, como se
  preparan sus secretos; basta con que el nombre sea válido y no reservado.
- **`policies.get`** de una identidad sin política devuelve la política vacía: `reads` vacío, `status: false`,
  `version: 0` e `history` vacío.

## Lo que cambia en operaciones que ya existen

- **`status`** gana `secrets_due_rotation`, que responde también sellada, como el resto de recuentos (FR-013).
- **`secrets.create`:** `rotate_every`, si llega, es una duración ISO 8601 en días o en semanas
  ([formats.md](formats.md#rotate_every)); cualquier otro texto da `validation` (FR-023). Es el único cambio de
  comportamiento de la feature 002.

## Errores

| Caso | Código |
|---|---|
| Identidad mal formada o reservada (`hecate`, `admin`), ruta mal formada o con `@N`, un secreto de la propia identidad, argumentos que faltan o sobran | `validation` |
| `policies.grant_read` de un secreto que no existe | `not_found` |
| Hécate sellada | `conflict` |
| `rotate_every` que no es una duración en días o semanas, en `secrets.create` | `validation` |
