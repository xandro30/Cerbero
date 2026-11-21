# Contratos del cambio 012

El cambio toca un contrato entre componentes, la API de Hermes
([hermes.v1.json](../../../shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json)), y solo en sus descripciones
y su versión. Decisiones en [research R4 y R5](../research.md#r4-el-contrato).

## `hermes.v1.json` 1.0.2

Pasa de 1.0.1 a 1.0.2, una subida de parche: no cambian los esquemas, las operaciones, los parámetros obligatorios ni
los códigos. Los textos nuevos, en inglés como el resto del contrato:

### `info.description`

> Every operation needs the client certificate of a Cerbero component, forwarded by the TLS proxy, and a short call
> token in Authorization, as the integration guide of Hécate fixes, with aud equal to the SPIFFE ID of Hermes and, in
> its header, the kid of the caller's call signing key. A failure while sending, from the provider or with an unknown
> outcome, travels inside the envelope with HTTP 200; a failure before reaching the provider carries the HTTP status of
> its code. The health of Hermes is not in this API: it goes through its local administration channel.

### `components.securitySchemes.callToken.description`

> A short call token (EdDSA, 300 s at most, single-use jti) signed by the caller. Its header carries alg EdDSA, typ JWT
> and kid, the id of the call signing key that Hécate gave the caller with its certificate; a token without kid answers
> auth. Its iss is the SPIFFE ID of the certificate of the connection and its aud, the SPIFFE ID of Hermes. It may carry
> cerbero_audit: an object of up to 16 text keys (1 to 64 characters) with text values (up to 256), 2 KiB at most.

### `components.parameters.IdempotencyKey.description` (nueva)

> One per send, chosen by the caller and kept with its identity for 24 hours. The key keeps the response of what was
> sent or may have been sent: a completed send, or a timeout with outcome_unknown. A repetition with the same key and
> body gives that response back without sending again, and, while the key is kept, the same key with another body is
> conflict. A failure that guarantees nothing was sent, like one before reaching the provider, keeps nothing: the next
> request with that key is a new attempt.

### Respuesta 200 de `POST /chat/v1/send_message`

> Sent, or the send failed once it reached the provider: the failure goes inside the envelope. A timeout with
> outcome_unknown, also after an unexpected failure while calling the provider, means the message may have arrived.

### Respuesta 409 de `POST /chat/v1/send_message`

> The channel is not verified, or the Idempotency-Key is kept for another request.

### Respuesta 500 de `POST /chat/v1/send_message`

> An unexpected failure before reaching the provider, or a secret past its hard cache time.

Las respuestas 500 de `GET /v1/capabilities` y de `POST /v1/instances/{name}/test` no cambian: no envían nada.

## La regla de repetición

Lo que el contrato describe, código a código, y lo que repiten la guía de integración, el diseño de las llamadas, la
tabla de códigos y el diseño de Hades:

| Código | ¿Lo guarda la clave? | Qué hace quien llama |
|---|---|---|
| `completed` | Sí | Nada: una repetición devuelve lo mismo |
| `timeout` con `outcome_unknown` (también tras un fallo inesperado al enviar) | Sí | No reenviar sin más: con la misma clave recibe lo mismo, y con otra puede duplicar el mensaje |
| `rate_limited` del proveedor (200) | No | Esperar `retry_after_s` y repetir con la misma clave |
| `provider_unavailable` con `retryable` | No | Reintentar más tarde con la misma clave |
| `provider_unavailable` sin `retryable` | No | Avisar al admin y, arreglada la causa, repetir con la misma clave |
| `conflict` del proveedor (200) | No | Avisar al admin y, cuando la persona vuelva a admitir el bot, repetir con la misma clave |
| Los fallos antes del proveedor | No | Como hoy: corregir la petición, o repetir con la misma clave si el código dice `retryable` |

## Lo que no cambia

- **El sobre común** (`common.v1.json` 1.1.0): los mismos campos y códigos cerrados.
- **La API de Hécate** (`hecate.v1.json`) y su guía de integración, que describe la cabecera que exige Hécate; Hécate no
  rechaza un `kid` en ella, así que quien llama usa la misma con los dos.
- **La autenticación de Hermes:** sigue eligiendo la clave por el `kid` y respondiendo `auth` sin él; el contrato solo
  lo describe.
- **El canal local y la CLI de Hermes:** sus órdenes, salidas y códigos de salida.
- **Las pruebas de contrato** siguen validando las respuestas contra los esquemas; ganan la comprobación de la versión
  1.0.2 y de las descripciones del `kid`, de la repetición y de la 409 (research R4).
