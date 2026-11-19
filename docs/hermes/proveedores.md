# Proveedores de Hermes

Cómo se añade un proveedor a Hermes: qué es una librería, de qué partes consta, su manifiesto y su versión, el polling,
la batería de conformidad de su tipo de servicio, el proveedor fake y la librería de Telegram. Es para quien desarrolla
una librería; el contrato que implementa está en [contratos](contratos.md).

## Qué es una librería

Una librería implementa uno o varios tipos de servicio para un proveedor: traduce el contrato del tipo de servicio a la
API del proveedor y su respuesta de vuelta al contrato. Cada instancia de la configuración es una librería configurada
([las instancias](configuracion.md#las-instancias)).

- **Las oficiales** son código de Hermes en el repositorio de Cerbero: se añaden por PR con CI, siguen las convenciones
  de calidad y de API Contracts, pasan la batería de conformidad de su tipo de servicio y se despliegan con Hermes.
- **Las propias** viven en el repositorio privado (ruta `hermes/`) y llegan por una PR desde Ténaro que pasa su batería
  de pruebas.
- **Su versión** es propia, separada del contrato: adaptarse a un cambio de la API del proveedor es minor o fix y no
  afecta a Hades. Un cambio del contrato del tipo de servicio es otra cosa: cambia el API Contract
  ([el versionado](contratos.md#formato-y-versionado)).

## Sus partes

```text
infrastructure/providers/telegram/
├── manifest.json
├── adapter.py     # las llamadas de salida y la prueba de conexión
├── rendering.py   # el mensaje neutral en el formato del proveedor
├── errors.py      # la traducción de cada respuesta del proveedor
└── polling.py     # lo que llega al bot
```

- **El adapter** implementa el port de su tipo de servicio. En `chat`, `ChatProvider`
  (`domain/services/chat/provider.py`): dibujar el mensaje neutral (`render`), enviarlo (`send`), dar lo que llega al
  bot de una instancia, cada mensaje una vez y en orden (`receive`), y probar la conexión de una instancia
  (`check_connection`).
- **El dibujo** usa el de su tipo de servicio, que reparte el mensaje en bloques y recorta el detalle si no cabe
  ([el mensaje neutral](comunicacion.md#el-mensaje-neutral)); la librería solo pone su formato (en Telegram, el título
  en negrita y cada texto escapado).
- **Los reintentos** usan los comunes de Hermes: solo lo que es seguro, con esperas exponenciales y jitter completo, sin
  pasar del timeout de la función ([los reintentos seguros](llamadas.md#los-reintentos-seguros)).
- **Sus fallos** son errores del contrato, con su código y sin la URL del proveedor, un secreto, una dirección ni el
  texto que respondió el proveedor, y van dentro del sobre ([el sobre común](contratos.md#el-sobre-común)).
- **El registro** de Hermes (`infrastructure/providers/registry.py`) carga cada librería con su manifiesto.
- Una librería que es poco más que su adapter, como el fake, no necesita más ficheros.

## El manifiesto

Cada librería declara en `manifest.json` lo que implementa, sus capacidades, sus límites, los permisos que necesita en
el proveedor y lo que lleva cada instancia. Hermes lo lee y lo valida al cargar la librería. El de Telegram:

```json
{
  "provider": "telegram",
  "version": "1.0.0",
  "implements": [
    { "service": "chat", "contract": "1",
      "functions": { "send_message": { "kind": "command", "mode": "sync", "timeout": "10s" } } }
  ],
  "capabilities": { "max_length": 4096, "formats": ["text", "html"] },
  "limits": { "requests_per_minute": 30 },
  "permissions": ["Recibir los mensajes privados que se envían al bot (getUpdates)",
                  "Enviar mensajes a los chats privados que han escrito al bot (sendMessage)"],
  "instance_config": { "bot_token": { "type": "secret", "required": true } }
}
```

| Campo | Qué dice |
|---|---|
| `provider` | El nombre del proveedor, el que nombran sus instancias en la configuración, con la regla de los nombres (`^[a-z][a-z0-9_]{0,62}$`) |
| `version` | La versión de la librería, `<major>.<minor>.<fix>` |
| `implements` | Por tipo de servicio: `service`, la versión de su contrato (`contract`) y sus `functions`, cada una con `kind` (`query` o `command`), `mode` (el nativo, `sync` o `task`) y su timeout técnico (`timeout`, en segundos: `10s`) |
| `capabilities` | `max_length`, lo que cabe en un mensaje, y `formats`, los formatos que dibuja |
| `limits` | `requests_per_minute`: el límite de uso por defecto de cada instancia, que la configuración ajusta de 1 a 600 |
| `permissions` | Los permisos mínimos que necesita en el proveedor, cada uno descrito con lo que hace |
| `instance_config` | Lo que lleva cada instancia: cada campo con su `type`, `secret` (una referencia `secret://` en la configuración) o `param` (un parámetro), y si es obligatorio (`required`) |

- `/v1/capabilities` sale de los manifiestos: los proveedores con su versión, sus funciones y sus instancias
  ([integración](integracion.md#las-capacidades-y-la-prueba-de-conexión)).
- La configuración de una instancia se valida contra su `instance_config`: los campos obligatorios, ninguno que no
  conozca y cada secreto con la referencia de su instancia ([la validación](configuracion.md#la-validación)).

## El polling

Todo lo que llega del proveedor se obtiene por polling, uno por instancia, sin webhooks: Hermes no abre nada al
exterior. En `chat`, el polling recoge los mensajes que llegan al bot, con los que se verifican los canales
([la verificación](comunicacion.md#la-verificación-de-un-canal)).

## La batería de conformidad

**Conformidad por tipo de servicio:** todo proveedor de un tipo de servicio pasa la misma batería contra el contrato
común del tipo (`hermes/tests/contract/test_conformance_chat.py` en `chat`). La batería se parametriza por proveedor:
el fake la pasa con sus fallos simulados y Telegram con respuestas grabadas de la Bot API. Una librería nueva añade su
forma de simular cada caso y pasa los mismos.

| Caso | Situación | Lo que exige el contrato |
|---|---|---|
| C1 | Envío a un canal verificado | `completed`, con `provider_message_ref`, y el proveedor recibe el texto dibujado |
| C2 | Un mensaje más largo que el límite del proveedor | Un texto que cabe, con el título y el enlace, y `truncated: true` |
| C3 | Textos con los caracteres especiales del formato del proveedor | Llegan escapados, sin romper el formato |
| C4 | No conecta en el primer intento y sí en el segundo | `completed`, con un solo mensaje entregado |
| C5 | No conecta en ningún intento | `provider_unavailable` con `retryable: true`, sin pasar del timeout técnico |
| C6 | El proveedor limita con una espera que cabe en el timeout | Reintenta tras la espera y `completed` |
| C7 | El proveedor limita con una espera que no cabe | `rate_limited`, con `retry_after_s` |
| C8 | Se corta después de enviar | `timeout`, `retryable: false` y `outcome_unknown: true`, sin reintento |
| C9 | El proveedor rechaza las credenciales | `provider_unavailable` con `retryable: false`, y la instancia pasa a `unavailable` |
| C10 | El destinatario no admite mensajes del bot | `conflict`, con `retryable: false` |
| C11 | La prueba de conexión | `healthy` con credenciales que funcionan, y `unavailable` sin ellas |
| C12 | Los mensajes que llegan | Cada mensaje privado se entrega una sola vez, con su chat y en el orden en que llegó |

Ningún caso deja en los logs ni en la respuesta la URL del proveedor, el token, la dirección del destinatario ni el
texto que respondió el proveedor, y una comprobación de fugas lo busca en cada uno.

## El proveedor fake

Un proveedor `fake` por tipo de servicio, para las pruebas: las de Hermes, su E2E, los entornos funcionales completos y
la CI de otros componentes, que así prueban sin tocar los servicios reales.

- **Cuándo se carga:** solo con `HERMES_FAKE_PROVIDERS=enabled`. Sin ella, una configuración que lo usa se rechaza al
  aplicarla y al arrancar, así que nunca llega a producción.
- **Lo que hace:** guarda lo que envía, simula cada fallo de la batería (no conectar, un límite, un corte tras enviar,
  credenciales rechazadas y un destinatario que no admite el bot) y reintenta como Telegram.
- **Sus bandejas:** con `HERMES_FAKE_DIR`, deja lo que envía y recoge lo que le llega en ese directorio, para que una
  prueba los lea y los escriba desde fuera de Hermes; sin ella, todo queda en memoria.

| Fichero | Quién lo escribe | Formato |
|---|---|---|
| `<dir>/<instancia>/outbox.jsonl` | Hermes, una línea por envío | `{"at": "<fecha ISO 8601>", "chat": "<dirección>", "text": "<texto dibujado>", "provider_message_ref": "<referencia>"}` |
| `<dir>/<instancia>/inbox/<nombre>.json` | La prueba, un fichero por mensaje que llega | `{"chat": "<dirección>", "chat_type": "private", "text": "<texto>"}` |

- Su polling lee la bandeja de entrada por orden de nombre, entrega cada mensaje como lo haría Telegram y borra su
  fichero, así que cada mensaje se entrega una sola vez.
- Su manifiesto: `send_message` de `chat`, síncrona, con un timeout de 10 s, un `max_length` de 4096, texto sin
  formato, un límite de 600 por minuto y ninguna credencial en sus instancias. Sus direcciones son inventadas, y su
  directorio, de pruebas: nunca va a un repositorio.

## La librería de Telegram

La librería del proveedor `telegram` para `chat`: implementa `send_message`, la prueba de conexión y la recogida de los
mensajes que llegan al bot. El token de cada bot vive en Hécate, en `secret://hermes/telegram/<instancia>/bot_token`.

| Método de la Bot API | Para qué | Parámetros |
|---|---|---|
| `getMe` | La prueba de conexión: las credenciales funcionan | — |
| `sendMessage` | Enviar el mensaje dibujado | `chat_id` (la dirección del canal), `text`, `parse_mode: "HTML"` y `link_preview_options: {"is_disabled": true}` |
| `getUpdates` | El polling de los mensajes que llegan al bot | `offset` (el último `update_id` más uno), `timeout: 25` y `allowed_updates: ["message"]` |

- **El token va en la ruta** (`https://api.telegram.org/bot<token>/<método>`), así que ninguna URL de Telegram, ni las
  excepciones que la contienen, llega a los logs ni a una respuesta.
- **El polling**, uno por instancia, guarda su `offset` en memoria; al reiniciar, Telegram devuelve lo que no se
  confirmó en las últimas 24 horas. Solo mira los mensajes de texto de los chats privados: los de los grupos se
  ignoran.
- **El límite:** 30 mensajes por minuto por instancia, por defecto.

**El dibujo:** el mensaje neutral pasa a HTML, con cada texto escapado (`&`, `<` y `>`); las partes vacías no se
escriben. Telegram cuenta el texto que enseña, sin las etiquetas, hasta 4096 caracteres.

```text
<b>{title}</b>
Gravedad: {severity}

{summary}

• {label}: {value}          (un hecho por línea)
Hecho: {acción}             (una acción por línea)
Qué se espera: {expected}
{link}
```

**Sus errores:**

| Situación | Código | `retryable` | Estado de la instancia |
|---|---|---|---|
| No conecta y se agota el timeout técnico reintentando | `provider_unavailable` | Sí | Cuenta como fallo |
| 429 con un `retry_after` que cabe en el timeout técnico | Reintenta tras esperarlo | — | — |
| 429 con un `retry_after` que no cabe | `rate_limited`, con `retry_after_s` | Sí | — |
| Un corte o un error de lectura tras enviar `sendMessage` | `timeout`, con `outcome_unknown: true` | No | Cuenta como fallo |
| 5xx a `sendMessage` | `timeout`, con `outcome_unknown: true` | No | Cuenta como fallo |
| 5xx o un fallo de transporte en `getMe` | `provider_unavailable` | Sí | `unavailable` |
| 401 o 404: el token no vale | `provider_unavailable` | No | `unavailable` hasta que una prueba funcione |
| 403: la persona bloqueó al bot o no admite sus mensajes | `conflict` | No | — |
| 400: Telegram rechaza la petición, p. ej. porque el chat ya no existe | `provider_unavailable` | No | Cuenta como fallo |

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [Contratos](contratos.md): el contrato que implementa cada librería.
- [Comunicación](comunicacion.md): el mensaje neutral que dibuja cada proveedor.
- [Configuración](configuracion.md): las instancias de cada librería.
- [Telegram Bot API](https://core.telegram.org/bots/api)
