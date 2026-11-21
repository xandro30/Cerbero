# Contratos de Hermes

Qué promete Hermes a quien lo llama: la taxonomía de los servicios, las tres capas de datos, el sobre común, el contrato
de una función y `chat.send_message`. Es para quien quiere entender o revisar el diseño; cómo llama un componente está
en la [guía de integración](integracion.md), y cómo se atiende cada llamada, en [llamadas](llamadas.md).

## La taxonomía

Hermes ordena los servicios de terceros en cinco niveles. Un Playbook usa el contrato de un tipo de servicio, y el
proveedor que hay detrás no cambia nada para él:

```text
categoría (para qué sirve)          communication, enrichment, containment... (lista abierta)
└─ tipo de servicio (contrato común) chat, email, ticketing, threat_intel, network_control...
   └─ función (operación)            chat.send_message, chat.ask, ticketing.create, threat_intel.ip_reputation...
      └─ proveedor (librería)        telegram, jira, virustotal, pihole...
         └─ instancia (configurada)   family_bot, admin_bot...
```

Ejemplos de tipos de servicio y funciones:

| Categoría | Tipo de servicio | Funciones (ejemplos) |
|---|---|---|
| `communication` | `chat` | `send_message`, `ask` |
| `communication` | `email` | `send`, `ask` |
| `communication` | `ticketing` | `create`, `comment`, `close`, `get`, `watch` |
| `enrichment` | `threat_intel` | `ip_reputation`, `domain_reputation`, `hash_reputation`, `url_scan` |
| `containment` | `network_control` | `block_device` / `unblock_device`, `block_ip` / `unblock_ip`, `block_domain` / `unblock_domain` |

