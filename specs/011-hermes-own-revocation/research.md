# Research: Hermes nota la revocación urgente de su propio certificado

Decisiones técnicas del arreglo 011. Cada una dice qué se decidió, por qué y qué se descartó. Los nombres concretos de
métodos, eventos de log y ficheros se fijan aquí.

## R0. Decisiones del propietario

- **Al aclarar la spec (2026-11-19):**
  - cuenta el rechazo de cualquier llamada que Hermes hace a Hécate con su identidad, no solo el del refresco;
  - el rechazo vive solo en memoria: tras un reinicio, la primera llamada del arranque vuelve a comprobarlo;
  - una identidad rechazada sigue no válida hasta un alta nueva o un reinicio, y mientras tanto Hermes no llama a
    Hécate con ella.
- **Al revisar el plan (2026-11-19):** aprobó el paso único con el port `SecretResolver` (R2), qué cuenta como rechazo
  (R3), la llamada bloqueada que responde `auth` y la línea `identity.refused` (R4 y R5), el arranque sin cambios (R6)
  y la pasada en Linux (R9).
- **Al analizar (2026-11-19):** aprobó los seis remedios, y que Hermes vuelva a pedir la contraseña de su base en
  cuanto termina el alta (R7).
- **En la segunda pasada de analyze (2026-11-19):** aprobó los cuatro remedios: la spec sin nombres de tecnología, la
  tarea de fondo nueva en la operación (R10), qué filas del registro de accesos son de Hermes (R8) y el nombre de la
  tarea de la contraseña tras cada alta (R7).
- **Al implementar (2026-11-19):** adelantó T022 y T024, la contraseña de la base tras el alta (R7), al checkpoint de
  US1: el escenario de T013 da de alta a Hermes, y el siguiente del E2E necesita una conexión nueva a la base.
- **El orden:** el arreglo empieza con la 010 fusionada en `main`, porque toca los mismos documentos.

## R1. La causa

Reproducida en el recorrido de la 010 y confirmada en el código de `main` (`f11cd29`):

- `IdentityService.refresh_publications` ([identity.py](../../hermes/src/hermes/application/identity.py)) atrapa el
  `HermesError` de Hécate y solo deja `publications.refresh_failed` con su motivo: un `auth` no cambia nada.
- La identidad solo se da por rechazada en `renewal_tick`, cuando Hécate rechaza la renovación con `auth`, a los dos
  tercios de la vida del certificado (unos 2 días con certificados de 3).
- `problem()` mira además si su serie está en la lista de revocación, pero Hécate nunca da esa lista a un certificado
  revocado: le rechaza la llamada antes.
- `enroll()` responde `conflict` mientras `problem()` es `None`.
- Tras un reinicio, el arranque usa la copia guardada de las publicaciones y falla al resolver la contraseña de la
  base con `auth`, así que la salud dice `database unavailable` y el alta responde `conflict` hasta el primer refresco.

## R2. Un solo paso para las llamadas con la identidad de Hermes

**Decisión:** el servicio de identidad es el único paso de las llamadas a Hécate hechas con el certificado de Hermes.
Un método privado, `_as_hermes(operation, call)`, envuelve cada una:

| Llamada | Quién la hace | `operation` |
|---|---|---|
| `renew` | `renew()`, desde `renewal_tick` | `renew` |
| `revocations`, `keys` y `callers` | `_fetch_publications()`: el refresco y el arranque sin copia | `revocations`, `keys`, `callers` |
| `key` | `_ask_key()`, ante un `kid` que la lista no tiene | `key` |
| `callers` | `_ask_access()`, ante quien no está en la copia | `callers` |
| `resolve` | `resolve()`, nuevo: la caché de secretos y la comprobación de la configuración | `resolve` |

- **La resolución pasa por el servicio:** `SecretCache` y `HecateSecretChecker` dependen de un port estrecho nuevo,
  `SecretResolver` (en `ports.py`, con solo `resolve`), en lugar de `HecateApi`. En `hermes serve` reciben el servicio
  de identidad; sus pruebas siguen dándoles `FakeHecate`, que cumple el port igual que el cliente de Hécate.
