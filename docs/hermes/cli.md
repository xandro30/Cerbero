# CLI y variables de entorno de Hermes

Cómo se lanza Hermes, qué necesita, qué variables de entorno admite y qué hace cada orden de la CLI `hermes`, con sus
argumentos, su salida, sus errores y su código de salida. Es para el admin, que opera Hermes desde su equipo; qué hacer
en cada situación está en [operación](operacion.md), y el fichero de la configuración, en
[configuración](configuracion.md).

## Lanzar Hermes

`hermes serve` ejecuta Hermes en primer plano hasta que recibe SIGINT o SIGTERM:

1. lee sus variables de entorno; con una que no vale, escribe `error: <mensaje>` y sale con 2 antes de hacer nada;
2. crea, si no existen, el directorio de `HERMES_RUN_DIR` y el de `HERMES_ADMIN_DIR`, este con permisos 0700;
3. abre el socket de la API, `api.sock` en `HERMES_RUN_DIR`, y el del canal local, `admin.sock` en
   `HERMES_ADMIN_DIR`, los dos con permisos 0600 y sin ningún puerto TCP; sustituye un socket que haya quedado de una
   ejecución anterior;
4. sigue los pasos del arranque: su identidad, las publicaciones de Hécate con su lista de acceso, la contraseña de su
   base, la conexión, las migraciones, la configuración, la prueba de cada instancia y el polling. La salud responde
   desde el principio, y Hermes no está listo hasta terminarlos; sin identidad, espera a `hermes enroll`.

