# La configuración de Hermes

Qué configura el admin y cómo: la configuración en la base de Hermes, su fichero, la lectura y la validación, el plan,
la aplicación, la exportación y las instancias. Es para quien quiere entender o revisar el diseño; las órdenes están en
la [CLI](cli.md), y los pasos de cada situación, en [operación](operacion.md).

## Dónde vive

- **Qué es:** las instancias, las plantillas, las variables y los contactos ([comunicación](comunicacion.md)), en la
  base de Hermes, en el PostgreSQL común ([ADR 0012](../adr/0012-persistencia-postgresql-comun.md)).
- **Cómo cambia:** el admin la escribe entera en un fichero YAML y la aplica con la CLI de Hermes, en el equipo de
  Hermes, por el canal local. No hay cambios sueltos: cada aplicación lleva la configuración entera.
- **El fichero nunca va a un repositorio:** lleva contactos y valores de variables, aunque ni secretos (solo
  referencias) ni direcciones (las pone la verificación).
- **Cada petición** toma la configuración del momento en que empieza: la de antes o la de después de una aplicación,
  nunca una mezcla.

## El fichero

```yaml
version: 1

variables:
  console_url: https://cerbero.home

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
          properties:
            mac: { type: string }
            ip: { type: string }
            case_id: { type: string }
        message:
          title: Dispositivo desconocido en la red
          severity: high
          summary: "Ha aparecido ${data.mac} con la IP ${data.ip}."
          facts:
            - { label: MAC, value: "${data.mac}" }
          actions_done: [Bloqueado mientras decides]
          expected: Acepta o bloquea el dispositivo
          link: "${vars.console_url}/cases/${data.case_id}"

contacts:
  - name: propietario
    channels:
      - { service: chat, provider: telegram, instance: family_bot }
```