- **El alta no pasa por él:** no usa la identidad de Hermes, sino el token del admin.

**Por qué:** una sola regla para todos los caminos (R0), en la capa donde ya vive la validez de la identidad, y que se
prueba con `FakeHecate` en las unitarias del servicio, como pide el TDD del arreglo.

**Alternativas:**

- **Que lo detecte y lo bloquee el cliente de Hécate** (el adaptador), por donde ya pasan todas las llamadas: la regla
  viviría en el adaptador, y las pruebas del servicio, que usan `FakeHecate` en su lugar, no la verían.
- **Un decorador de `HecateApi`** entre el cliente y los servicios: crea un ciclo con el servicio de identidad, que
  necesita la API, y atribuye el rechazo a una identidad leída aparte de la llamada.
- **Detectarlo solo en el refresco**, o en el refresco y la renovación: descartado por el propietario (R0).

## R3. Qué es un rechazo y a qué identidad cuenta

**Decisión:**

- **Un rechazo** es un `HermesError` con el código `auth` que sale de una llamada hecha con la identidad, salvo
  `HecateTrustChanged`: esa excepción hereda de `HermesError` con el mismo código, pero significa que Hécate presenta
  otra CA raíz, y sigue como hoy, sin marcar ni bloquear nada, para que Hermes vuelva a estar listo si Hécate se
  presenta otra vez con la CA de siempre (FR-009). Los demás códigos (`unavailable`, `internal`, `not_found`,
  `conflict`, `validation`, `rate_limited`) no cambian la validez (FR-003).
- **A qué identidad cuenta:** `_as_hermes` guarda la identidad vigente antes de llamar y, si llega el rechazo, solo
  marca si sigue siendo la vigente (`self._identity is identity`); el rechazo de una identidad ya sustituida por un
  alta nueva no cuenta (FR-004).
- **La marca** es la serie de la identidad rechazada, `self._refused: str | None`, en memoria (FR-007), y sustituye al
  booleano que hoy pone la renovación rechazada. `problem()` dice `needs a new enrollment token` si la serie de la
  identidad vigente es la marcada; el alta pone otra identidad y borra la marca.
- **El invariante del cliente:** el cliente de Hécate lee la identidad al empezar cada llamada (`_authenticated`), sin
  esperar nada entre la captura de `_as_hermes` y esa lectura, así que la identidad capturada es la que firma la
  llamada. Una prueba fija la atribución con una llamada en curso durante un alta.

**Alternativas:**

- **Una marca booleana**, como la de hoy: no distingue la identidad rechazada de la nueva, así que el alta tendría que
  borrarla a mano y un rechazo atrasado invalidaría la nueva.
- **Comparar por la serie de la respuesta:** el sobre de Hécate no dice qué certificado rechazó.

## R4. Sin llamadas con una identidad rechazada

**Decisión:** con la identidad vigente marcada, `_as_hermes` responde `HermesError(auth, "hecate refused the identity
of hermes: it needs a new enrollment token")` sin llamar a Hécate. Cada camino falla como hoy ante un rechazo de
Hécate (FR-008):

| Camino | Qué pasa |
|---|---|
| La caché de secretos | Quita el valor y falla, como hoy con `auth`; dentro del tiempo blando sigue dando el valor guardado sin llamar a nadie, como hoy |
| El refresco de las publicaciones | Deja `publications.refresh_failed` con el motivo `auth` y conserva su copia |
| Una clave o la lista de acceso ante quien llama | `unavailable`, como hoy ante un fallo de Hécate; con la identidad rechazada no se llega a ellas, porque la API responde antes `unavailable` |
| La comprobación de los secretos al aplicar la configuración | `unavailable`: «hecate cannot check the secrets of the file: nothing was applied» |
| La renovación | Ni lo intenta: `renewal_tick` mira antes `problem()` |
| El polling de cada instancia | Sigue como hoy mientras el token de su bot está en el tiempo blando de la caché; después falla y reintenta a los 5 s, sin llamar a Hécate |

- **El alta sí sale:** no usa la identidad rechazada.
- **La CA sustituida no se bloquea** (R3).