Al parar, cierra los dos sockets, los borra y sale con 0. Escribe sus logs en la salida estándar, una línea JSON por
evento: cada petición a la API deja una línea `api.request`, y cada orden del canal local, la suya
([los logs](seguridad.md#los-logs)).

Si no puede preparar un socket, escribe `error: cannot prepare <ruta>: <motivo>` y sale con 1, y si uno de sus dos
servidores se para solo, `error: the <servidor> server stopped on its own`, también con 1. Hermes, su CLI y Envoy
corren con el mismo usuario; cómo se deja corriendo como servicio en el home lab es una decisión
[abierta](../../DECISIONS.md#abiertas).

## Lo que necesita

| Qué | Para qué |
|---|---|
| **El directorio de estado** (`HERMES_STATE_DIR`) | Su identidad: la clave de firma de llamadas, los metadatos, la renovación en curso, las últimas publicaciones de Hécate y la copia de su lista de acceso, en un directorio 0700 con ficheros 0600 |
| **El directorio del proxy** (`HERMES_PROXY_DIR`) | Su certificado, su cadena, su clave TLS y la CA raíz, en `gen-<n>/` con el enlace `current`, que lee Envoy; el directorio y cada `gen-<n>/`, 0700, con ficheros 0600 |
| **El socket de la API** (`api.sock`) | Las llamadas que reenvía Envoy, el único que lo alcanza |
| **El socket del canal local** (`admin.sock`) | Las órdenes de la CLI, en un directorio privado |
| **Rutas de socket cortas** | Cada socket, con su directorio, cabe en 100 bytes |
| **Envoy delante** | Termina el mTLS y le reenvía cada llamada con el certificado del cliente ([el proxy](seguridad.md#el-proxy)) |
| **Hécate**, en `HERMES_HECATE_URL` | Su identidad y sus renovaciones, las publicaciones, su lista de acceso y sus secretos: la contraseña de su base y los de las instancias |
| **Su base en el PostgreSQL común** (`HERMES_DATABASE_URL`) | Su configuración y lo que escribe al funcionar; Hermes la migra al arrancar |

- El canal local y su protocolo solo los usa esta CLI.
- Al arrancar sin Hécate o sin su base, Hermes abre sus sockets, pero no está listo hasta que responden; lo que pasa
  si caen con Hermes en marcha está en [ante fallos](../hermes.md#ante-fallos).

## Variables de entorno

| Variable | Por defecto | Efecto | Límites |
|---|---|---|---|
| `HERMES_STATE_DIR` | Ninguno: obligatoria en `serve` | El directorio de estado: la identidad de Hermes | Una ruta absoluta |
| `HERMES_PROXY_DIR` | Ninguno: obligatoria en `serve` | El directorio del proxy: el certificado y la clave que lee Envoy | Una ruta absoluta |
| `HERMES_RUN_DIR` | `/run/hermes` | El directorio del socket de la API, `api.sock` | Una ruta absoluta en la que `api.sock` quepa en 100 bytes |
| `HERMES_ADMIN_DIR` | `/run/hermes-admin` | El directorio del socket del canal local, `admin.sock`; lo usan todas las órdenes | Una ruta absoluta en la que `admin.sock` quepa en 100 bytes |
| `HERMES_HECATE_URL` | Ninguno: obligatoria en `serve` | La API de Hécate, p. ej. `https://hecate:8443` | `https://<host>:<puerto>`, con el puerto y sin ruta |
| `HERMES_DATABASE_URL` | Ninguno: obligatoria en `serve` | La base de Hermes, p. ej. `postgresql+psycopg://hermes@postgres:5432/hermes` | `postgresql+psycopg://<usuario>@<host>[:<puerto>]/<base>`, sin contraseña |
| `HERMES_DATABASE_PASSWORD_REF` | `secret://hermes/postgres/password` | La referencia de la contraseña de la base en Hécate | Una referencia de un secreto de Hermes, `secret://hermes/...` |
| `HERMES_VERIFICATION_CODE_MINUTES` | `30` | Lo que vale un código de verificación | Un número entero de minutos de 5 a 1440 |
| `HERMES_FAKE_PROVIDERS` | `disabled` | Con `enabled`, Hermes carga el proveedor `fake`, para las pruebas | `disabled` o `enabled` |
| `HERMES_FAKE_DIR` | Ninguno | Con el fake activado, el directorio de sus bandejas; sin ella, el fake lo guarda todo en memoria | Una ruta absoluta |
| `HERMES_LOG_LEVEL` | `info` | El nivel de los logs | `debug`, `info`, `warning` o `error` |

- Ninguna lleva un secreto: la contraseña de la base es una referencia.
- `hermes serve` las lee todas antes de hacer nada; las demás órdenes solo usan `HERMES_ADMIN_DIR`, y necesitan a
  Hermes en marcha.
- Con un valor que no vale, la orden se detiene antes de hacer nada, escribe `error: <mensaje>`, p. ej.
  `error: HERMES_VERIFICATION_CODE_MINUTES must be a whole number of minutes from 5 to 1440`, y sale con 2.

## Convenciones

- **Salida:** lo que sale bien va a la salida estándar, una línea por resultado; los errores van a la salida de error.
- **Errores de Hermes:** `error: <código>: <mensaje>`, con una línea `error: <ruta>: <mensaje>` por cada detalle, como
  cada fallo de un fichero de configuración con su ruta en él. Los códigos son los del
  [sobre común](contratos.md#el-sobre-común).
- **Errores locales:** `error: <mensaje>`, p. ej. `error: cannot reach hermes at <socket>: <motivo>` si Hermes no está
  en marcha o la CLI no llega a su socket, o `error: cannot read <fichero>: <motivo>`.
- **Códigos de salida:**

  | Código | Cuándo |
  |---|---|
  | `0` | La orden se hizo, también si no cambió nada |
  | `1` | La operación falló, se canceló o la CLI no llega al socket; con `hermes health`, Hermes no está listo |
  | `2` | Uso incorrecto (falta un argumento o una opción no existe) o una variable que no vale |

- **El token de alta, nunca en la línea de órdenes:** `hermes enroll` lo pide sin eco cuando la entrada es un terminal,
  o lee la primera línea de la entrada estándar. Una pregunta sin respuesta (Ctrl-C, el fin de la entrada o un token
  vacío) escribe `cancelled` y sale con 1.
- **Las fechas** van en UTC, con la forma `2026-11-16T18:40:00Z`.
- **El registro:** cada orden del canal local deja una línea en el log de Hermes, y las que cambian la configuración o
  piden un código dejan además su entrada de auditoría, con `actor: admin` ([la auditoría](seguridad.md#la-auditoría)).

## Las órdenes

Las cinco de primer nivel y sus seis órdenes finales. Los errores de cada fila son los propios de la orden; los de las
convenciones valen para todas.

| Orden | Qué hace y qué pide | Salida si va bien | Errores propios |
|---|---|---|---|
| `hermes serve` | Ejecuta Hermes en primer plano hasta SIGINT o SIGTERM ([lanzar Hermes](#lanzar-hermes)) | Sus logs | No puede preparar un socket, o uno de sus servidores se para solo (1) |
| `hermes enroll` | Da de alta a Hermes en Hécate con un token de alta, que pide sin eco o lee de la entrada estándar | `enrolled <spiffe id> serial <serie> until <fecha>` | `conflict` si Hermes ya tiene una identidad válida; `validation` si el token no tiene la forma de los de Hécate (`HET1....`); y los errores de Hécate: `auth` si rechaza el token (gastado, caducado o de otra CA), `rate_limited` si frena las altas y `unavailable` si no responde |
| `hermes config apply <fichero> [--yes]` | Lee el fichero, pide el plan y enseña los cambios; con `yes` en la pregunta, o con `--yes`, los aplica ([el plan y la aplicación](configuracion.md#el-plan-y-la-aplicación)) | Un cambio por línea, p. ej. `+ instance family_bot` o `- channel propietario telegram/family_bot (loses its verification)`; después `apply these <n> changes? [yes/no]` y `applied <n> changes`; sin cambios, `no changes` | El fichero no se puede leer o no es UTF-8 (1); `validation`, con un detalle por cada fallo del fichero; `conflict` si la configuración cambió desde el plan; `unavailable` si Hécate no puede comprobar los secretos o la base no responde; otra respuesta que `yes` a la pregunta, `cancelled` (1) |
| `hermes config export [--out <fichero>]` | Escribe la configuración vigente en el formato del fichero ([la exportación](configuracion.md#la-exportación)) | El YAML en la salida estándar; con `--out`, un fichero nuevo con permisos 0600 y `exported to <fichero>` | El fichero de `--out` ya existe, `<fichero> already exists; it is not overwritten`, o no se puede escribir (1) |
| `hermes contacts code <contacto> --instance <instancia>` | Da un código de un solo uso para el canal del contacto en esa instancia, que queda sin verificar y sin dirección hasta que llega ([la verificación](comunicacion.md#la-verificación-de-un-canal)) | `code <XXXX-XXXX> for <contacto> on <instancia> expires <fecha>` y `send it to the bot from the person's Telegram account` | `not_found` si el contacto no existe o no tiene canal de chat en esa instancia |
| `hermes health [--live]` | Dice si Hermes está listo; con `--live`, si el proceso responde, que es lo que usa el healthcheck del contenedor | `ready` (0) o `not ready: <motivo>` (1); con `--live`, `live` | — |

`hermes config apply`, `hermes config export` y `hermes contacts code` necesitan la base de Hermes: hasta que el
arranque la conecta y la migra, responden `unavailable`. `hermes health` da el primer motivo en el orden del arranque:

| Motivo | Qué le falta a Hermes |
|---|---|
| `needs a new enrollment token` | Una identidad válida: no se ha dado de alta, o su certificado caducó, es de otra CA o Hécate rechazó su renovación, como tras una revocación urgente. Se arregla con un token de alta nuevo y `hermes enroll` ([la identidad de Hermes deja de valer](operacion.md#la-identidad-de-hermes-deja-de-valer)) |
| `access list not loaded` | Una copia de su lista de acceso, ni guardada ni obtenida de Hécate |
| `database unavailable` | Su base, que no responde |
| `configuration not loaded` | La configuración de su base, sin leer o rechazada al validarla (p. ej. usa el `fake` sin `HERMES_FAKE_PROVIDERS=enabled`) |

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [Configuración](configuracion.md): el fichero que aplica la CLI.
- [Operación](operacion.md): los procedimientos que usan estas órdenes.
- [CLI y configuración de Hécate](../hecate/cli.md): las órdenes de Hécate que usa el admin para Hermes.