- `reputation` es la opinión del proveedor sobre un indicador; no es el Verdict de Hades.
- Cada proveedor lo implementa una librería ([proveedores](proveedores.md)), y cada instancia es una librería
  configurada ([configuración](configuracion.md#las-instancias)).

## Las tres capas de datos

Todas las capas las define y mantiene Hermes, en la petición y en la respuesta:

```text
sobre común: status, data, error, meta, ids, idempotencia
└─ contrato del tipo de servicio (p. ej. chat): campos comunes del tipo
   └─ contrato del proveedor (p. ej. telegram): lo propio de ese proveedor
```

Nada en el formato nativo del proveedor llega al Playbook. Si hace falta un dato del proveedor, se añade al contrato.

## El sobre común

El contrato de la API se publica como OpenAPI 3.1 en `shared/contracts` (`hermes.v1.json`), con el sobre común de
`common.v1.json`, y cada respuesta, también las de error, lo cumple.

```json
{
  "status": "completed | failed",
  "data": { "...": "según el contrato" },
  "error": { "code": "provider_unavailable", "message": "...", "retryable": true },
  "meta": {
    "request_id": "...", "duration_ms": 120, "service": "chat", "provider": "telegram", "idempotency_key": "..."
  }
}
```

- **`meta`** lleva siempre `request_id` (el `X-Request-Id` de la petición, o uno nuevo) y `duration_ms`, y `service`,
  `provider` e `idempotency_key` cuando aplican.
- **`error`** lleva el código, un mensaje sin secretos, si vale reintentar (`retryable`) y, si hay que esperar,
  `retry_after_s`.
- **Es error** un fallo de infraestructura o una respuesta 4XX o 5XX del proveedor (y las 3XX problemáticas). Un
  resultado de negocio, como una reputación `unknown`, va en `data`.

Los códigos son cerrados y comunes a Cerbero:

| Código | Estado HTTP antes del proveedor | ¿Reintentar? | Cuándo |
|---|---|---|---|
| `validation` | 400 | No | La petición no cumple el contrato |
| `auth` | 401 | No | Las credenciales no valen o quien llama no tiene acceso |
| `not_found` | 404 | No | La plantilla, la versión, la instancia o el contacto no existen |
| `conflict` | 409 | No | Operación no permitida en el estado actual (un canal sin verificar, la persona que no admite mensajes del bot) o una clave de idempotencia que ya guarda otra petición |
| `rate_limited` | 429, con `Retry-After` | Sí | Se acabó el límite de uso de la instancia, o el proveedor pide esperar más de lo que cabe |
| `provider_unavailable` | Solo llega del proveedor | Sí, salvo si el proveedor rechaza la petición o las credenciales | El proveedor no responde o falla sin haber hecho nada, o rechaza la petición o las credenciales de la instancia |
| `timeout` | Solo llega del proveedor | No, con `outcome_unknown` | No se sabe si la petición se ejecutó en el proveedor, también tras un fallo inesperado de Hermes al llamarlo |
| `unavailable` | 503 | Sí | Hermes no puede atender ahora |
| `internal` | 500 | Sí | Un fallo inesperado antes de llegar al proveedor |

**El estado HTTP:** toda respuesta que llegó al proveedor, se completara o fallara, lleva 200, y su fallo va dentro del
sobre con su código: también un `rate_limited` del proveedor, que lleva su espera en `retry_after_s` y no en la
cabecera. Los fallos de antes del proveedor llevan el estado de su código. Qué hace quien llama con cada uno está en la
[guía de integración](integracion.md).

## El contrato de una función

Cada función declara qué es, en el contrato de su tipo de servicio y en la extensión `x-cerbero-function` de su
operación en el OpenAPI:

```json
{
  "block_url": {
    "kind": "command",
    "mode": "sync",
    "side_effect": "Bloquea la URL ${input.url} en el proxy",
    "compensation": "unblock_url",
    "input": "...",
    "output": "..."
  },
  "url_scan": {
    "kind": "query",
    "mode": "task",
    "discloses_data": "Publica la URL ${input.url} en VirusTotal, visible para su comunidad",
    "input": "...",
    "output": "..."
  }
}
```

- **`kind`:** consulta (`query`) o cambio (`command`). Hades valida con él el verbo del bloque.
- **`mode`:** el nativo del proveedor (`sync` o `task`), sin conversiones ([llamadas](llamadas.md)). Cambiar de
  proveedor puede cambiar el API Contract (campos o modo): es major.
- **`side_effect`** describe qué cambia, con la entrada; va con su `compensation` o con `non_reversible`.
- **`discloses_data`** describe qué datos salen a un tercero y dejan de ser privados. Hades avisa al registrar un
  Playbook que la use.
- El texto resuelto de las dos descripciones aparece en el aviso, en el timeline y en el registro de auditoría
  ([seguridad](seguridad.md#la-auditoría)).
- Cada función declara también su timeout técnico ([llamadas](llamadas.md#los-timeouts)).

## `chat.send_message`

Envía un mensaje a un contacto por un canal de chat verificado. En su `x-cerbero-function` es un `command` síncrono
con un timeout de 10 s, `non_reversible`, cuyo `side_effect` es el envío al contacto y cuyo `discloses_data` es que el
proveedor recibe el texto del mensaje y el chat del contacto.

- **Petición:** `recipients` (una lista con exactamente un contacto), `template` (`id` y `version` opcional, sin ella la
  última), `data` (que cumple el esquema de esa versión) e `instance` (opcional; sin ella, el único canal de chat del
  contacto). Las plantillas y los contactos están en [comunicación](comunicacion.md).
- **Respuesta:** `recipient`, `instance`, `provider_message_ref`, `sent_at` y `truncated` (si se recortó el detalle);
  nunca la dirección del contacto.
- **Orden de las comprobaciones:**
  1. sin estar listo, `unavailable`, antes de todo;
  2. la autenticación y el acceso (`auth`, o `unavailable` si Hécate no responde al buscar a quien llama);
  3. el cuerpo y la clave (`validation`);
  4. la clave de idempotencia (`conflict`, la misma respuesta o `timeout`);
  5. la plantilla, la versión, la instancia y el contacto (`not_found`, o `validation` si los datos no cumplen);
  6. el canal sin verificar (`conflict`);
  7. el límite de la instancia (`rate_limited`);
  8. el secreto pasado su tiempo duro (`internal`);
  9. el proveedor.

  Con su base caída, la autenticación comprueba el certificado, el token y el acceso pero no puede guardar el `jti`,
  así que una llamada válida responde `unavailable` sin llegar al proveedor, y una que no se autentica, `auth`.

## Formato y versionado

- **Formato:** los contratos, en JSON Schema; la API, publicada en OpenAPI 3.1, de la que se generan los clientes de
  Hades.
- **Versionado:** major en la ruta (`/chat/v1/...`, `/v1/capabilities`). Hermes sirve a la vez las majors en uso y no
  retira una mientras algo la use; las deprecadas avisan con el mismo patrón que el resto de Cerbero.
- **Fuera de la API:** la configuración, los códigos de verificación y la salud van por el canal local de
  administración, como en Hécate: ninguna API interna acepta llamadas sin autenticar ([CLI](cli.md)).

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [Llamadas](llamadas.md): cómo se atiende una llamada.
- [Comunicación](comunicacion.md): las plantillas y los contactos de `chat.send_message`.
- [Integrar un componente](integracion.md): el contrato desde el lado de quien llama.