| Campo | Qué lleva |
|---|---|
| `version` | `1` |
| `variables` | Cada variable con su valor de texto, de hasta 4096 caracteres |
| `instances` | `name`, `provider`, `params` (los de su librería), `secrets` (una referencia por campo secreto de su librería) y, si cambia el de su librería, `limits.requests_per_minute` |
| `templates` | `id` y sus `versions`, cada una con `version` (desde 1), `data_schema` (un JSON Schema, Draft 2020-12) y `message` (los textos del [mensaje neutral](comunicacion.md#el-mensaje-neutral)) |
| `contacts` | `name` y sus `channels`, cada uno con `service` (`chat`), `provider` e `instance`; como mucho un canal por instancia |

- **Los textos del mensaje:** el título, de 1 a 256 caracteres; el resumen, hasta 2048; hasta 20 hechos, con etiqueta y
  valor de hasta 256; hasta 10 acciones, de hasta 256; lo esperado, hasta 512; y el enlace, una URL `https`. Un texto
  con `${...}` se comprueba al enviar, ya sustituido.

## La lectura

- **Formato:** UTF-8, 1 MiB como mucho, YAML seguro, sin alias, anclas ni claves repetidas.
- **Solo valores de JSON:** texto, números finitos, booleanos, null, listas y mapas con claves de texto. Una fecha sin
  comillas sigue siendo texto: no hay conversiones implícitas.
- **Estricta:** sin campos desconocidos y con los tipos exactos (un número entre comillas no vale donde va un número).
- **Los nombres** de variables, instancias, proveedores, plantillas y contactos siguen `^[a-z][a-z0-9_]{0,62}$`. No se
  repiten las variables, las instancias, las plantillas, los contactos ni las versiones de una plantilla, ni un contacto
  tiene dos canales en la misma instancia; un proveedor sí se repite, en cada una de sus instancias.
- **Cada fallo** dice su ruta en el fichero (`instances[0].name`), nunca el valor, y llegan todos a la vez.

## La validación

Antes de escribir nada, Hermes comprueba:

1. que cada instancia es de un proveedor con librería en este Hermes (el `fake`, solo con
   `HERMES_FAKE_PROVIDERS=enabled`), con los parámetros y los secretos que pide su manifiesto y ninguno que no conozca
   ([proveedores](proveedores.md));
2. que cada referencia de secreto es la de su instancia, `secret://hermes/<proveedor>/<instancia>/<campo>`;
3. que cada `data_schema` es un JSON Schema válido y cada `${...}` de los textos es `${data.<ruta>}` o
   `${vars.<variable>}` con una variable que existe;
4. que cada canal va a una instancia que existe y es de su proveedor;
5. que una versión de plantilla que ya existe no cambia: para cambiarla se añade otra;
6. y que cada referencia de secreto nueva o cambiada se resuelve en Hécate, una a una, para que el fallo diga cuál. Si
   Hécate no responde, `unavailable`.

Un fallo dice su ruta en el fichero, sin valores, y no se aplica nada. Al arrancar, Hermes valida igual la
configuración de su base: una que usa el `fake` sin `HERMES_FAKE_PROVIDERS=enabled` también se rechaza ahí.

## El plan y la aplicación

- **El plan:** la CLI enseña un cambio por línea, con `+` para crear, `~` para cambiar y `-` para borrar, y los ordena
  así: borrar, cambiar, crear. Lo que depende de otra cosa se borra antes que ella y se crea después.
- **La confirmación:** la CLI la pide antes de aplicar, salvo con `--yes` ([CLI](cli.md)).
- **La aplicación:** Hermes vuelve a validar y aplica en una sola transacción, con un bloqueo, y solo si la base no
  cambió desde el plan; si cambió, `conflict`, y hay que pedir otro plan.
- **La auditoría:** la entrada de la aplicación, con sus cambios y `actor: admin`, va en la misma transacción. Si no se
  puede escribir, no se aplica nada ([seguridad](seguridad.md#la-auditoría)).
- **La verificación:** un canal que no cambia conserva su verificación; uno que se borra o cambia de instancia la
  pierde, y el plan lo dice ([comunicación](comunicacion.md#la-verificación-de-un-canal)).
- **Sin cambios:** un fichero igual a lo que hay no escribe nada, tampoco en la auditoría, así que aplicar dos veces el
  mismo fichero no cambia nada.
- **Después:** las peticiones nuevas toman la configuración aplicada, cada instancia nueva empieza su polling y las que
  se van lo paran.

## La exportación

`hermes config export` escribe la configuración vigente en el mismo formato, con las listas ordenadas por nombre y las
versiones por número, con las referencias de los secretos y sin direcciones, códigos ni estado de verificación.
Aplicarla no cambia nada.

## Las instancias

- **Una instancia** es una librería configurada: sus parámetros y las referencias a sus secretos, nunca el valor. Puede
  haber varias por proveedor (p. ej. un bot para la familia y otro para el administrador), y quien llama elige por
  nombre.
- **Sus secretos** son del ámbito de Hermes en Hécate, con la forma `secret://hermes/<proveedor>/<instancia>/<campo>`, y
  Hermes los resuelve al usarlos ([seguridad](seguridad.md#los-secretos)).
- **El límite de uso:** un cubo de fichas por instancia, en memoria, con tantas fichas como peticiones por minuto, que
  vuelven una a una a lo largo del minuto. Vale lo que diga su librería (30 por minuto en Telegram) y se ajusta en la
  instancia de 1 a 600 por minuto. Sin fichas, un envío responde `rate_limited` con los segundos hasta la siguiente
  (HTTP 429 con `Retry-After`), sin llegar al proveedor.
- **La prueba de conexión** confirma que las credenciales funcionan, sin cambiar nada fuera. Hermes prueba cada
  instancia al arrancar, y un componente con acceso la pide con `POST /v1/instances/{name}/test`
  ([integración](integracion.md)). La última prueba cuenta en el estado de la instancia, que dan las capacidades.

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [Comunicación](comunicacion.md): los contactos, las plantillas y la verificación de los canales.
- [Proveedores](proveedores.md): el manifiesto con el que se valida cada instancia.
- [CLI y variables de entorno](cli.md): `hermes config apply` y `hermes config export`.
