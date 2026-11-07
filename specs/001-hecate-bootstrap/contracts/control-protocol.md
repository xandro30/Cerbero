# Contrato: canal local de control de Hécate

Protocolo entre la CLI del admin y el proceso de Hécate. Decisiones en [research.md](../research.md) (R5-R8).

## Transporte

- Socket Unix en `${HECATE_DATA_DIR}/control.sock` (configurable con `HECATE_SOCKET_PATH`), permisos `0600` y
  directorio `0700`, propiedad del usuario de Hécate. En Linux se rechaza a quien conecte con otro uid.
- Una petición y una respuesta por línea, en JSON UTF-8.

## Petición

```json
{ "request_id": "uuid", "op": "unseal", "args": { "passphrase": "..." } }
```

## Respuesta (sobre común)

```json
{
  "status": "completed | failed",
  "data": { },
  "error": { "code": "auth", "message": "...", "retryable": false, "retry_after_s": 0 },
  "meta": { "request_id": "uuid", "duration_ms": 12 }
}
```

`retry_after_s` solo aparece con `rate_limited`.

## Códigos de error

| Código | Cuándo | ¿Cuenta como intento? | `retryable` |
|---|---|---|---|
| `validation` | Petición mal formada, frase de menos de 12 caracteres, parte con suma incorrecta, partes repetidas | No | No |
| `auth` | Frase incorrecta, partes que no recuperan la raíz o de otro juego | Sí | No |
| `rate_limited` | Hay una espera pendiente | No (se registra) | Sí, tras `retry_after_s` |
| `conflict` | La operación no está permitida en el estado actual | No (se registra) | No |
| `internal` | Fallo inesperado | No | Sí |

## Operaciones

| `op` | Estados admitidos | `args` | `data` si va bien |
|---|---|---|---|
| `status` | Todos | — | `state`, `initialized_at`, `unsealed_via`, `recommend_passphrase_change`, `consecutive_failures`, `retry_after_s` |
| `init.begin` | `uninitialized` | `passphrase` | `init_id`, `shares` (3 textos), `expires_at`, `warning` (FR-018) |
| `init.confirm` | `uninitialized` | `init_id`, `shares` (2 de las mostradas) | `state: sealed` |
| `init.abort` | `uninitialized` | `init_id` | — |
| `unseal` | `sealed` | `passphrase` **o** `shares` (2) | `state: unsealed`, `unsealed_via`, `recommend_passphrase_change` |
| `seal` | `unsealed` | — | `state: sealed` |
| `passphrase.change` | `unsealed` | `proof` (`{passphrase}` o `{shares}`), `new_passphrase` | `config_version` |
| `shares.rekey.begin` | `unsealed` | `proof` | `rekey_id`, `shares` (3 textos nuevos), `expires_at` |
| `shares.rekey.confirm` | `unsealed` | `rekey_id`, `shares` (2 de las nuevas) | `config_version` |
| `shares.rekey.abort` | `unsealed` | `rekey_id` | — |

Reglas comunes:

- `unseal` en `unsealed` responde `conflict` con el estado actual (caso límite "ya desbloqueada").
- Cualquier `op` distinta de `status` y `unseal` en `sealed` responde `conflict` (FR-007).
- `seal` en `sealed` responde `conflict` con `state: sealed`: informa sin cambiar nada (US3, escenario 2).
- Dos `unseal` simultáneos: se atienden en orden; el segundo recibe `conflict`.
- Toda operación reconocida, salvo `status`, deja una fila en el registro de accesos. Las peticiones mal formadas o con una `op` desconocida solo quedan en el log estructurado: no corresponden a ninguna operación del registro.

## Pruebas de contrato

Cada fila de la tabla de operaciones y de códigos tiene al menos una prueba en `hecate/tests/contract/`.
