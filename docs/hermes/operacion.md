# Operación de Hermes

Los procedimientos para poner Hermes en marcha y para cada situación de su operación: conceder y retirar el acceso,
verificar un canal, rotar el token de un bot, Hécate sellada o caída, PostgreSQL caído, una instancia que falla, la
identidad de Hermes que deja de valer, arrancar sin copia de la lista de acceso y restaurar una copia. Cada uno dice
cuándo se aplica, qué hace falta, los pasos con las órdenes de la [CLI de Hermes](cli.md) y de la
[de Hécate](../hecate/cli.md), y cómo comprobar que ha ido bien; el último apartado dice cómo se ejecutan en el compose
de pruebas. Es para el admin.

## Antes de empezar

- Las órdenes de `hermes` se ejecutan en el equipo de Hermes, con el mismo usuario y las mismas variables de entorno que
  `hermes serve` ([CLI](cli.md#variables-de-entorno)); las de `hecate`, en el de Hécate
  ([operar Hécate](../hecate/operacion.md)).
- `hermes serve` corre en primer plano y se para con SIGINT o SIGTERM. Es un solo proceso asyncio con la API, el canal
  local y sus tareas de fondo: el arranque, la renovación de su certificado, el refresco cada 240 s de las publicaciones
  de Hécate (la lista de revocación, las claves publicadas y su lista de acceso) y de la contraseña de su base, que
  también pide tras cada alta, la limpieza cada 5 minutos de las claves de idempotencia y de los `jti`, y un polling
  por instancia ([ADR 0011](../adr/0011-pila-hermes.md)). Cómo se deja corriendo como servicio en el home lab es una
  decisión [abierta](../../DECISIONS.md#abiertas).
- **La salud** va por el canal local: `hermes health` dice si Hermes está listo o por qué no, y `hermes health --live`,
  si el proceso responde, que es lo que usa el healthcheck de su contenedor ([CLI](cli.md#las-órdenes)).
- **El log** de `hermes serve` dice cómo va el arranque: `startup.failed`, con el paso y el motivo, cuando un paso falla
  y se reintenta a los 30 s; `startup.waiting` mientras espera el alta; y `startup.ready` al terminar.
- El lado de quien llama está en la [guía de integración](integracion.md).

| Situación | Procedimiento |
|---|---|
| Equipo sin Hermes | [Puesta en marcha](#puesta-en-marcha) |
| Un componente va a llamar a Hermes, o deja de hacerlo | [Conceder y retirar el acceso](#conceder-y-retirar-el-acceso) |
| Un contacto nuevo, o un canal que perdió su verificación | [Verificar un canal](#verificar-un-canal) |
| El token de un bot cambia en su proveedor | [Rotar el token de un bot](#rotar-el-token-de-un-bot) |
| Hécate no responde o está sellada | [Hécate sellada o caída](#hécate-sellada-o-caída) |
| La base de Hermes no responde | [PostgreSQL caído](#postgresql-caído) |
| Una instancia `degraded` o `unavailable` | [Una instancia que falla](#una-instancia-que-falla) |
| `not ready: needs a new enrollment token`, o el certificado de Hermes revocado con urgencia | [La identidad de Hermes deja de valer](#la-identidad-de-hermes-deja-de-valer) |
| `not ready: access list not loaded` | [Sin copia de la lista de acceso](#sin-copia-de-la-lista-de-acceso) |
| Se pierde la base o el directorio de estado | [Restaurar una copia](#restaurar-una-copia) |

## Puesta en marcha

**Cuándo:** la primera vez, en un equipo sin Hermes.

**Qué necesitas:**

- Hécate en marcha y desbloqueada, con su CA ([operar Hécate](../hecate/operacion.md#puesta-en-marcha));
- en el PostgreSQL común, la base de Hermes y su usuario, sin acceso a las demás bases
  ([ADR 0012](../adr/0012-persistencia-postgresql-comun.md)), y la contraseña de ese usuario;
- Envoy, con la configuración de `hermes/deploy/envoy/` como referencia ([el proxy](seguridad.md#el-proxy));
- el token de cada bot, creado en su proveedor;
- el fichero de la configuración ([configuración](configuracion.md)), fuera de cualquier repositorio;
- las identidades de los componentes que van a llamar a Hermes.

**Pasos:**

1. Guarda en Hécate los secretos de Hermes, cada uno sin eco o con `--from-file <fichero>`: la contraseña de su base,
   `hecate secrets create secret://hermes/postgres/password --type string`, y los de cada instancia,
   `hecate secrets create secret://hermes/<proveedor>/<instancia>/<campo> --type string` (en Telegram, el campo es
   `bot_token`).
2. Crea el token de alta de Hermes: `hecate tokens create hermes`. Vale 15 minutos por defecto.
3. Arranca Hermes con sus variables de entorno ([CLI](cli.md#variables-de-entorno)): `hermes serve`. Abre sus sockets
   y espera el alta.
4. Da de alta a Hermes: `hermes enroll`, con el token sin eco o desde la entrada estándar. Hermes escribe su
   certificado en `HERMES_PROXY_DIR` y sigue el arranque.
5. Arranca Envoy delante de Hermes, con acceso al socket de la API y al directorio del proxy.
6. Aplica la configuración: `hermes config apply <fichero>`, revisa los cambios y responde `yes`.
7. Verifica cada canal ([verificar un canal](#verificar-un-canal)).
8. Concede el acceso a cada componente que va a llamar a Hermes
   ([conceder y retirar el acceso](#conceder-y-retirar-el-acceso)).

**Cómo comprobarlo:**

- antes del alta, `hermes health` dice `not ready: needs a new enrollment token`; después, `ready`, y el log de Hermes,
  `startup.ready`;
- `hermes enroll` escribe `enrolled spiffe://<trust domain>/hermes serial <serie> until <fecha>`;
- `hermes config apply` termina con `applied <n> changes`, y aplicar otra vez el mismo fichero, con `no changes`;
- un componente con acceso envía un mensaje, que llega a su contacto ([guía de integración](integracion.md)).

Hermes prueba sus instancias al arrancar, así que las que llegan con la primera configuración se prueban en el
siguiente arranque o con la prueba de conexión ([una instancia que falla](#una-instancia-que-falla)).

## Conceder y retirar el acceso

**Cuándo:** un componente va a llamar a Hermes, o deja de hacerlo.

**Qué necesitas:** la identidad del componente, que puede no tener alta en Hécate.

**Pasos:**

1. Para concederlo: `hecate access grant <identidad> hermes`. Vale desde la llamada siguiente del componente, sin
   esperar a que Hermes refresque su lista.
2. Para retirarlo: `hecate access revoke <identidad> hermes`. Vale cuando Hermes refresca su lista, como mucho a los
   240 s, sin reiniciarlo.

**Cómo comprobarlo:**

- `hecate access get hermes` muestra la lista vigente, con una línea `caller <identidad>` por identidad;
- antes de concederlo, el componente recibe `auth` y el log de Hermes dice `auth.refused` con el motivo `not_allowed`
  y el `request_id` de la llamada; después, su llamada siguiente funciona;
- como mucho 240 s después de retirarlo, vuelve a recibir `auth`.

## Verificar un canal

**Cuándo:** un contacto nuevo en la configuración, o un canal que perdió su verificación al borrarse o al cambiar de
instancia ([la verificación](comunicacion.md#la-verificación-de-un-canal)).

**Qué necesitas:** el contacto y su canal en la configuración aplicada, y la persona, con la cuenta del proveedor en la
que va a recibir los mensajes; en Telegram, su cuenta, desde la que escribe al bot en un chat privado.

**Pasos:**

1. Pide el código: `hermes contacts code <contacto> --instance <instancia>`. Escribe
   `code <XXXX-XXXX> for <contacto> on <instancia> expires <fecha>`. El canal queda sin verificar hasta que llega el
   código, también si ya lo estaba, y un código nuevo deja sin valor el anterior.
2. Dáselo a la persona, que lo envía al bot desde su cuenta antes de que caduque: 30 minutos por defecto
   (`HERMES_VERIFICATION_CODE_MINUTES`).

**Cómo comprobarlo:**

- el bot le contesta «Canal verificado.», y el log de Hermes dice `channel.verified` con el contacto y la instancia;
- un envío a ese contacto ya no responde `conflict`;
- un código caducado, usado o mal escrito no recibe respuesta: pide otro.

## Rotar el token de un bot

**Cuándo:** el token de un bot cambia en su proveedor, por ejemplo porque se regenera tras filtrarse.

**Qué necesitas:** el token nuevo y un componente con acceso, para comprobar la instancia.

**Pasos:**

1. Añade la versión nueva en Hécate: `hecate secrets put secret://hermes/<proveedor>/<instancia>/<campo>`, con el
   valor sin eco o con `--from-file <fichero>`. Pasa a ser la activa, y la anterior queda en su periodo de gracia.
2. Hermes toma la versión nueva al refrescar su caché: la primera vez que usa la instancia pasados 5 minutos desde la
   última resolución, así que como mucho 5 minutos después de añadirla ([los secretos](seguridad.md#los-secretos)).
3. Pasado ese tiempo, pide desde un componente con acceso la prueba de conexión de la instancia,
   `POST /v1/instances/{name}/test` ([integración](integracion.md#las-capacidades-y-la-prueba-de-conexión)).
4. Si la versión nueva no funciona, vuelve a la anterior mientras dura su gracia, 24 horas:
   `hecate secrets promote <ruta> --version <n>`. Hermes la toma igual, como mucho en 5 minutos.

**Cómo comprobarlo:** la prueba funciona (el sobre dice `completed`) y los envíos por la instancia llegan. Si el
proveedor rechazó el token anterior entre medias, la instancia queda `unavailable` en las capacidades hasta que una
prueba funciona, aunque los envíos ya lleguen, y después `degraded` mientras ese fallo esté entre sus diez llamadas
últimas.

## Hécate sellada o caída

**Cuándo:** Hécate no responde o está sellada, por ejemplo tras un reinicio, un corte de luz o `hecate seal`.

**Qué pasa mientras:**

- Hermes sigue con los secretos de su caché hasta una hora desde su última resolución; después, los envíos que los
  necesitan fallan con `internal` sin llegar al proveedor;
- sigue con sus últimas publicaciones, su lista de acceso incluida: nadie gana el acceso ni lo pierde por ello, y
  quien no está en su copia recibe `unavailable`;
- un componente recién dado de alta o renovado, cuya clave de firma Hermes no tiene, recibe `unavailable`;
- su certificado vale hasta que caduca, y no se renueva;
- `hermes config apply` con una referencia de secreto nueva o cambiada no aplica nada, porque no puede comprobarla.

**Pasos:** arranca o desbloquea Hécate ([Hécate caída o sellada](../hecate/operacion.md#hécate-caída-o-sellada)).
Hermes sigue solo, sin reiniciarlo.

**Cómo comprobarlo:** los envíos funcionan, también los que necesitan una resolución nueva, y una concesión nueva vale
en la llamada siguiente.

## PostgreSQL caído

**Cuándo:** la base de Hermes no responde, porque el PostgreSQL común está parado o Hermes no llega a él.

**Qué pasa mientras:** Hermes no está listo, y los envíos responden `unavailable` sin llegar al proveedor: no envía
nada que no pueda registrar. `hermes config apply`, `hermes config export` y `hermes contacts code` también responden
`unavailable`.

**Pasos:** arranca el PostgreSQL común, o arregla lo que impide llegar a él. Hermes vuelve solo, sin reiniciarlo.

**Cómo comprobarlo:** mientras está caído, `hermes health` dice `not ready: database unavailable`; después, vuelve a
decir `ready`, y los envíos funcionan.

## Una instancia que falla

**Cuándo:** las capacidades dicen que una instancia está `degraded` o `unavailable`, los envíos por ella fallan con
`provider_unavailable` o `timeout`, o el log de Hermes dice `instance.tested` con otro estado que `healthy`.

**Qué necesitas:** un componente con acceso, para consultar las capacidades y pedir la prueba de conexión
([integración](integracion.md#las-capacidades-y-la-prueba-de-conexión)).

**Pasos:**

1. Mira su estado en `GET /v1/capabilities` y por qué lo tiene
   ([qué dice cada estado](integracion.md#las-capacidades-y-la-prueba-de-conexión)).
2. Si el proveedor rechazó las credenciales, comprueba el token en el proveedor y, si cambió,
   [rótalo](#rotar-el-token-de-un-bot).
3. Si el proveedor no responde, comprueba que Hermes llega a él: la red, la resolución de nombres y el estado del
   proveedor. Hermes lo sigue intentando en cada envío y en su polling.
4. Pide la prueba de conexión de la instancia: `POST /v1/instances/{name}/test`.

**Cómo comprobarlo:** la prueba funciona (el sobre dice `completed`) y los envíos por la instancia también. El estado
vuelve a `healthy` cuando ninguna de sus diez llamadas últimas ha fallado; si fallaron las tres últimas, sigue en
`unavailable` hasta que una funciona. Al arrancar, Hermes prueba cada instancia y lo dice en su log
(`instance.tested`).

## La identidad de Hermes deja de valer

**Cuándo:** `hermes health` dice `not ready: needs a new enrollment token`, porque Hermes no tiene identidad, su
certificado caducó sin renovarse, Hécate sustituyó su CA o Hécate rechazó una llamada hecha con su certificado: tras una
revocación urgente, por ejemplo porque su clave se comprometió, con su identidad deshabilitada o tras un segundo intento
de renovar. Hermes nota el rechazo en la primera llamada que Hécate le rechaza, y como mucho en el siguiente refresco de
sus publicaciones, 240 s después de la revocación.

**Qué pasa mientras:**

- Hermes no está listo, y su API responde `unavailable`.
- Si Hécate rechazó su certificado, Hermes no vuelve a llamar a Hécate con él hasta el alta nueva o un reinicio, y su
  log dice una vez `identity.refused`, con la serie (`serial`) y la llamada rechazada (`operation`).
- Tras un reinicio, Hermes lo vuelve a comprobar con Hécate en la primera llamada del arranque, y su salud da el mismo
  motivo a los pocos segundos.
- Si Hécate sustituyó su CA, Hermes sigue intentando conectar y vuelve a estar listo solo si Hécate se presenta otra
  vez con la CA de siempre; con la CA nueva, necesita el alta.

**Qué necesitas:** la identidad de Hermes habilitada en Hécate. `hecate identities list` la muestra como `enrolled`; si
está `disabled`, habilítala antes con `hecate identities enable hermes`
([clave de un componente comprometida](../hecate/operacion.md#clave-de-un-componente-comprometida)).

**Pasos:**

1. Espera a que `hermes health` diga `not ready: needs a new enrollment token`. Tras una revocación urgente,
   `hermes enroll` responde `conflict` («hermes already has a valid identity») hasta que Hermes nota el rechazo.
2. Crea un token de alta nuevo para Hermes: `hecate tokens create hermes`.
3. Da de alta a Hermes otra vez: `hermes enroll`, con el token sin eco o desde la entrada estándar, sin parar Hermes ni
   vaciar su directorio de estado. El alta descarta la renovación en curso, si la había, y no toca su base.

**Cómo comprobarlo:** `hermes enroll` escribe `enrolled spiffe://<trust domain>/hermes serial <serie> until <fecha>`,
con una serie nueva, y en 2 minutos como mucho `hermes health` vuelve a decir `ready`, con la configuración y los
canales verificados de antes. Envoy toma el certificado nuevo sin reiniciarse.

Si tras el alta Hermes vuelve enseguida a `needs a new enrollment token`, con otra línea `identity.refused`, los relojes
de Hermes y de Hécate pueden diferir en más de 30 s: Hécate rechaza entonces el token corto de cada llamada, y Hermes no
lo distingue de una revocación. Otra alta no lo arregla: corrige el reloj y reinicia Hermes.

## Sin copia de la lista de acceso

**Cuándo:** `hermes health` dice `not ready: access list not loaded`: Hermes arrancó sin ninguna copia de su lista de
acceso, porque es nuevo o perdió su directorio de estado, y Hécate no responde.

**Qué pasa mientras:** Hermes no está listo, y los envíos responden `unavailable`: sin la lista, no sabe a quién
atender.

**Pasos:** arranca o desbloquea Hécate. Hermes vuelve a pedir la lista en el siguiente intento de su arranque, a los
30 s como mucho, y sigue solo.

**Cómo comprobarlo:** `hermes health` dice `ready`, y su directorio de estado tiene la copia, `access.json`.

## Restaurar una copia

**Cuándo:** se pierde la base de Hermes o su directorio de estado.

**Lo que se puede hacer:**

- **La base**, con Hermes parado, se restaura desde una copia del PostgreSQL común con sus propias herramientas. Vuelve
  con lo que guardaba al hacerla: la configuración, la verificación de los canales, las claves de idempotencia, los
  `jti` usados y la auditoría. Al arrancar, Hermes la migra si hace falta y lee su configuración; lo que cambió después
  de la copia se recupera aplicando otra vez el fichero de la configuración y verificando los canales que falten.
- **El directorio de estado y el del proxy** no hace falta copiarlos: con un token de alta nuevo y `hermes enroll`,
  Hermes tiene otra identidad y vuelve a pedir su lista de acceso a Hécate
  ([la identidad de Hermes deja de valer](#la-identidad-de-hermes-deja-de-valer)).
- La política común de copias de Cerbero (la regla 3-2-1, el cifrado y la restauración probada) y el cifrado de las
  copias de Hermes con una clave de Hécate (`wrap`) son decisiones [abiertas](../../DECISIONS.md#abiertas).

## El compose de pruebas

`hermes/deploy/` tiene un compose con PostgreSQL, una Hécate de pruebas, Hermes y Envoy, para el E2E y para recorrer
estos procedimientos. Es un entorno de pruebas: no fija cómo corre Hermes en el home lab.

- **Los secretos del compose:** `hermes/deploy/prepare.sh` genera en `hermes/deploy/.secrets/`, fuera de Git, la
  contraseña de administración de PostgreSQL y la del usuario `hermes`, y conserva las que ya existen. Al arrancar por
  primera vez, PostgreSQL crea la base `hermes` y su usuario, sin acceso a las demás bases.
- **Los ficheros:** desde la raíz del repositorio, `COMPOSE_FILE` elige `hermes/deploy/compose.yaml`, para un bot de
  Telegram, o además `hermes/deploy/compose.test.yaml`, que carga el proveedor `fake`, con sus bandejas en
  `/home/hermes/fake`, y publica Hécate y Envoy en otros puertos:

```bash
export COMPOSE_FILE=hermes/deploy/compose.yaml:hermes/deploy/compose.test.yaml
```

- **Docker Compose:** las órdenes usan `docker compose`; con Docker Compose instalado aparte, la orden es
  `docker-compose`, con los mismos argumentos.

| Qué | Cómo |
|---|---|
| Arrancar PostgreSQL y Hécate | `docker compose up -d --build postgres hecate` |
| Una orden de Hécate | `docker compose exec hecate hecate <orden>`; si lee un valor de la entrada estándar, con `-T` detrás de `exec` |
| Arrancar Hermes y Envoy | `docker compose up -d --build hermes envoy`; Envoy espera a que Hermes responda |
| Una orden de Hermes | `docker compose exec hermes hermes <orden>`; si lee la entrada estándar, con `-T` detrás de `exec` |
| Aplicar la configuración | `docker compose exec -T hermes hermes config apply /dev/stdin --yes < <fichero>`. La entrada estándar es el fichero, así que la orden no puede preguntar: sin `--yes`, enseña los cambios y no aplica nada (`cancelled`), lo que sirve para revisarlos antes |
| Parar o arrancar un servicio | `docker compose stop <servicio>` y `docker compose start <servicio>` |
| El log de Hermes | `docker compose logs hermes` |
| Las direcciones | Hécate en `https://127.0.0.1:8443` y Envoy en `https://127.0.0.1:9443`; con `compose.test.yaml`, en los puertos `18443` y `19443` |
| Las bandejas del fake | Lo enviado, en `docker compose exec hermes cat /home/hermes/fake/<instancia>/outbox.jsonl`; un mensaje que llega, un fichero JSON en `/home/hermes/fake/<instancia>/inbox/` ([las bandejas](proveedores.md#el-proveedor-fake)) |
| Borrarlo todo | `docker compose down -v` |

- **La Hécate de pruebas** se pone en marcha como dice [operar Hécate](../hecate/operacion.md#puesta-en-marcha), con
  su propio trust domain, como `cerbero.test`: ningún secreto de la de verdad pasa a ella.
- **El cliente de referencia** hace de componente que llama. Se da de alta con su token por la entrada estándar,
  `uv run --package cerbero-hermes python hermes/tests/support/caller.py enroll --hecate <Hécate> --state <directorio>`,
  y después envía con `send` o `bench` y `--hermes <Envoy>`
  ([integración](integracion.md#probar-un-componente-contra-hermes)).
- **Un bot de Telegram de pruebas**, distinto del de casa: su token vive en `hermes/deploy/.secrets/telegram-test.env`,
  fuera de Git y con permisos 0600, como `TELEGRAM_TEST_BOT_TOKEN=<token>`, y pasa a la Hécate de pruebas por la
  entrada estándar, sin escribirse en la línea de órdenes ni en el terminal. Las pruebas usan el fake y respuestas
  grabadas; el bot de pruebas solo se usa en el recorrido completo con Telegram.

```bash
( set -a; . hermes/deploy/.secrets/telegram-test.env; set +a
  printf '%s\n' "$TELEGRAM_TEST_BOT_TOKEN" |
    docker compose exec -T hecate hecate secrets create secret://hermes/telegram/test_bot/bot_token --type string )
```

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [CLI y variables de entorno](cli.md): cada orden de `hermes`.
- [Operar Hécate](../hecate/operacion.md): la puesta en marcha de Hécate y sus situaciones.
- [Seguridad](seguridad.md): lo que se exige al proxy.
