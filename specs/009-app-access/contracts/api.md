# Contrato: la consulta de Hécate y lo que cambia en la API de Hermes (v1)

Decisiones en [research.md](../research.md) (R3, R5 y R7).

## Hécate: `GET /v1/callers`

La lista de acceso de la app que pregunta: qué identidades pueden llamarla (historia 2). Va por la API mTLS de la
feature 003, con el mismo transporte, sobre, códigos y autenticación: certificado de cliente válido de la CA vigente y
token corto en `Authorization: Bearer`. Cabeceras: `Authorization` y `X-Request-Id` (opcional). Sin `Idempotency-Key`:
es una consulta sin efecto. Al implementar se escribe en `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`
con el `operationId` `listCallers`, y el documento pasa a la 1.1.0.

Si va bien:

```json
{ "app": "hermes", "callers": ["caller", "hades"], "version": 3 }
```

| Campo | Regla |
|---|---|
| `app` | La identidad de quien pregunta: la consulta no tiene parámetros, así que nadie puede pedir la lista de otra app (FR-006) |
| `callers` | Las identidades que pueden llamarla, ordenadas por nombre; vacía si no hay ninguna, que no es un error |
| `version` | La versión de su lista (data-model.md); 0 si nunca se le ha cambiado nada |

### Errores

| Código | HTTP | Cuándo | `retryable` |
|---|---|---|---|
| `validation` | 400 | `X-Request-Id` mal formado | No |
| `auth` | 401 | Certificado o token corto que no valen, como en la feature 003 | No |
| `unavailable` | 503 | Hécate sellada | Sí |
| `internal` | 500 | Fallo inesperado | Sí |

- **Filas:** atendida no deja fila en el registro de accesos; un rechazo, sí (`callers_list`), también los de antes de
  la operación (Hécate sellada, `X-Request-Id` mal formado), como en la feature 003.
- **`GET /v1/capabilities`** lista también `callers`.

## Hermes: lo que cambia en su API

Las tres operaciones de la feature 008 no cambian de forma. Cambia cuándo responden `auth` y `unavailable`:

| Código | HTTP | Cuándo, además de los casos de la 008 |
|---|---|---|
| `auth` | 401 | Quien llama está autenticado pero no está en la lista de Hermes, ni tras volver a pedirla a Hécate. Mismo mensaje que cualquier otro fallo de autenticación |
| `unavailable` | 503 | Hermes no tiene ninguna copia de su lista, o quien llama no está en la copia y Hécate no responde al volver a pedirla |

- **Orden de las comprobaciones:** el certificado, el token, el acceso y, después, el `jti` y el resto de la 008. Quien
  no tiene acceso no deja `jti` guardado.
- **Lo que diga el token** sobre permisos, roles o accesos no cuenta (FR-007).
- **El log** dice el motivo del rechazo, `not_allowed`, con el `request_id` de la petición; la respuesta, nunca.
- Las descripciones del 401 y del 503 de `hermes.v1.json` lo dicen, y el documento pasa a la 1.0.1.
