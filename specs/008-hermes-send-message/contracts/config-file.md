# Fichero de configuración

El fichero que el admin aplica con `hermes config apply` y que `hermes config export` devuelve. Describe la
configuración entera de Hermes: aplicarlo crea, cambia y borra hasta que la base coincide con él (FR-024). La base es la
única fuente; el fichero solo es la forma de escribirla.

- **Nunca va a un repositorio:** lleva nombres de contactos y valores de variables (principio V).
- **No lleva secretos:** cada campo de secreto es una referencia `secret://` que resuelve Hécate (FR-025).
- **No lleva direcciones:** la de cada canal la pone la verificación (FR-022).

## Ejemplo

```yaml
version: 1

variables:
  case_base_url: https://tenaro.cerbero.home/cases

instances:
  - name: family_bot
    provider: telegram
    secrets:
      bot_token: secret://hermes/telegram/family_bot/bot_token
    limits:
      requests_per_minute: 20

templates:
  - id: device_unknown
    versions:
      - version: 1
        data_schema:
          type: object
          required: [mac, ip, case_id]
          additionalProperties: false
          properties:
            mac: { type: string }
            ip: { type: string }
            case_id: { type: string }
        message:
          title: Dispositivo desconocido en la red
          severity: high
          summary: "Se ha conectado un equipo que no está en la lista: ${data.mac}."
          facts:
            - { label: MAC, value: "${data.mac}" }
            - { label: IP, value: "${data.ip}" }
          expected: Aprobar o denegar el bloqueo
          link: "${vars.case_base_url}/${data.case_id}"

contacts:
  - name: contacto_demo
    channels:
      - { service: chat, provider: telegram, instance: family_bot }
```

## Campos

| Campo | Regla |
|---|---|
| `version` | Obligatorio: `1` |
| `variables` | Opcional: un mapa de nombre a texto (hasta 4096 caracteres) |
| `instances[]` | `name`, `provider`, `params` (opcional, sin secretos), `secrets` (los campos de secreto del manifiesto, cada uno una referencia `secret://`) y `limits.requests_per_minute` (opcional, de 1 a 600) |
| `templates[]` | `id` y `versions` (al menos una) |
| `templates[].versions[]` | `version` (entero desde 1, sin repetir), `data_schema` (un JSON Schema, Draft 2020-12) y `message` (los textos del [mensaje neutral](../data-model.md#textos-de-una-versión-de-plantilla)) |
| `contacts[]` | `name` y `channels` (al menos uno) |
| `contacts[].channels[]` | `service` (`chat`), `provider` y `instance`; uno por contacto e instancia |

- Los nombres siguen `^[a-z][a-z0-9_]{0,62}$` y no se repiten dentro de su lista.
- Un campo que no está en esta tabla se rechaza.

## Reglas de lectura

- YAML en UTF-8, de 1 MiB como mucho.
- Sin alias ni anclas, y sin claves repetidas.
- Sin conversiones implícitas: un texto que parece un número o un booleano tiene que ir entre comillas si el campo es de
  texto, y un número entre comillas en un campo numérico se rechaza.

## Validación al aplicar

Todo se comprueba antes de escribir nada, y cualquier fallo deja la base como estaba:

1. el formato y los campos de arriba;
2. que el proveedor de cada instancia existe y que `fake` solo aparece con `HERMES_FAKE_PROVIDERS=enabled`;
3. que cada instancia tiene los campos de secreto que pide el manifiesto de su proveedor, y nada más, cada uno con la
   referencia `secret://hermes/<proveedor>/<instancia>/<campo>` de esa instancia (FR-019), y que `params` solo lleva
   los campos no secretos que declara el manifiesto;
4. que el `data_schema` de cada versión es un JSON Schema válido;
5. que cada `${...}` de los textos es `${data.<ruta>}` o `${vars.<nombre>}` de una variable que existe;
6. que el texto fijo de `severity` y `link`, si no lleva `${...}`, ya cumple su regla;
7. que cada canal usa una instancia que existe y del mismo proveedor;
8. que una versión de plantilla que ya existe no cambia su contenido: para cambiarla, se añade otra;
9. que cada referencia `secret://` nueva o cambiada se resuelve en Hécate; si Hécate no responde, no se aplica nada.

## Lo que hace la aplicación

- Crea, cambia y borra variables, instancias, versiones de plantilla, contactos y canales, en una sola transacción.
- Un canal que no cambia conserva su dirección y su verificación. Uno que se borra, o que cambia de instancia (se borra
  y se crea otro), las pierde, y la CLI lo dice en el cambio.
- Una instancia no se puede borrar si un canal que se queda la usa: la validación lo rechaza.
- Al terminar, Hermes carga la configuración nueva de una vez, y deja una entrada de auditoría con los cambios.

## Exportar

`hermes config export` escribe el mismo formato, con las listas ordenadas por nombre y las versiones por número, las
referencias de los secretos y sin direcciones, códigos ni estado de verificación. Aplicar lo exportado no cambia nada.
