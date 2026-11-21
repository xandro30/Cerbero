# 0014 — Lo que guarda la clave de idempotencia de Hermes

Estado: aceptada · Fecha: 2026-11-19

## Contexto

Hermes guarda con la `Idempotency-Key` de quien llama la respuesta de un envío y la devuelve a toda repetición durante
24 horas, sin volver a enviar ([llamadas](../hermes/llamadas.md#la-idempotencia)). La clave guardaba toda respuesta que
llegó al proveedor, también un fallo, y eso chocaba con quien llama:

- Hades deriva la clave del paso y de su input (`response_id + paso + hash(input resuelto)`), así que un reintento del
  bloque, o relanzar el paso con el mismo input, lleva la misma clave
  ([Hades](../hades.md#relanzar-idempotencia-y-compensación)). Recibía el fallo guardado y no llegaba a enviar, aunque
  el proveedor ya respondiera.
- Tras arreglar la causa de un fallo que garantiza que no se envió nada (las credenciales rechazadas, la persona que no
  admite el bot o una petición rechazada por el proveedor), repetir con la misma clave recibía el mismo fallo durante
  24 horas.
- La guía de integración lo esquivaba con dos reglas: la misma clave para unos códigos y otra clave para reintentar un
  fallo del proveedor, una regla que el diseño de Hades no sigue.
- Un fallo inesperado de Hermes mientras llamaba al proveedor respondía `internal` y quedaba en la clave, aunque
  `internal` manda repetir con la misma clave.

La decisión estaba abierta en DECISIONS desde el cambio 010, y el propietario la tomó al especificar el cambio 012, el
2026-11-19. Las referencias:

- **SecOps SOAR** no tiene clave de idempotencia en sus acciones.
- **Stripe** guarda la respuesta de una petición que empezó a ejecutarse, también un 500, que pide tratar como
  indeterminado, y no la de una que no llegó a ejecutarse, que se repite tras arreglar la causa
  ([peticiones idempotentes](https://docs.stripe.com/api/idempotent_requests)).
- **El borrador de la IETF** de la cabecera `Idempotency-Key`
  ([datatracker](https://datatracker.ietf.org/doc/html/draft-ietf-httpapi-idempotency-key-header)) deja a cada
  servicio qué guarda la clave. Hermes se aparta de él en dos cosas, que recoge
  [DECISIONS](../../DECISIONS.md#la-clave-de-idempotencia-de-hermes-frente-a-stripe-y-al-borrador-de-la-ietf).

## Opciones

Qué guarda la clave:

| Opción | A favor | En contra |
|---|---|---|
| **Solo lo que se envió o pudo enviarse; un fallo sin envío, reintentable o no, la deja libre** | Una sola regla para quien llama, repetir con la misma clave; Hades reintenta y relanza como está diseñado | Hermes tiene que distinguir los fallos que garantizan que no salió nada, algo que el contrato de cada proveedor ya fija |
| Liberarla solo con los fallos reintentables | Cambia menos | Repetir tras arreglar la causa de un fallo no reintentable sigue recibiendo el fallo guardado |
| Que Hades cambie la clave en cada intento, sin tocar Hermes | Hermes no cambia | Rompe el diseño de Hades, donde el mismo input lleva la misma clave para que un reintento no duplique un efecto que ya ocurrió |

Qué responde un fallo inesperado de Hermes mientras llama al proveedor, cuando no sabe si el mensaje salió:

| Opción | A favor | En contra |
|---|---|---|
| **`timeout` con `outcome_unknown`, guardado como un resultado desconocido** | Es lo que el diseño de Hermes pide ante un resultado ambiguo; la repetición no reenvía y quien llama lo trata como cualquier `timeout` | El código no dice que el fallo fue de Hermes: lo dice su log |
| `internal` con la clave libre | El código dice que el fallo es de Hermes | Si el mensaje salió, repetir con la misma clave lo duplica |
| `internal` guardado, sin `retryable` | No duplica | Da dos significados a `internal`: repetir con la misma clave antes del proveedor y no repetir al enviar |

## Decisión

- **La clave guarda solo lo que se envió o pudo enviarse:** un envío completado o un `timeout` con `outcome_unknown`.
  Toda repetición con la misma clave y el mismo cuerpo recibe esa respuesta durante 24 horas, sin volver a enviar.
- **Un fallo que garantiza que no se envió nada la deja libre,** reintentable o no, como un fallo antes del proveedor:
  el proveedor no conecta, pide esperar más de lo que cabe, rechaza las credenciales o la petición, o la persona no
  admite el bot. Su entrada de auditoría se escribe en la misma transacción en que se libera la clave, y la petición
  siguiente con la misma clave es un intento nuevo.
- **Un fallo inesperado de Hermes mientras llama al proveedor** responde `timeout` con `outcome_unknown` y queda en la
  clave, porque Hermes no sabe si el mensaje salió. `internal` queda para lo inesperado antes del proveedor, que no
  deja clave.
- **Quien llama tiene una sola regla,** repetir con la misma clave
  ([integración](../hermes/integracion.md#la-idempotency-key)). Con otro cuerpo, la clave responde `conflict` mientras
  guarda otra petición.

## Consecuencias

- Hades reintenta y relanza como está diseñado: un reintento tras un fallo sin envío llega al proveedor, y lo que pudo
  salir no se reenvía.
- Cada intento que llega al proveedor deja su entrada de auditoría, también el que libera la clave.
- Una petición con otro cuerpo tras un fallo sin envío es una petición nueva.
- La clasificación de los fallos no cambia: el contrato de cada proveedor ya obliga a marcar con `outcome_unknown` lo
  que pudo salir ([proveedores](../hermes/proveedores.md)).
- El contrato de Hermes describe la regla en `IdempotencyKey`, desde su versión 1.0.2.
- Las filas que guardaron un fallo sin envío antes de este cambio caducan con la limpieza, a las 24 horas, sin
  migrarlas.
