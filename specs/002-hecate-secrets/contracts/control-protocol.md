# Contrato: operaciones de secretos en el canal local

Amplía el canal local de administración de la feature 001
([contrato original](../../001-hecate-bootstrap/contracts/control-protocol.md)): mismo transporte, mismo sobre y
mismos códigos de error. Decisiones en [research.md](../research.md) (R8-R10).

## Reglas comunes

- Todas exigen Hécate desbloqueada; si no, `conflict` con el estado (FR-015).
- Ninguna respuesta contiene el valor de un secreto (FR-019).
- Las que cambian un secreto llevan `expected_version` (la `stream_version` que devolvió `secrets.get`); si no
  coincide, `conflict` con `data.current_version`.
- Toda operación deja una fila en el registro de accesos con `subject: admin`, la ruta y la versión.

## Operaciones

| `op` | `args` | `data` si va bien |
|---|---|---|
| `secrets.create` | `path`, `value`, `value_type`; opcionales `max_versions`, `rotate_every`, `expires_at` | `path`, `version` (1), `fingerprint`, `stream_version` |
| `secrets.add_version` | `path`, `value`, `expected_version` (los tres obligatorios) | `version`, `fingerprint`, `stream_version` |
| `secrets.list` | `identity` (opcional) | `secrets`: lista de `{path, value_type, active_version, version_count, rotate_every, expires_at}` |
| `secrets.get` | `path` | Metadatos, `stream_version` y `versions`: lista de `{version, state, created_at, deprecated_at, fingerprint}` |
| `secrets.revoke` | `path`, `version`, `expected_version` | `stream_version` |
| `secrets.restore` | `path`, `version`, `expected_version` | `stream_version` |
| `secrets.promote` | `path`, `version`, `expected_version` | `stream_version` |
| `secrets.destroy` | `path`, `version`, `expected_version`, `proof` (`{passphrase}` o `{shares}`) | `stream_version` |

## Errores

| Situación | Código |
|---|---|
| Ruta mal formada, tipo desconocido, valor no válido para su tipo, vacío o de más de 64 KiB, `max_versions` fuera de 2 a 100, `rotate_every` que no sea un texto imprimible de 1 a 64 caracteres, `expires_at` que no sea una fecha y hora ISO 8601, `expected_version` ausente en `secrets.add_version` | `validation` |
| Ruta inexistente en `get`, `add_version`, `revoke`, `restore`, `promote` o `destroy`; versión inexistente | `not_found` |
| Transición no permitida, `expected_version` desactualizada (con `data.current_version`), `secrets.create` sobre una ruta existente, Hécate sellada | `conflict` |
| Prueba de `destroy` incorrecta (cuenta en el contador común) | `auth` |
| Espera pendiente al verificar la prueba de `destroy` | `rate_limited` |

`not_found` es uno de los códigos comunes del sobre (docs/hermes.md, GLOSSARY).

## `status` (ampliado)

Añade `secrets_expired` y `secrets_expiring_soon` (recuentos, sin rutas). Disponibles también con Hécate sellada.

## Resolución (sin operación en el canal)

La resolución es un caso de uso interno (`resolve(identity, references)`), sin operación en este canal: su canal
autenticado llega con la feature de identidades (aclaración Q1 de la spec). Sus reglas (todo o nada, `not_found`
único, tiempos de caché) están en [data-model.md](../data-model.md).

## Pruebas de contrato

Cada operación y cada código de esta tabla tiene al menos una prueba en `hecate/tests/contract/`.
