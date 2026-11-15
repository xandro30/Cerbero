# API de Hermes v1

La API entre componentes que deja esta feature. Se publica en
`shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json` (OpenAPI 3.1 en JSON), con el sobre común de
`common.v1.json` 1.1.0.

## Transporte

- Por Envoy ([proxy.md](proxy.md)): TLS 1.3, HTTP/1.1 o HTTP/2, en la dirección del proxy. Hermes no escucha en ningún
  puerto TCP.
- Cuerpo en JSON estándar UTF-8, de 64 KiB como mucho; con más, o con un JSON que no es estándar, `validation`.

## Autenticación

Todas las operaciones exigen dos cosas:

- el **certificado de cliente** de un componente de Cerbero, con su intermedia, en la conexión TLS; Envoy lo reenvía en
  `X-Forwarded-Client-Cert`;
- un **token corto** en `Authorization: Bearer <token>`, con el formato de la
  [guía de integración de Hécate](../../../docs/hecate/integracion.md#cada-llamada-autenticada) y `aud` igual al SPIFFE
  ID de Hermes (`spiffe://<trust domain>/hermes`).

Hermes comprueba, por orden:

1. que la cadena llega a la CA raíz vigente de Hécate a la hora de su reloj, que el certificado tiene como único nombre
   un SPIFFE ID de su trust domain, que no ha caducado y que su serie no está en la lista de revocación;
2. que el token lo firmó una clave de firma de llamadas que Hécate publica, activa y de la identidad del certificado; si
   Hermes aún no la tiene, porque el componente se acaba de dar de alta o de renovar, se la pide a Hécate;
3. que su `iss` es el SPIFFE ID del certificado y su `aud` el de Hermes;
4. que `exp - iat` es de 300 s como mucho, con 30 s de margen entre relojes, y que su `jti` no se ha usado antes con el
   mismo `iss`.

Cualquier fallo responde `auth` (HTTP 401), sin efectos y con el mismo mensaje, sea cual sea la comprobación. Si Hécate
no responde cuando Hermes le pide una clave que no tiene, `unavailable` (HTTP 503). Hermes no toma ningún permiso del
token (FR-004).

### Contexto de auditoría

El token puede llevar el claim `cerbero_audit`: un objeto de hasta 16 claves de texto (de 1 a 64 caracteres) con valores
de texto (hasta 256 caracteres), de 2 KiB como mucho al serializarlo. Hermes lo guarda tal cual en la entrada de
auditoría del envío. Con otra forma, la petición responde `validation`. Quien llama decide qué claves usa (por ejemplo,
`source`, `response` o `playbook` en Hades).

## Cabeceras

| Cabecera | Regla |
|---|---|
| `Authorization` | `Bearer <token corto>` |
| `Idempotency-Key` | De 1 a 128 caracteres ASCII imprimibles. Obligatoria en `send_message`; sin ella, `validation` |
| `X-Request-Id` | Opcional, de 1 a 128 caracteres ASCII imprimibles; se repite en `meta.request_id`. Sin ella, Hermes genera uno |
| `Content-Type` | `application/json` en las peticiones con cuerpo |

## Sobre

La estructura de `common.v1.json`: `status` (`completed` o `failed`), `data`, `error` y `meta`. En la versión 1.1.0,
`meta` gana tres campos opcionales, que Hermes rellena cuando aplican:

| Campo | Cuándo |
|---|---|
| `request_id` | Siempre |
| `duration_ms` | Siempre |
| `service` | En las operaciones de un tipo de servicio: `chat` |
| `provider` | Cuando hay un proveedor: `telegram` o `fake` |
| `idempotency_key` | Cuando la petición la trae |

## Estado HTTP

Un fallo del proveedor va dentro del sobre (docs/hermes.md). Por eso, toda respuesta que llegó al proveedor, se
completara o fallara, lleva HTTP 200; los fallos de antes llevan el estado de su código.

| Código | Antes de llegar al proveedor | Tras llegar al proveedor |
|---|---|---|
| `validation` | 400 | — |
| `auth` | 401 | — |
| `not_found` | 404 | — |
| `conflict` | 409 | 200: la persona no admite mensajes del bot |
| `rate_limited` | 429: límite de la instancia, con `Retry-After` | 200: límite del proveedor que no cabe en el timeout |
| `provider_unavailable` | — | 200 |
| `timeout` | — | 200, con `outcome_unknown` |
| `unavailable` | 503: Hermes no está listo o su base no responde | — |
| `internal` | 500: un secreto pasado su tiempo duro, o un fallo inesperado | — |

## `POST /chat/v1/send_message`

Envía un mensaje a un contacto por un canal de chat verificado. Función `command`, síncrona, con su contrato en la
extensión `x-cerbero-function` de la operación:

```json
{
  "kind": "command",
  "mode": "sync",
  "timeout": "10s",
  "side_effect": "Envía a ${recipient} el mensaje «${title}» por la instancia ${instance}",
  "discloses_data": "${provider} recibe el texto del mensaje y el chat de ${recipient}",
  "non_reversible": true
}
```

### Petición

```json
{
  "recipients": ["contacto_demo"],
  "template": { "id": "device_unknown", "version": 1 },
  "data": { "mac": "AA:BB:CC:DD:EE:FF", "ip": "192.168.1.57", "case_id": "case-1234" },
  "instance": "family_bot"
}
```

| Campo | Regla |
|---|---|
| `recipients` | Una lista con exactamente un nombre de contacto (FR-021); con más o con ninguno, `validation` |
| `template.id` | El identificador de una plantilla |
| `template.version` | Opcional; sin él, la última versión |
| `data` | Un objeto que cumple el esquema de esa versión |
| `instance` | Opcional. Con él, el canal del contacto en esa instancia; sin él, el único canal de chat del contacto, y si tiene más de uno, `validation` |

### Respuesta

```json
{
  "status": "completed",
  "data": {
    "recipient": "contacto_demo",
    "instance": "family_bot",
    "provider_message_ref": "4521",
    "sent_at": "2026-11-14T10:00:00.000000Z",
    "truncated": false
  },
  "error": null,
  "meta": {
    "request_id": "req-1", "duration_ms": 640, "service": "chat", "provider": "telegram",
    "idempotency_key": "case-1234-notify"
  }
}
```

`provider_message_ref` es la referencia del mensaje en el proveedor; `truncated` dice si se recortó el detalle. La
respuesta nunca lleva la dirección del contacto.

### Orden y errores

1. Autenticación: `auth`.
2. Cuerpo e `Idempotency-Key`: `validation`.
3. Clave de idempotencia: con otra huella, `conflict`; ya completada, la misma respuesta; en curso tras un corte,
   `timeout` con `outcome_unknown` ([research R11](../research.md#r11-idempotencia)).
4. Plantilla, versión, instancia y contacto: `not_found` si no existen o si el contacto no tiene canal en esa instancia;
   `validation` si los datos no cumplen el esquema o un campo del mensaje no cumple su regla tras sustituir.
5. Canal sin verificar: `conflict` (FR-020).
6. Límite de uso de la instancia: `rate_limited` (FR-028).
7. Token del bot: `internal` si su valor ya pasó su tiempo duro (FR-009).
8. Proveedor: `provider_unavailable`, `rate_limited`, `timeout` o `conflict`, con la traducción de
   [telegram.md](telegram.md#errores).

Con Hermes sin estar listo, `unavailable` antes de todo lo demás. Con su base caída, la autenticación comprueba el
certificado y el token, pero no puede guardar el `jti`: una llamada válida responde `unavailable` sin llegar al
proveedor, y una con credenciales que no valen, `auth`.

## `GET /v1/capabilities`

```json
{
  "status": "completed",
  "data": {
    "component": "hermes",
    "services": [
      {
        "category": "communication",
        "service": "chat",
        "contract": "v1",
        "providers": [
          {
            "provider": "telegram",
            "version": "1.0.0",
            "functions": [
              { "name": "send_message", "kind": "command", "mode": "sync", "timeout_s": 10 }
            ],
            "instances": [ { "name": "family_bot", "status": "healthy" } ]
          }
        ]
      }
    ]
  },
  "error": null,
  "meta": { "request_id": "req-2", "duration_ms": 3 }
}
```

Sin contactos ni direcciones. El estado de cada instancia se calcula como dice el
[modelo de datos](../data-model.md#estado-de-una-instancia) (FR-013).

## `POST /v1/instances/{name}/test`

Prueba la conexión de una instancia con su proveedor (en Telegram, `getMe`) y actualiza su estado (FR-014). No cambia
nada fuera de Hermes, así que no lleva `Idempotency-Key`.

- **Respuesta:** `data` con `instance`, `status` (el estado tras la prueba) y `checked_at`.
- **Errores:** `not_found` (404) si la instancia no existe; un fallo del proveedor va dentro del sobre (200), con su
  código, y deja la instancia en `unavailable`.

## Salud

No está en la API de red: va por el canal local de administración ([admin-channel.md](admin-channel.md)), como la de
Hécate, porque ninguna API interna acepta llamadas sin autenticar (principio V).
