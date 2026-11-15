# Batería de conformidad del tipo `chat`

Los casos que pasa todo proveedor de `chat` contra el contrato común del tipo (principio IV, FR-031). La batería se
parametriza por proveedor: el `fake` la pasa con sus fallos simulados y Telegram con respuestas grabadas de la Bot API
(respx). Un proveedor nuevo añade su forma de simular cada caso y pasa los mismos.

| Caso | Situación | Resultado que exige el contrato |
|---|---|---|
| C1 | Envío a un canal verificado | `completed`, con `provider_message_ref`, y el proveedor recibe el texto dibujado |
| C2 | Mensaje más largo que el límite del proveedor | El proveedor recibe un texto que cabe, con el título y el enlace; `truncated: true` |
| C3 | Textos con los caracteres especiales del formato del proveedor | Llegan escapados y el formato no se rompe |
| C4 | No conecta en el primer intento y sí en el segundo | `completed`, con un solo mensaje entregado |
| C5 | No conecta en ningún intento | `provider_unavailable`, `retryable: true`, sin pasar del timeout técnico |
| C6 | El proveedor limita con una espera que cabe en el timeout | Reintenta tras la espera y `completed` |
| C7 | El proveedor limita con una espera que no cabe | `rate_limited`, con `retry_after_s` |
| C8 | Se corta después de enviar | `timeout`, `retryable: false`, `outcome_unknown: true`, sin reintento |
| C9 | El proveedor rechaza las credenciales | `provider_unavailable`, `retryable: false`, y la instancia pasa a `unavailable` |
| C10 | El destinatario no admite mensajes del bot | `conflict`, `retryable: false` |
| C11 | Prueba de conexión | `healthy` con credenciales que funcionan; `unavailable` si no |
| C12 | Recoger los mensajes que llegan | Cada mensaje privado se entrega una sola vez, con su chat, en el orden en que llegó |

Ningún caso deja en los logs ni en la respuesta la URL del proveedor, el token, la dirección del destinatario ni el
texto que respondió el proveedor.
