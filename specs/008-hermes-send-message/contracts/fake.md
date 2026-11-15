# Proveedor fake

El proveedor `fake` del tipo `chat` es para las pruebas: las de Hermes, su E2E y la CI de otros componentes. Nunca se
despliega en producción: solo se carga con `HERMES_FAKE_PROVIDERS=enabled`, y sin ella una configuración que lo use se
rechaza al aplicarla y al arrancar ([research R17](../research.md#r17-proveedor-fake-y-batería-de-conformidad)).

## Manifiesto

`hermes/src/hermes/infrastructure/providers/fake/manifest.json`: el proveedor `fake`, con `send_message` del tipo `chat`
síncrona y un timeout de 10 s, `max_length` de 4096 y sin campos de secreto en sus instancias.

## Bandejas

Con `HERMES_FAKE_DIR`, el fake deja lo que envía y recoge lo que le llega en ese directorio, para que una prueba lo lea
y lo escriba desde fuera del proceso de Hermes. Sin la variable, todo queda en memoria.

| Fichero | Quién lo escribe | Formato |
|---|---|---|
| `<dir>/<instancia>/outbox.jsonl` | Hermes, una línea por envío | `{"at": "<fecha ISO 8601>", "chat": "<dirección>", "text": "<texto dibujado>", "provider_message_ref": "<referencia>"}` |
| `<dir>/<instancia>/inbox/<nombre>.json` | La prueba, un fichero por mensaje entrante | `{"chat": "<dirección>", "chat_type": "private", "text": "<texto>"}` |

- El sondeo del fake lee la bandeja de entrada por orden de nombre, entrega cada mensaje como lo haría Telegram y borra
  su fichero, así que cada mensaje se entrega una sola vez (caso C12 de la [batería](conformance.md)).
- Los fallos que simula para la batería se piden dentro del proceso; las bandejas no los cambian.
- Las direcciones del fake son inventadas, y el directorio es de pruebas: nunca va a un repositorio.
