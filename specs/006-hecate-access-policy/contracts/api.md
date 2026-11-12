# Contrato: API mTLS de Hécate, resolución y estado (v1)

Dos operaciones que esta feature añade a la API de la feature 003, con el mismo transporte, sobre, códigos y
autenticación: certificado de cliente válido de la CA vigente y token corto en `Authorization: Bearer`
([specs/003-hecate-identities/contracts/api.md](../../003-hecate-identities/contracts/api.md)). Al implementar se
escriben en `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`. Decisiones en
[research.md](../research.md) (R1, R2, R4, R8 y R9).

## Errores

| Código | HTTP | Cuándo | `retryable` |
|---|---|---|---|
| `validation` | 400 | Cuerpo que no es JSON estándar (sin fila), campos que faltan o sobran, ninguna o más de 100 referencias, una referencia mal formada, una versión que no es un entero de 1 a 9 cifras sin ceros a la izquierda, `X-Request-Id` mal formado | No |
| `auth` | 401 | Certificado o token corto que no valen, como en la feature 003; en el estado, además, una identidad sin el permiso de estado | No |
| `not_found` | 404 | Resolución: alguna referencia no existe, es de otra identidad sin excepción o no tiene versión resoluble. El mismo mensaje en los tres casos (`secret not found`) | No |
| `unavailable` | 503 | Resolución con Hécate sellada. El estado no lo usa: responde sellada | Sí |
| `internal` | 500 | Fallo inesperado | Sí |

## Operaciones

### `POST /v1/secrets:resolve`

Resuelve los secretos de la identidad que llama y los que su Access Policy le comparte (historias 1 y 2). Cabeceras:
`Authorization` y `X-Request-Id` (opcional). Sin `Idempotency-Key`: no cambia nada salvo el registro de accesos.

```json
{ "references": ["secret://hermes/telegram/bot_token", "secret://hades/smtp/password@3"] }
```

Si va bien, los secretos en el orden pedido ([formats.md](formats.md#secreto-resuelto)):

```json
{
  "secrets": [
    {
      "reference": "secret://hermes/telegram/bot_token",
      "version": 4,
      "value": "123456:ABC-DEF...",
      "cache_ttl_soft_s": 300,
      "cache_ttl_hard_s": 3600
    },
    {
      "reference": "secret://hades/smtp/password@3",
      "version": 3,
      "value": "...",
      "cache_ttl_soft_s": 300,
      "cache_ttl_hard_s": 3600
    }
  ]
}
```

- **Todo o nada:** si una referencia falla, no se devuelve ningún valor (FR-003).
- **Versiones:** sin `@N`, la `active`; con `@N`, una `active` o una `deprecated` en su gracia; nunca una `revoked` ni
  una `destroyed` (FR-002).
- **Permiso:** la identidad lee lo suyo y las rutas exactas que su política le comparte (FR-005, FR-006).
- **Rechazo:** un `not_found` no se responde antes de 250 ms desde que la petición llega a la operación, para que los
  tres casos no se distingan por el tiempo, sea cual sea la referencia que falla (FR-004). La duración del sobre
  (`meta.duration_ms`) incluye esa espera.
- **Memoria:** el valor viaja en búferes que se borran al enviarse (FR-016).

### `GET /v1/status`

El estado agregado de Hécate, para las identidades con el permiso de estado (historia 3). Cabeceras: `Authorization`
y `X-Request-Id` (opcional).

```json
{
  "sealed": false,
  "secrets_expired": 0,
  "secrets_expiring_soon": 1,
  "secrets_due_rotation": 2,
  "identities_expiring_24h": 0,
  "suspicious_renewals_24h": 0,
  "intermediate_days_left": null,
  "enrollment_throttled": false
}
```

- **También sellada:** con Hécate sellada responde con `sealed: true` y los mismos recuentos (FR-012).
- **Sin nombres:** ninguna ruta, identidad ni metadato de un secreto concreto (FR-011). Los campos, en
  [data-model.md](../data-model.md#estado-agregado).
- **Sin el permiso:** `auth`, con fila en el registro.

### `GET /v1/capabilities`

La lista de operaciones gana `secrets:resolve` y `status`.

## Registro de accesos

- Cada resolución deja fila: una por referencia si se atiende y una con la primera referencia que falla si se rechaza.
- El estado solo deja fila al rechazar.
- Los rechazos antes de la operación (Hécate sellada, `X-Request-Id` mal formado) dejan su fila, como en la feature
  003. Una petición que no es JSON estándar, no.
- Nunca un valor, una huella ni un token. Detalle en [data-model.md](../data-model.md#registro-de-accesos).
