# CLI y configuración de Hécate

Cómo se lanza Hécate, qué necesita del sistema, qué variables de entorno admite y qué hace cada orden de la CLI
`hecate`, con sus argumentos, su salida, sus errores y su código de salida. Es para el admin, que opera Hécate desde su
equipo; qué hacer en cada situación está en [operación](operacion.md).

## Lanzar Hécate

`hecate serve` ejecuta Hécate en primer plano hasta que recibe SIGINT o SIGTERM:

1. crea el directorio de datos si no existe y abre su base de datos, `hecate.db`;
2. se protege: desactiva los volcados de memoria y bloquea su memoria
   ([lo que necesita del sistema](#lo-que-necesita-del-sistema));
3. abre el socket del canal local, por el que le hablan las demás órdenes de la CLI;
4. si hay CA y su certificado propio es válido, abre la API en `HECATE_API_ADDR`; sin CA, la API no escucha.

Hécate arranca siempre [sellada](sellado.md#los-estados). Al parar, borra la raíz de la memoria, cierra el socket y
sale con 0. Escribe sus logs en la salida estándar, una línea JSON por evento (`ts`, `level`, `logger`, `event` y sus
campos), nunca con valores: cada orden del canal local deja una línea con la operación y su resultado, y la API avisa
cuando empieza o deja de escuchar.

Si no puede preparar el directorio de datos o abrir la base de datos, escribe `error: cannot prepare <directorio>: ...`
o `error: cannot open <fichero>: ...` y sale con 1. Hécate y su CLI corren con el mismo usuario: cómo se deja corriendo
como servicio en el home lab es una decisión [abierta](../../DECISIONS.md#abiertas).

## Lo que necesita del sistema

| Qué | Qué hace Hécate | Si no puede |
|---|---|---|
| **Sin volcados de memoria** | Pone a cero el límite de los volcados al arrancar | Avisa en el log (`core dumps could not be disabled`) y sigue |
| **Memoria bloqueada** | En Linux bloquea toda su memoria, la actual y la futura, para que la raíz no llegue al swap. El usuario necesita permiso para bloquear memoria: un límite de memoria bloqueada sin tope (p. ej. `--ulimit memlock=-1:-1` en un contenedor) o la capacidad `CAP_IPC_LOCK` | En Linux sin permiso avisa (`mlockall failed (missing permission?); the root may reach swap`); en macOS no se puede, y avisa siempre (`memory locking is only available on Linux; the root may reach swap`). En los dos casos sigue |
| **Directorio de datos privado** | Lo crea con permisos `0700`, y se los pone si ya existía | — |
| **Base de datos privada** | Crea `hecate.db` con permisos `0600` | — |
| **Socket privado** | Lo crea con permisos `0600`, y sustituye el que haya quedado de una ejecución anterior. En Linux solo atiende conexiones del mismo usuario que el proceso; en macOS la barrera es el socket `0600` dentro de un directorio `0700` | — |
| **Ruta del socket corta** | Los sockets Unix admiten rutas de poco más de 100 bytes | No arranca |

Hécate atiende las órdenes del canal local de una en una. El canal local y su protocolo solo los usa esta CLI.

## Variables de entorno

| Variable | Por defecto | Efecto | Límites |
|---|---|---|---|
| `HECATE_DATA_DIR` | Ninguno: es obligatoria | El directorio de datos: la base de datos `hecate.db` y, por defecto, el socket | No puede estar vacía |
| `HECATE_SOCKET_PATH` | `<HECATE_DATA_DIR>/control.sock` | La ruta del socket del canal local | Una ruta corta; vacía, vale la de por defecto |
| `HECATE_KDF_PROFILE` | `sensitive` | El perfil de Argon2id: `sensitive` (4 pasadas y 1 GiB) o `moderate` (3 pasadas y 256 MiB). En `hecate serve`, el de la frase de desbloqueo; en `hecate ca create` y `hecate ca replace`, el de la copia de la clave de la CA raíz | Solo esos dos, en mayúsculas o en minúsculas |
| `HECATE_API_ADDR` | `127.0.0.1:8443` | La dirección IP y el puerto donde escucha la API | Una dirección IP y un puerto |
| `HECATE_CERT_LIFETIME_HOURS` | `72` | La vida de los certificados de los componentes y de Hécate | Un entero de 1 a 72 |
| `HECATE_ENROLL_TOKEN_MINUTES` | `15` | La vida de un token de alta | Un entero de 1 a 60 |

- Ninguna lleva un secreto.
- Todas las órdenes leen la configuración antes de hacer nada, así que todas necesitan `HECATE_DATA_DIR`, y las que
  hablan con Hécate usan el mismo socket que `hecate serve`.
- Con un valor que no vale, la orden se detiene antes de hacer nada, escribe `error: <mensaje>`, p. ej.
  `error: HECATE_CERT_LIFETIME_HOURS must be a whole number of hours from 1 to 72`, y sale con 2.

## Convenciones

- **Salida:** lo que sale bien va a la salida estándar, una línea por resultado; los errores van a la salida de error.
- **Errores de Hécate:** `error (<código>): <mensaje>`, con ` (retry in <n>s)` si es `rate_limited` y
  ` [state: <estado>]` si el estado de sellado no permite la orden. Los códigos son `validation`, `auth`, `not_found`,
  `conflict`, `rate_limited` e `internal`, con el sentido del sobre común de Cerbero.
- **Errores locales:** `error: <mensaje>`, p. ej. `error: cannot reach Hécate at <socket>` si `hecate serve` no está
  en marcha, o `error: cannot read <fichero>: <motivo>`.
- **Códigos de salida:**

  | Código | Cuándo |
  |---|---|
  | `0` | La orden se hizo, también si no cambió nada |
  | `1` | Hécate la rechazó, no se pudo hablar con ella o se canceló una pregunta |
  | `2` | La configuración no vale, o falta un argumento u opción |

- **Datos sensibles, nunca en la línea de órdenes:** la frase, las partes, la frase de la CA y los valores de los
  secretos se piden en el terminal, sin eco cuando la entrada es un terminal, y los valores también se pueden leer de un
  fichero (`--from-file`). Cancelar una pregunta (Ctrl-C o fin de la entrada) escribe `cancelled` y sale con 1.
- **Lo que se muestra una vez:** las partes de recuperación y los tokens de alta. Tras verlos, conviene borrar el
  historial del terminal.
- **Según el estado:** sin inicializar, solo valen `hecate init` y `hecate status`; sellada, solo `hecate unseal` y
  `hecate status`. El resto responde `conflict` con el estado, p. ej.
  `error (conflict): operation not allowed while sealed [state: sealed]`.
- **El registro:** cada orden que habla con Hécate, salvo `hecate status`, deja su fila en el
  [registro de accesos](auditoria.md#el-registro-de-accesos).

## Las órdenes

Las trece de primer nivel y sus 32 órdenes finales, agrupadas por concepto. Los errores de cada tabla son los propios
de la orden; los de las convenciones valen para todas.

### Sellado

| Orden | Qué hace y qué pide | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate serve` | Ejecuta Hécate ([lanzar Hécate](#lanzar-hécate)) | Sus logs | No puede preparar sus datos o abrir el socket (1) |
| `hecate init` | Inicializa Hécate, una vez. Avisa de que, sin la frase y sin dos partes, los datos no se recuperan, y pide `yes`; después, la frase nueva dos veces (12 caracteres o más); muestra las tres partes y pide dos para confirmar, como mucho 15 minutos después | Las tres partes (`  share 1: HEC2-...`), `Hécate is initialized and sealed. Unseal it with: hecate unseal` y el aviso de borrar el historial | Otra respuesta que `yes`: `cancelled`. Las frases no coinciden. Las partes no coinciden o se cancela: `initialization aborted: nothing was saved` |
| `hecate unseal` | Desbloquea con la frase, que pide sin eco; con `--shares`, con dos partes | `state: unsealed`; con partes, además, el consejo de fijar una frase nueva con `hecate passphrase change --with-shares` | `auth`: la frase no es, las partes no recuperan la raíz o una es de otro juego. `validation`: una parte mal escrita o repetida. `rate_limited` con los segundos que faltan ([freno de intentos](sellado.md#el-freno-de-intentos)) |
| `hecate seal` | Sella al momento: la raíz sale de la memoria | `state: sealed` | — |
| `hecate status` | El estado; vale en cualquier estado y no deja fila | Ver abajo | — |
| `hecate passphrase change` | Fija una frase nueva. Pide la prueba (la frase actual; con `--with-shares`, dos partes) y la frase nueva dos veces | `passphrase changed (configuration version <n>)` | `auth` o `rate_limited` con la prueba; frase corta (`validation`) |
| `hecate shares rekey` | Reparte tres partes nuevas. Pide la prueba (la frase actual; con `--with-shares`, dos partes actuales), las muestra y pide dos para confirmar | Las tres partes, `new recovery shares active (configuration version <n>)` y el aviso de borrar el historial | Las partes no coinciden o se cancela: `rekey aborted: the old shares still work` |

Un ejemplo de `hecate status` con Hécate desbloqueada con partes y con CA:

```text
state: unsealed
recommendation: unsealed with recovery shares; set a new passphrase
secrets expired: 0
secrets expiring in the next 7 days: 0
secrets due rotation: 0
ca: cerbero.home sha256:f00a219b22a50e6f5bd5c14ffaa326542ce71a5dd95b6b1f07de162efcd47443
hecate certificate 6ac09bc8fedf25ace40cb9be89474202 until 2026-11-15T18:32:37+00:00
identities expiring within 24h: 0
suspicious renewals within 24h: 0
enrollment throttled: no
```

- `state` es `uninitialized`, `sealed` o `unsealed`; `recommendation` solo aparece tras desbloquear con partes.
- Con fallos seguidos aparece `failed attempts in a row: <n>`, y si hay que esperar, `next attempt allowed in <n>s`.
- Sin CA, el bloque de la CA es una sola línea: `ca: none`. Con CA, `intermediate days left: <n>` solo aparece si a
  la intermedia le quedan menos de 30 días.
- Lo que significa cada recuento está en [acceso](acceso.md#el-permiso-de-estado).

### Secretos

Todas exigen Hécate desbloqueada. Las rutas son `secret://<identidad>/<resto>`
([rutas](secretos.md#las-rutas)), y las identidades `hecate` y `admin` no admiten secretos nuevos.

| Orden | Qué hace y qué pide | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate secrets create <ruta> --type <string \| json \| pem>` | Guarda un secreto nuevo, con su versión 1. Pide el valor sin eco, o lo lee de `--from-file <fichero>`. Opciones: `--max-versions <2 a 100>` (10 por defecto), `--rotate-every <P90D o P12W>` y `--expires-at <fecha ISO 8601>` ([metadatos](secretos.md#los-metadatos)) | `created <ruta> version 1 fingerprint <huella>` | `conflict` si la ruta ya existe; `validation` si la ruta, el tipo, el valor o un metadato no valen, o la identidad está reservada |
| `hecate secrets list` | Las rutas y sus metadatos; con `--identity <identidad>`, solo las de esa identidad | Una línea por secreto: `<ruta>  <tipo>  active <n>  versions <n>`, con ` expires <fecha>` si la tiene (`active -` si no hay versión activa); o `no secrets` | — |
| `hecate secrets get <ruta>` | Los metadatos y las versiones, nunca el valor | `<ruta>  (<tipo>, owner <identidad>, max versions <n>)`, `rotate every:` y `expires at:` si los tiene, y una línea por versión: `version <n>  <estado>  <fecha>  fingerprint <huella>` | `not_found` |
| `hecate secrets put <ruta>` | Añade una versión, que pasa a activa. Pide el valor sin eco, o lo lee de `--from-file <fichero>` | `added version <n> to <ruta> fingerprint <huella>` | `conflict` si otro cambio llegó entre medias |
| `hecate secrets revoke <ruta> --version <n>` | Revoca una versión al momento, sin periodo de gracia | `version <n> of <ruta> revoked` | `conflict` si su estado no lo permite |
| `hecate secrets restore <ruta> --version <n>` | Restaura una versión revocada, que pasa a `deprecated` con una gracia nueva | `version <n> of <ruta> restored` | `conflict` si no está revocada |
| `hecate secrets promote <ruta> --version <n>` | Vuelve a hacer activa una versión `deprecated` | `version <n> of <ruta> promoted` | `conflict` si no está `deprecated` |
| `hecate secrets destroy <ruta> --version <n>` | Destruye para siempre una versión revocada (borrado criptográfico). Avisa y pide la prueba: la frase actual; con `--with-shares`, dos partes | `version <n> of <ruta> destroyed` | `conflict` si no está revocada; `auth` o `rate_limited` con la prueba |

- Un valor leído de un fichero tiene que ser UTF-8, y sus finales de línea pasan a `\n`. El valor no puede estar vacío
  ni pasar de 64 KiB.
- Las órdenes que cambian un secreto leen antes su versión y la mandan como versión esperada: si otro cambio llega entre
  medias, responden `conflict` y no cambian nada.

### CA

Las tres son [ceremonias](identidades.md#las-ceremonias): exigen Hécate desbloqueada, solo hay una a la vez y caducan
a los 15 minutos. La clave de la CA raíz solo existe en la memoria de la CLI mientras dura cada una.

| Orden | Qué hace y qué pide | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate ca create --trust-domain <td> --out <fichero>` | Crea la CA. Pide la frase de la CA (12 caracteres o más, distinta de la de desbloqueo), genera la CA raíz, firma la intermedia que prepara Hécate y escribe la copia de la clave raíz en `<fichero>`, con permisos `0600`. Después pide la ruta de una copia guardada y otra vez la frase, y solo si la copia se abre la CA queda creada | `root key saved to <fichero>; store it on two separate media`, y al final `created ca for <td>` y `root fingerprint sha256:<huella>` | `<fichero> already exists; it is not overwritten`; ya hay CA (`conflict`); frase corta o igual a la de desbloqueo (`validation`); `the saved copy does not open with that passphrase`: la ceremonia se aborta y la copia que se escribió se borra |
| `hecate ca renew-intermediate --root-key <copia>` | Renueva la intermedia, también ya caducada. Lee la copia de la clave de la CA raíz y pide su frase | `renewed intermediate version <n> until <fecha>` | `the copy does not open: wrong passphrase or the copy of another CA`; `the copy does not hold the key of the root CA of Hécate`; `cannot read <copia>` |
| `hecate ca replace --out <fichero>` | Sustituye la CA entera. Pide la prueba (la frase actual; con `--with-shares`, dos partes) y una frase de la CA nueva, y sigue como `hecate ca create` | `replaced ca for <td> generation <n>` y `root fingerprint sha256:<huella>` | Los de `hecate ca create`, más `auth` o `rate_limited` con la prueba. Después, cada componente se da de alta otra vez |

El trust domain son de 1 a 255 caracteres entre letras minúsculas, cifras, `.`, `-` y `_`, p. ej. `cerbero.home`.

### Tokens de alta

| Orden | Qué hace | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate tokens create <identidad>` | Crea un token de alta para una identidad, también una que aún no existe | `token HET1....`, `id <id> identity <identidad> expires <fecha>` y `this token is shown only once` | Identidad reservada (`validation`); deshabilitada, sin CA o con la intermedia caducada (`conflict`) |
| `hecate tokens list` | Los tokens pendientes, sin su valor | `<id> <identidad> expires <fecha>` por token, o `no pending tokens` | — |
| `hecate tokens annul <id>` | Anula un token pendiente; uno que ya no lo está no cambia | `annulled <id>` | — |

### Identidades

| Orden | Qué hace | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate identities list` | Las identidades que se han dado de alta | `<identidad> <enrolled \| disabled> certificate until <fecha>` por identidad, o `no identities` | — |
| `hecate identities get <identidad>` | Una identidad y sus certificados, sin claves ni tokens | `identity <identidad> <estado>` y, por certificado, `<serie> <valid \| revoked \| expired> renewable <yes \| no> <desde> <hasta>` | `not_found` si nunca se dio de alta |
| `hecate identities disable <identidad>` | La deshabilita: sin renovación, sin claves publicadas y sin tokens. Con `--urgent`, además revoca con urgencia sus certificados válidos | `disabled <identidad>` y, con `--urgent`, `revoked <n> certificates` | `the identity 'hecate' cannot be disabled` (`validation`) |
| `hecate identities enable <identidad>` | La habilita; vuelve a tener certificado con un alta y un token nuevo | `enabled <identidad>` | — |

### Certificados

| Orden | Qué hace | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate certificates revoke <serie>` | Por defecto, revocación pasiva: el certificado deja de renovarse. Con `--urgent`, deja de valer al momento, entra en la lista de revocación y su clave de firma de llamadas deja de firmar ([revocación](identidades.md#la-revocación)) | `revoked <serie>`, con ` urgent` si lo es; el de Hécate añade `reissued <serie nueva>` | El certificado de Hécate sin `--urgent`: `Hécate's certificate only admits an urgent revocation` (`validation`) |

### Políticas

Las órdenes de la [Access Policy](acceso.md#la-access-policy) no se conceden a `hecate` ni a `admin`, y se pueden dar a
una identidad que aún no se ha dado de alta. Una orden que no cambia nada responde `unchanged: ...` y sale con 0.

| Orden | Qué hace | Salida si va bien | Errores propios |
|---|---|---|---|
| `hecate policies grant-read <identidad> <ruta>` | Deja a una identidad leer un secreto concreto de otra, sin `@N` | `granted read <ruta> to <identidad> (policy version <n>)` o `unchanged: <identidad> already reads <ruta>` | `not_found` si el secreto no existe; `validation` si es de la propia identidad |
| `hecate policies revoke-read <identidad> <ruta>` | Se lo retira, desde la petición siguiente | `revoked read <ruta> from <identidad> (policy version <n>)` o `unchanged: <identidad> does not read <ruta>` | — |
| `hecate policies grant-status <identidad>` | Le concede el permiso de estado | `granted status to <identidad> (policy version <n>)` o `unchanged: <identidad> already has status` | — |
| `hecate policies revoke-status <identidad>` | Se lo retira | `revoked status from <identidad> (policy version <n>)` o `unchanged: <identidad> has no status` | — |
| `hecate policies list` | Las identidades con algún cambio de política, también si ya no tienen nada concedido | `<identidad> version <n> status <yes \| no> reads <n>` por identidad, o `no policies` | — |
| `hecate policies get <identidad>` | La política vigente y su historial | `policy <identidad> version <n> status <yes \| no>`, una línea `read <ruta>` por excepción, y tras `history`, una por cambio: `<versión> <fecha> <cambio> [<ruta>] by admin` | — |

## Relacionado

- [Hécate](../hecate.md): el documento de alto nivel.
- [Operación](operacion.md): los procedimientos que usan estas órdenes.
- [Sellado](sellado.md), [secretos](secretos.md), [identidades](identidades.md) y [acceso](acceso.md): el diseño que
  hay detrás de cada grupo de órdenes.