**Por qué:** cada llamada rechazada deja una fila en el registro de accesos de Hécate, que es append-only y no se purga
([auditoría de Hécate](../../docs/hecate/auditoria.md#el-registro-de-accesos)); hoy, el polling de cada instancia
dejaría una cada 5 s, y esas filas taparían las de quien use la clave robada.

**Alternativas:**

- **Parar las tareas de fondo** (el refresco, la contraseña, el polling) mientras la identidad no vale: más piezas que
  tocar, y que volver a arrancar tras el alta; el paso único las cubre todas.
- **Responder `unavailable`** en lugar de `auth`: la caché seguiría dando los valores guardados hasta su tiempo duro,
  que no es lo que hace hoy ante un rechazo, y FR-008 pide fallar como si Hécate hubiera rechazado la llamada.

## R5. La línea del log

**Decisión:** `identity.refused`, de nivel warning, con `serial` y `operation` (la llamada que Hécate rechazó, R2), una
vez por identidad rechazada mientras Hermes no se reinicia: tras un reinicio, la primera llamada rechazada la vuelve a
dejar (FR-007). Sin secretos ni certificados: la serie ya sale en `identity.enrolled`.

- Las demás líneas no cambian: cada camino sigue dejando la suya con el motivo `auth` (`publications.refresh_failed`,
  `startup.failed`, `polling.failed`, `identity.renewal_failed`), y la salud sigue con su motivo (FR-009).
- Una llamada bloqueada no deja línea propia: su camino ya deja la suya.

**Alternativas:**

- **Solo las líneas de hoy:** cada camino escribe la suya, y alguna solo dice el tipo del error
  (`secrets.refresh.failed` con `HermesError`), así que el admin no tendría una línea que buscar.
- **Una línea por llamada bloqueada:** una cada 5 s por instancia, el ruido que R4 quita del registro de Hécate.

## R6. El arranque y el reinicio

**Decisión:** el arranque no cambia. Tras un reinicio no hay marca (FR-007), y la primera llamada con la identidad, la
que resuelve la contraseña de la base (o la de las publicaciones, si no hay copia), recibe el rechazo: el servicio
marca, la salud dice `needs a new enrollment token` a los pocos segundos, y el paso se reintenta cada 30 s sin salir
de Hermes. `hermes enroll` funciona, y el reintento siguiente sigue con la identidad nueva; con los pasos que quedan,
Hermes está listo en 2 minutos como mucho (SC-003).

**Alternativas:**

- **Devolver el arranque a esperar el alta** (`NeedsEnrollment`) cuando un paso falla con la identidad rechazada:
  sigue nada más terminar el alta, pero cambia el arranque para ganar 30 s como mucho.
- **Guardar la marca:** descartado por el propietario (R0).

## R7. El alta sin vaciar el directorio de estado

**Decisión:** el almacén y el alta no cambian, salvo la marca. Ya hoy, `enroll()` escribe una generación nueva en el
directorio del proxy (Envoy la toma sin reiniciarse) con su clave de firma y sus metadatos, borra la renovación en
curso, quita las generaciones viejas y refresca las publicaciones con la identidad nueva, que sustituyen a las copias
anteriores. La base no se toca (FR-006). Lo único que impedía el alta era `problem()`, que con la marca ya dice que la
identidad no vale.

**La contraseña de la base tras el alta** (decisión al analizar): `hermes serve` la vuelve a pedir en cuanto termina
cada alta. Una tarea de fondo de `main.py`, `_password_after_enrollments`, espera cada alta con `wait_for_enrollment` y
llama a `_refresh_password`, la misma petición que el refresco de cada 240 s, que solo actúa con la base ya conectada
(tras un reinicio, la pide el paso del arranque). Si falla, deja `secrets.refresh.failed` con el tipo del error, como
ese refresco, que la vuelve a pedir después.

- **Por qué:** mientras la identidad no vale, la primera petición de la contraseña que pregunta a Hécate falla con
  `auth`, la rechace Hécate o la bloquee Hermes, y la caché quita el valor (R4). Las conexiones abiertas siguen, pero
  una nueva toma la contraseña de la caché sin esperar a nadie: sin ella, si PostgreSQL cortó las conexiones o hacen
  falta más, la salud diría `database unavailable` y los envíos fallarían hasta el refresco siguiente, hasta 240 s
  después del alta, más de los 2 minutos de SC-003. Los demás secretos se piden al usarse, así que ya los pide la
  identidad nueva. Antes del arreglo no pasaba, porque recuperarse exigía reiniciar Hermes, y el arranque la pide.
- **Alternativas:**
  - **Aceptarlo y documentarlo:** SC-003 dejaría de cumplirse en ese caso.
  - **Que una conexión nueva use la última contraseña resuelta** cuando la caché no la tiene: usaría un valor que
    Hécate ya no da, por ejemplo tras una rotación.
  - **No quitar el valor ante la llamada bloqueada:** contradice R4 y FR-008, que piden fallar como ante un rechazo de
    Hécate.
  - **Un aviso de alta nuevo en el servicio de identidad:** `wait_for_enrollment` ya avisa al arranque, y basta con
    otra tarea que lo espere.

## R8. Pruebas

TDD en cada nivel. Las pruebas de hoy siguen pasando; la de la renovación rechazada pasa a comprobar también que no
vuelve a llamar.

- **`FakeHecate`** gana lo que hace Hécate tras una revocación urgente: `refused`, las series cuyas llamadas
  autenticadas rechaza con `auth` (y que van también a su lista de revocación), y un recuento de las llamadas
  autenticadas. Sin series rechazadas, se comporta como hoy.
- **Unitarias del servicio de identidad** (`test_identity_state.py`):
  - un refresco rechazado deja la identidad no válida, con `needs a new enrollment token` (FR-001);
  - lo mismo con cada otro camino: la resolución, una clave, la lista de acceso y la renovación (historia 1, escenario
    6);
  - `unavailable`, `internal`, Hécate sellada o caída y la CA sustituida no la marcan, y la CA sustituida no bloquea
    (FR-003, FR-009);
  - tras la marca, ninguna llamada llega a `FakeHecate` y cada una falla con `auth` (FR-008, SC-006);
  - una línea `identity.refused` con la serie y la llamada, una sola vez (FR-005);
  - el alta con un token nuevo funciona sin tocar el almacén, y la identidad nueva vale y vuelve a llamar (FR-006);
  - un rechazo que llega de una llamada en curso cuando un alta ya ha sustituido la identidad no cuenta (FR-004);
  - un servicio nuevo sobre el mismo almacén, como tras un reinicio, vale hasta que Hécate le rechaza su primera
    llamada (FR-007).
- **Unitarias de la caché de secretos** (`test_secret_cache.py`): sobre el servicio con la identidad rechazada, la
  resolución falla con `auth`, quita el valor y no llama a Hécate.
- **Integración contra una Hécate real** (`test_client.py`): tras `hecate certificates revoke <serie> --urgent`, Hécate
  responde `auth` en su sobre (no corta el TLS), el servicio marca en la primera llamada y bloquea la siguiente, y un
  token nuevo lo da de alta otra vez sobre los mismos directorios. Cada llamada rechazada deja una fila de Hermes en el
  registro de accesos de Hécate, y la bloqueada no deja ninguna (SC-006). Son las filas de la tabla `access_log` de su
  `hecate.db`, leída en solo lectura, con `subject` = el SPIFFE ID de Hermes: las órdenes del admin sobre Hermes
  también llevan su identidad, pero con `subject` = `admin`.
- **El alta en `hermes serve`** (`hermes/tests/unit/test_serve_enrollment.py`): tras un alta, la caché vuelve a tener
  la contraseña de la base sin esperar al refresco de cada 240 s (R7).
- **E2E en el compose** (`test_flow.py`): un escenario nuevo antes del de PostgreSQL parado. Revoca con urgencia el
  certificado de Hermes, con la serie leída de su directorio de estado; espera a que la salud diga
  `not ready: needs a new enrollment token` (300 s como mucho: 240 s más margen); un envío por Envoy responde
  `unavailable` sin enviar nada; `hermes enroll` con un token nuevo funciona sin vaciar nada; la salud vuelve a
  `ready`, Envoy presenta el certificado nuevo, sin que se reinicie ningún contenedor, y un envío llega. Alarga el E2E
  hasta unos 4 minutos.
- **El reinicio completo** (el rechazo en el arranque, el alta y el arranque que sigue) se recorre a mano, en el paso 9
  del apartado 3 del quickstart: lo automático cubre el servicio de identidad, y el arranque no cambia (R6).
- **Calidad:** ruff, el formato, mypy estricto y pip-audit, sin avisos.

## R9. La suite en Linux

**Decisión:** las pruebas que no necesitan Docker, en un contenedor Linux con la imagen de Python de
Hermes y un `HECATE_BIN` compilado para Linux; las que necesitan Docker (el PostgreSQL de usar y tirar, Envoy y el
compose), desde macOS, donde esos servicios ya corren en contenedores Linux y el E2E ejecuta Hermes en Linux.

- **El binario de Hécate para Linux:** `cargo build --release` en `rust:1.99.1-bookworm`, con su directorio de
  compilación fuera del repositorio; corre en la imagen de Python, basada en una Debian más nueva.
- **La pasada:** en `python:3.15.2-slim-trixie` con uv, sobre una copia del repositorio dentro del contenedor, para no
  escribir en el `.venv`, las cachés ni los `__pycache__` de macOS; `-m "not docker"`.
- **Las órdenes** están en el [quickstart](quickstart.md#1-pruebas-y-calidad).

**Alternativas:**

- **Toda la suite dentro de un contenedor**, con el socket de Docker y la red del anfitrión: completa, pero frágil con
  Colima (las rutas de los volúmenes del compose y los puertos de los contenedores hermanos).
- **Como en la 008 a la 010:** unitarias e integración en macOS y solo el E2E en Linux; no cumple «la suite en macOS y
  Linux».

## R10. Documentos

Al cerrar (FR-010):

| Documento | Qué cambia |
|---|---|
| `docs/hermes/operacion.md` | «La identidad de Hermes deja de valer»: cuándo (también la revocación urgente, que Hermes nota en la primera llamada rechazada, 240 s como mucho), qué pasa mientras (no está listo, la API responde `unavailable`, deja de llamar a Hécate con esa identidad y su log dice `identity.refused`), los pasos sin parar Hermes ni vaciar su directorio de estado, la identidad deshabilitada, que se habilita antes de crear el token, y el desfase de relojes, que se arregla con el reloj y un reinicio. En las tareas de fondo de `hermes serve`, la contraseña de la base también se pide tras cada alta. En el compose de pruebas se retira la fila para vaciar el directorio de estado, que solo usaba ese paso |
| `docs/hermes/cli.md` | El motivo `needs a new enrollment token`: Hécate rechazó una de sus llamadas, como tras una revocación urgente |
| `docs/hermes.md` | «Ante fallos»: la fila de la identidad, con la revocación urgente notada como mucho a los 240 s y el alta sin pararlo |
| `docs/hermes/seguridad.md` | «La identidad de Hermes»: una identidad rechazada deja de valer y Hermes deja de llamar con ella. «Lo que no cubre»: la revocación urgente pasa de no notarse a notarse como mucho a los 240 s, con lo que sigue atendiendo hasta entonces |
| `hermes/README.md` | Lo que recorre el E2E: la revocación urgente de Hermes y su alta nueva |
| `DECISIONS.md` | Se retira la fila «Revocación urgente de Hermes» de las abiertas |

- **No cambian:** ARCHITECTURE, GLOSSARY (la revocación urgente ya está definida), los documentos de Hécate, la
  constitución ni ningún ADR.
- **La fila de DECISIONS** es un diseño sin construir, no una decisión abierta: retirarla no resuelve ninguna decisión
  de forma implícita.

## R11. Sin tecnología nueva

Ninguna dependencia, variable de entorno, tabla, fichero ni contrato nuevos. La constitución (1.2.1) y los ADR no
cambian.
