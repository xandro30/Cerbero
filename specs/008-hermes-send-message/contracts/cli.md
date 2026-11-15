# CLI `hermes`

La CLI que el admin usa en el equipo de Hermes. `hermes serve` ejecuta el servicio; las demás órdenes le hablan por el
canal local ([admin-channel.md](admin-channel.md)), así que necesitan a Hermes en marcha y la variable
`HERMES_ADMIN_DIR`.

## Códigos de salida

| Código | Cuándo |
|---|---|
| 0 | La orden se hizo |
| 1 | La operación falló (el sobre dice `failed`), se canceló, o la CLI no llega al socket: `error: cannot reach hermes at <socket>: <motivo>` |
| 2 | Uso incorrecto (los mensajes de argparse) o una variable de entorno que no vale: `error: <mensaje>` |

Con `failed`, la CLI escribe `error: <código>: <mensaje>` y, si los hay, un `error: <ruta>: <mensaje>` por cada fallo de
validación.

## Órdenes

| Orden | Qué hace | Salida |
|---|---|---|
| `hermes serve` | Ejecuta Hermes en primer plano hasta SIGINT o SIGTERM ([research R1](../research.md#r1-proceso-y-arranque)) | Los logs, una línea JSON por evento |
| `hermes enroll` | Pide el token de alta sin eco si la entrada es un terminal, o lo lee de la entrada estándar, y da de alta a Hermes en Hécate | `enrolled <spiffe id> serial <serie> until <fecha>` |
| `hermes config apply <fichero> [--yes]` | Lee el fichero, pide el plan y enseña los cambios; con `yes` en la pregunta, o con `--yes`, aplica | Un cambio por línea, `+` crear, `~` cambiar y `-` borrar (p. ej. `+ instance family_bot`, `- channel contacto_demo telegram/family_bot (loses its verification)`), y después `apply these <n> changes? [yes/no]` y `applied <n> changes`; sin cambios, `no changes` |
| `hermes config export [--out <fichero>]` | Escribe la configuración vigente en el formato del fichero | El YAML en la salida estándar, o `exported to <fichero>` |
| `hermes contacts code <contacto> --instance <instancia>` | Da un código de un solo uso para el canal del contacto en esa instancia, que queda sin verificar hasta que llegue | `code <XXXX-XXXX> for <contacto> on <instancia> expires <fecha>` y `send it to the bot from the person's Telegram account` |
| `hermes health [--live]` | Consulta la salud por el canal local y sale con 0 si Hermes está listo y con 1 si no; con `--live`, sale con 0 si el proceso responde, y es lo que usa el healthcheck del contenedor | `ready` o `not ready: <motivo>`; con `--live`, `live` |

- `config apply` responde `cancelled` y sale con 1 si la respuesta no es `yes` o se acaba la entrada. Un fichero que no
  se puede leer da `error: cannot read <fichero>: <motivo>` y sale con 1.
- `config export --out` no sobrescribe: si el fichero existe, `<fichero> already exists; it is not overwritten` y 1. El
  fichero se crea con permisos 0600.
- Las fechas van en ISO 8601 con zona (`2026-11-14T10:30:00Z`).

## Variables de entorno

| Variable | Por defecto | Para qué |
|---|---|---|
| `HERMES_STATE_DIR` | Obligatoria en `serve` | La identidad de Hermes (R4) |
| `HERMES_PROXY_DIR` | Obligatoria en `serve` | El certificado y la clave que lee Envoy |
| `HERMES_RUN_DIR` | `/run/hermes` | El socket de la API |
| `HERMES_ADMIN_DIR` | `/run/hermes-admin` | El socket del canal local; lo usan todas las órdenes |
| `HERMES_HECATE_URL` | Obligatoria en `serve` | La API de Hécate, p. ej. `https://hecate:8443` |
| `HERMES_DATABASE_URL` | Obligatoria en `serve` | La base, sin contraseña: `postgresql+psycopg://hermes@postgres:5432/hermes` |
| `HERMES_DATABASE_PASSWORD_REF` | `secret://hermes/postgres/password` | La referencia de la contraseña de la base en Hécate, que es un secreto de Hermes (`secret://hermes/...`) |
| `HERMES_VERIFICATION_CODE_MINUTES` | `30` | Lo que vale un código, de 5 a 1440 |
| `HERMES_FAKE_PROVIDERS` | `disabled` | `enabled` permite el proveedor `fake` (pruebas) |
| `HERMES_FAKE_DIR` | Ninguno | Con el fake activado, el directorio de sus bandejas ([fake.md](fake.md)); sin ella, el fake guarda todo en memoria |
| `HERMES_LOG_LEVEL` | `info` | `debug`, `info`, `warning` o `error` |

Ninguna lleva secretos: la contraseña de la base es una referencia `secret://` (constitución, Operación). Un valor que
no vale para la orden hace que salga con 2 antes de hacer nada.
