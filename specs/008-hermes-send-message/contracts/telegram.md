# Librería de Telegram

La librería del proveedor `telegram` para el tipo de servicio `chat`. Implementa `send_message`, la prueba de conexión y
la recogida de los mensajes que llegan al bot para verificar canales.

## Manifiesto

`hermes/src/hermes/infrastructure/providers/telegram/manifest.json` (FR-026):

```json
{
  "provider": "telegram",
  "version": "1.0.0",
  "implements": [
    {
      "service": "chat",
      "contract": "1",
      "functions": {
        "send_message": { "kind": "command", "mode": "sync", "timeout": "10s" }
      }
    }
  ],
  "capabilities": { "max_length": 4096, "formats": ["text", "html"] },
  "limits": { "requests_per_minute": 30 },
  "permissions": [
    "Recibir los mensajes privados que se envían al bot (getUpdates)",
    "Enviar mensajes a los chats privados que han escrito al bot (sendMessage)"
  ],
  "instance_config": {
    "bot_token": { "type": "secret", "required": true }
  }
}
```

El token del bot vive en Hécate, en `secret://hermes/telegram/<instancia>/bot_token`.

## Uso de la Bot API

`https://api.telegram.org/bot<token>/<método>`, con httpx y peticiones JSON.

| Método | Para qué | Parámetros |
|---|---|---|
| `getMe` | La prueba de conexión: las credenciales funcionan | — |
| `sendMessage` | Enviar el mensaje dibujado | `chat_id` (la dirección del canal), `text`, `parse_mode: "HTML"` y `link_preview_options: {"is_disabled": true}` |
| `getUpdates` | El sondeo de los mensajes que llegan al bot | `offset` (el último `update_id` más uno), `timeout: 25` y `allowed_updates: ["message"]` |

- **El token va en la ruta.** Ninguna URL de Telegram, ni las excepciones de httpx que la contienen, llega a los logs ni
  a una respuesta: el adaptador traduce cada fallo a un error propio, sin URL.
- **Sondeo:** uno por instancia, en bucle; el `offset` vive en memoria. Al reiniciar, Telegram devuelve lo que no se
  confirmó en las últimas 24 horas.

## Dibujo del mensaje

El mensaje neutral pasa a HTML, con cada texto escapado (`&`, `<` y `>`):

```text
<b>{title}</b>
Gravedad: {severity}

{summary}

• {label}: {value}          (un hecho por línea)
Hecho: {acción}             (una acción por línea)
Qué se espera: {expected}
{link}
```

Las partes vacías no se escriben. Si el texto pasa de 4096 caracteres, se quitan hechos desde el último, después
acciones desde la última, y después se acorta el resumen con «…», hasta que cabe; el título y el enlace no se tocan
(FR-018), y la respuesta dice `truncated: true`.

## Verificación de canales

- El sondeo mira los mensajes de texto de los chats privados (`chat.type` igual a `private`). Un texto que coincide con
  un código pendiente y no caducado de un canal de esa instancia, comparado sin mayúsculas, espacios ni guiones,
  verifica el canal: su dirección pasa a ser `chat.id` ([research R12](../research.md#r12-verificación-de-canales)).
- Tras verificar, Hermes envía a ese chat el texto «Canal verificado.».
- Un código caducado, usado o desconocido, y cualquier otro mensaje, no reciben respuesta ni se guardan. Los mensajes de
  grupos se ignoran.

## Errores

| Situación | Código | `retryable` | Estado de la instancia |
|---|---|---|---|
| No conecta (`ConnectError`, `ConnectTimeout`, `PoolTimeout`) y se agota el timeout técnico reintentando | `provider_unavailable` | Sí | Cuenta como fallo |
| 429 con un `retry_after` que cabe en el timeout técnico | Reintenta tras esperarlo | — | — |
| 429 con un `retry_after` que no cabe | `rate_limited`, con `retry_after_s` | Sí | — |
| `ReadTimeout`, `WriteTimeout`, `RemoteProtocolError` o error de lectura tras enviar `sendMessage` | `timeout`, con `outcome_unknown: true` | No | Cuenta como fallo |
| 5xx a `sendMessage` | `timeout`, con `outcome_unknown: true` | No | Cuenta como fallo |
| 5xx o fallo de transporte en `getMe` | `provider_unavailable` | Sí | `unavailable` |
| 401 o 404: el token no vale | `provider_unavailable` | No | `unavailable` hasta que una prueba funcione |
| 403: la persona bloqueó al bot o no admite sus mensajes | `conflict` | No | — |
| 400: Telegram rechaza la petición (p. ej. el chat ya no existe) | `provider_unavailable` | No | Cuenta como fallo |

Los reintentos siguen [research R10](../research.md#r10-reintentos-timeouts-y-resultado-ambiguo). El mensaje de cada
error no lleva la URL, el token, el `chat_id` ni el texto que respondió Telegram.
