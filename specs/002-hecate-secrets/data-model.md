# Data Model: Secretos de Hécate

Entidades de la [spec](spec.md) con sus campos, reglas y eventos. Decisiones en [research.md](research.md).

## Ruta de un secreto (valor)

`secret://<identidad>/<resto>`:

- `<identidad>`: `[a-z][a-z0-9-]{0,62}`.
- `<resto>`: uno o más segmentos `[a-z0-9_-]{1,64}` separados por `/`; sin segmentos vacíos.
- Referencia de resolución: la ruta, opcionalmente con `@<n>` para una versión concreta (`n ≥ 1`).

## Secret (aggregate, event-sourced, stream `secret/<ruta>`)

Estado reconstruido a partir de sus eventos:

| Campo | Contenido |
|---|---|
| `path` | Ruta |
| `owner` | Identidad dueña (el primer segmento) |
| `value_type` | `string`, `json` o `pem` |
| `max_versions` | 10 por defecto, de 2 a 100 (con 1, rotar destruiría en el mismo cambio la versión que acaba de pasar a `deprecated`, sin dejarle su gracia) |
| `rotate_every`, `expires_at` | Opcionales, informativos; `rotate_every` es un texto imprimible de 1 a 64 caracteres |
| `versions` | Por número: `state`, `created_at`, `deprecated_at`, `fingerprint`, `key_version` |
| `stream_version` | Número de eventos; es la versión esperada para el siguiente cambio |

Eventos (ninguno lleva el valor ni una clave):

| Evento | Efecto |
|---|---|
| `SecretCreated` | Ruta, dueño, tipo y metadatos |
| `VersionAdded` | Versión `n` `active`, huella, versión de la clave de ámbito usada |
| `VersionDeprecated` | La anterior `active` pasa a `deprecated` (con `deprecated_at`) |
| `VersionRevoked` | Revocación explícita |
| `VersionRestored` | `revoked` → `deprecated`, con `deprecated_at` nuevo |
| `VersionPromoted` | `deprecated` → `active` (y la activa pasa a `deprecated` con su propio evento) |
| `VersionDestroyed` | Motivo: `admin` o `max_versions` |

Reglas:

- **Estado efectivo**: una versión `deprecated` cuya `deprecated_at` más la gracia (24 h, configurable) ya pasó es
  `revoked` (research R5).
- **Transiciones**: `active` → `deprecated` | `revoked`; `deprecated` → `active` | `revoked`; `revoked` →
  `deprecated` | `destroyed`. Cualquier otra se rechaza con `conflict`.
- **Una sola `active`** como máximo.
- **Máximo de versiones**: al añadir una versión que lo supere, se destruye la más antigua no destruida en el mismo
  cambio (motivo `max_versions`). Solo la versión anterior inmediata tiene garantizada su gracia (SC-005): con N
  rotaciones dentro de la gracia y un máximo de N, la más antigua se destruye aunque siga en gracia (FR-010).
- **Concurrencia**: cada cambio lleva la versión esperada del stream; si no coincide, `conflict` (FR-017).

## Key (aggregate, event-sourced, stream `key/<identidad>`)

| Campo | Contenido |
|---|---|
| `identity` | Identidad |
| `versions` | Por número: `state` (`active`; la rotación queda fuera de alcance) y `created_at` |

Los eventos de ambos aggregates se codifican igual (tipo, datos JSON sin valores ni claves y fecha) con el módulo
`domain/events.py`; las fechas son los campos `at` o que acaban en `_at`.

Eventos: `ScopeKeyCreated` (versión 1, al guardar el primer secreto de la identidad, FR-018).

## Material cifrado (SQLite, fuera de los eventos)

| Tabla | Clave | Contenido | Se borra |
|---|---|---|---|
| `key_material` | `identity`, `key_version` | Clave de ámbito envuelta por la raíz (nonce, texto cifrado) | Nunca en esta feature |
| `secret_material` | `path`, `version` | Valor cifrado con la clave de versión; clave de versión envuelta por la clave de ámbito; `key_version` | Al destruir la versión (borrado criptográfico) |

Datos asociados de cada cifrado en research R3.

## Valor

Texto UTF-8 no vacío de como mucho 64 KiB, válido para su tipo (R11).

## Huella (valor)

HMAC-SHA256 del valor con una clave derivada de la clave de ámbito, truncado a 8 bytes y mostrado en grupos de 4
caracteres hexadecimales (research R4). No permite recuperar el valor ni probar valores sin la clave.

## Resolución (respuesta, nunca se guarda)

Como mucho 100 referencias por petición. Por referencia: `path`, `version`, `value`, `cache_ttl_soft_s` (300) y `cache_ttl_hard_s` (3600). Todo o nada: si
una referencia falla, ninguna se devuelve, y el motivo único es `not_found` tanto si no existe, como si no está
permitida o no tiene versión `active` (FR-013, FR-014).

## Registro de accesos (ampliado)

Columnas nuevas, opcionales: `subject` (identidad o `admin`), `path`, `version`. Operaciones nuevas: `secret_create`, `secret_add_version`,
`secret_list`, `secret_get`, `secret_revoke`, `secret_restore`, `secret_promote`, `secret_destroy`,
`secret_resolve`. Nunca un valor ni una huella. Valores nuevos de los campos existentes: resultado `not_found` y método `identity` (la resolución por una identidad; el admin usa `local`).

## Estado de Hécate (ampliado)

`secrets_expired` y `secrets_expiring_soon` (caducan en los próximos 7 días): recuentos, sin rutas (FR-022).
