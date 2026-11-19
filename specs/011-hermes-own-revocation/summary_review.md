# Guía de revisión: Hermes nota la revocación urgente de su propio certificado

Rama `fix/hermes-own-revocation`. Guía para revisar el arreglo: por dónde empezar, cómo probarlo y qué conviene saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Tres historias (Hermes deja de estar listo tras la revocación urgente de su
   certificado, el admin le da una identidad nueva sin vaciar su directorio de estado, la operación describe lo que
   hace), 10 requisitos y 6 criterios de éxito. Las 3 aclaraciones fijan el alcance:
   - cuenta el rechazo de cualquier llamada que Hermes hace a Hécate con su identidad, no solo el del refresco;
   - el rechazo vive solo en memoria: tras un reinicio, la primera llamada del arranque lo vuelve a comprobar;
   - una identidad rechazada sigue sin valer hasta un alta nueva o un reinicio, y mientras tanto Hermes no llama a
     Hécate con ella.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md) y
   [contratos](contracts/README.md)). Ningún contrato cambia, y no hay ADR, dependencia, variable, tabla ni fichero
   nuevos (R11). R1 explica la causa, reproducida en el recorrido de la 010. Lo que decidiste:
   - al revisar el plan: un solo paso para las llamadas con la identidad de Hermes, con el port `SecretResolver` (R2);
     qué cuenta como rechazo y a qué identidad (R3); la llamada bloqueada, que responde `auth` (R4); la línea
     `identity.refused` (R5); el arranque sin cambios (R6) y la pasada en Linux (R9);
   - al analizar: que Hermes vuelva a pedir la contraseña de su base en cuanto termina cada alta (R7);
   - al implementar: adelantar T022 y T024, esa contraseña, al checkpoint de US1.

   Lo demás: las pruebas (R8) y los documentos (R10).
3. **El código**, en `hermes/src/hermes/` y en este orden:
   - `application/identity.py`: `_as_hermes` y `_refuse`, la marca `_refused` en `problem()` y en `enroll()`, y el
     `resolve` nuevo. Es el fichero más importante;
   - `application/ports.py`: el port `SecretResolver`, con solo `resolve`;
   - `application/secrets.py` y `application/configuration.py`: `SecretCache` y `HecateSecretChecker` dependen de
     `SecretResolver` en lugar de `HecateApi`, sin otro cambio;
   - `infrastructure/main.py`: la caché y la comprobación reciben el servicio de identidad, y la tarea de fondo
     `_password_after_enrollments`.
4. **Lo que toca código de las features anteriores**, que ya está en `main`:
   - la marca de rechazo pasa de un booleano a la serie de la identidad rechazada, y ya no la pone `renewal_tick` sino
     `_as_hermes`, en cualquier llamada;
   - las llamadas de la renovación, las publicaciones, una clave y la lista de acceso pasan por `_as_hermes`; cada
     camino sigue fallando y dejando su línea como antes;
   - `hermes serve` lanza una tarea de fondo más, `secrets.enrollment`.

   No se ha quitado ni rebajado ninguna prueba anterior. Lo que cambia en sus ficheros:
   - `test_a_renewal_refused_with_auth_needs_a_new_token` comprueba además que el siguiente `renewal_tick` no vuelve a
     llamar a Hécate;
   - `FakeHecate` gana `revoke_urgently`, `refused`, `authenticated_calls` y `hold_next`; sin series rechazadas ni
     llamadas retenidas, responde como antes, y una prueba lo fija;
   - el E2E busca en el log con un ayudante nuevo, `logged`, sobre el que pasa `refusals`; el escenario nuevo va entre
     el del certificado revocado de un componente y el de PostgreSQL parado, y termina con Hermes dado de alta otra vez
     para que sigan los demás.
5. **Las pruebas:** 907 en Hermes (siguen las 866 de partida y se suman 41). Por fichero:
   - `hermes/tests/unit/application/test_identity_state.py`, 34 casos de 16 pruebas: las del doble (10); cada camino
     que marca en la primera llamada rechazada (6); lo que no marca, con cada código que no es `auth`, una hora de
     Hécate sellada y otra caída, la revocación urgente de otro componente y la CA sustituida (12); el bloqueo, la línea
     del log, el reinicio, el alta tras el rechazo (2) y el rechazo atrasado;
   - `hermes/tests/unit/application/test_secret_cache.py` (1): con la identidad rechazada, la caché da el valor
     guardado en su tiempo blando y después falla sin preguntar a Hécate;
   - `hermes/tests/unit/test_serve_enrollment.py` (2, fichero nuevo): tras un alta, `hermes serve` vuelve a tener la
     contraseña de la base sin esperar al refresco, y si Hécate no responde lo dice en el log y espera a la siguiente;
   - `hermes/tests/integration/hecate/test_client.py` (3), contra una Hécate real: tras `hecate certificates revoke
     <serie> --urgent`, Hécate responde `auth`, Hermes marca y deja de llamar, cada llamada rechazada deja una fila de
     Hermes en el registro de accesos de Hécate y la bloqueada ninguna; un Hermes arrancado otra vez sobre los mismos
     directorios lo nota en su primera llamada; y un token nuevo da de alta al mismo Hermes sin vaciarlos;
   - `hermes/tests/e2e/test_flow.py` (1): la revocación urgente en el compose, la salud, un envío que responde 503 sin
     enviar nada, el alta nueva, Envoy con el certificado nuevo sin reiniciar ningún contenedor y un envío que llega.

## Cómo probarlo

Las órdenes del apartado 1 de [quickstart.md](quickstart.md): en Hermes ruff, el formato, `mypy --strict`, pip-audit y
la suite entera en macOS, con el binario release de Hécate y Docker:

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

En Linux, las pruebas sin Docker en el contenedor `python:3.15.2-slim-trixie`, con el binario de Hécate para Linux que
compila la primera orden de ese apartado. El apartado 2 tiene las pruebas del arreglo solas; el 3, el recorrido en el
compose de pruebas con el proveedor fake, y el 4, las comprobaciones de los documentos.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, investigación, modelo, contratos, tareas, checklist y esta guía;
  - el código y las pruebas de Hermes;
  - los docs consolidados: `docs/hermes.md`, `docs/hermes/operacion.md`, `cli.md` y `seguridad.md`,
    `hermes/README.md` y `DECISIONS.md`, que pierde la fila «Revocación urgente de Hermes».
- **Resultados:**
  - la línea base, sobre `main` (`f11cd29`): 866 pruebas en macOS;
  - Hermes en macOS: 907 pruebas con el binario release de Hécate, Docker y el E2E, sin ninguna saltada; ruff, el
    formato, `mypy --strict` y pip-audit, sin avisos ni vulnerabilidades;
  - Hermes en Linux, en el contenedor `python:3.15.2-slim-trixie` con el binario de Hécate para Linux: 831 pruebas,
    todas las que no necesitan Docker, sin ninguna saltada; las 76 de Docker son las que pasan desde macOS;
  - el recorrido del apartado 3 (9/9) y las comprobaciones del apartado 4 dieron lo esperado.
- **Lo que se vio en el recorrido** (apartado 3, en el compose de pruebas con el fake, siguiendo solo la operación):
  - la salud dijo `not ready: needs a new enrollment token` a los 228 s de la revocación (SC-001);
  - una sola línea `identity.refused`, con la serie y `operation: revocations`;
  - 3 filas de Hermes en el registro de accesos de Hécate en dos copias tomadas con 250 s de diferencia: no crece
    (SC-006);
  - un envío respondió 503 y la bandeja de salida no cambió (SC-002);
  - `hermes enroll` escribió la serie nueva sin parar nada ni vaciar el directorio de estado; `ready` al momento, un
    envío llegó al canal verificado de antes y no se reinició ni Hermes ni Envoy (SC-003);
  - tras revocar el certificado nuevo y reiniciar Hermes, la salud dio el motivo al segundo, por el rechazo de la
    contraseña de la base (`operation: resolve`), y otra alta lo dejó listo a los 31 s, el reintento del arranque cada
    30 s;
  - ningún paso del documento necesitó corrección (SC-005).
- **Lo que solo se recorre a mano:** el reinicio completo de FR-007 (el rechazo en el arranque, el alta y el arranque
  que sigue), en el paso 9. Lo automático cubre el servicio de identidad sobre el mismo almacén, como tras un reinicio,
  y la integración con un Hermes nuevo sobre los mismos directorios; el arranque no cambia (R6).
- **El TDD:** cada prueba falló antes de su implementación, salvo las de T018, T020 y T021:
  - las del doble, antes de T006;
  - las unitarias y de integración de US1, porque faltaban `resolve` y la marca;
  - el E2E de T013 sobre el código de partida: a los 300 s, Hermes seguía `ready`;
  - T022, porque faltaba `_password_after_enrollments`;
  - T019, con una segunda línea `identity.refused` del rechazo atrasado, hasta T023.

  Esas tres pasaron a la primera: el alta tras el rechazo ya funcionaba con la marca de US1, porque `problem()`
  deja de ser `None` y `enroll()` deja de responder `conflict`, y T021 necesitaba además la contraseña de T024, ya
  adelantada. No esconden un fallo: comprueban lo que US1 ya hizo posible.
- **Tus decisiones durante la implementación:**
  - adelantar T022 y T024 al checkpoint de US1: la primera suite completa falló en el E2E de PostgreSQL parado (500 en
    vez de 503), porque tras el alta de T013 la caché no tenía la contraseña de la base para una conexión nueva;
  - aprobaste US1, US2 y US3 en sus puntos de revisión.
- **Decisiones mías durante la implementación**, en las pruebas:
  - una tabla `ROUTES` con cada camino que marca, cómo falla como antes y qué línea deja, que comparten las pruebas de
    los caminos, del bloqueo y de la línea del log;
  - la integración cuenta las filas del registro de accesos por el `subject` y también por la serie, en la columna
    `certificate`, para que cada cuenta sea la de un certificado;
  - el E2E comprueba que no se reinicia ningún contenedor con la hora de arranque y el recuento de reinicios de
    `docker inspect`.
- **Los tiempos son de un Mac de desarrollo:**
  - la suite de Hermes tarda unos 9 min, de 3 a 4 más que la de partida (5 min y medio), por el escenario nuevo del
    E2E, que espera un refresco de 240 s como mucho;
  - en el contenedor Linux, las 831 pruebas tardan unos 19 s, más lo que tarda en instalar uv y las dependencias.
- **Fuera de alcance**, como fija la spec: cambiar Hécate o su API; distinguir un desfase de relojes de una
  revocación; la revocación de otros componentes, que ya funciona; y que Hermes renueve antes o refresque más a menudo.
  Hasta el refresco siguiente, 240 s como mucho, Hermes sigue atendiendo con el certificado revocado, como dice
  [seguridad](../../docs/hermes/seguridad.md).

## Dónde mirar con más sospecha

- **La atribución del rechazo (`_as_hermes`):** toma la identidad vigente antes de llamar y solo marca si sigue siendo
  la vigente al llegar el rechazo. Compara el objeto (`self._identity is identity`), que cambia con cada alta y cada
  renovación; el almacén solo se carga al arrancar. Depende de que el cliente de Hécate lea la identidad al empezar
  cada llamada, sin esperar nada antes (`_authenticated`), así que la que se tomó es la que firma. `FakeHecate` hace lo
  mismo y las pruebas fijan la atribución con una llamada retenida durante un alta; en el cliente real es una regla
  del código, que un `await` añadido antes de esa lectura rompería.
- **`HecateTrustChanged`:** hereda de `HermesError` con el código `auth`; su `except` va antes que el de
  `HermesError`, así que la CA sustituida no marca ni bloquea, y Hermes vuelve a estar listo si Hécate se presenta
  otra vez con la de siempre.
- **El bloqueo:** con la serie vigente marcada, ninguna llamada sale; el alta no pasa por `_as_hermes`, porque usa el
  token del admin. La caché de secretos quita el valor ante la llamada bloqueada, como ante un rechazo de Hécate
  (FR-008), y por eso hace falta la contraseña tras el alta.
- **La contraseña tras cada alta (`_password_after_enrollments`):** cada vuelta espera la siguiente alta, porque
  `wait_for_enrollment` crea una espera nueva en cada llamada y el alta despierta las que hay. Solo actúa con la base
  ya conectada; tras un reinicio, la pide el paso del arranque. Un fallo deja `secrets.refresh.failed` con el tipo del
  error, como el refresco de cada 240 s, y la tarea sigue. Una segunda alta mientras se pide la contraseña de la
  primera no la vuelve a pedir: la pide el refresco de cada 240 s. Para eso, la identidad de la primera alta tendría
  que dejar de valer en lo que tarda esa petición.
- **El reinicio:** la marca no se guarda. El paso del arranque que pide la contraseña falla con `auth`, la marca vuelve,
  y el reintento cada 30 s sigue con la identidad nueva tras el alta.

## Después de la revisión

- **El primer `/speckit-converge`** añadió la fase 7, ya hecha:
  - T036: el procedimiento «La identidad de Hermes deja de valer» dice ahora que se espere al motivo de
    `hermes health` antes del alta, porque tras una revocación urgente `hermes enroll` responde `conflict` («hermes
    already has a valid identity») hasta que Hermes nota el rechazo. Es solo documentación: que el alta preguntara a
    Hécate antes de responder `conflict` cambiaría lo que fija la spec (historia 2, escenario 5). Lo comprobé con un
    guion fuera del repositorio, sobre el servicio y el doble de Hécate, y las comprobaciones del apartado 4 siguen
    dando lo esperado;
  - T037: la tabla «Las specs» de `hermes/README.md` gana la fila de la 011.
- La segunda pasada de `/speckit-converge` salió limpia, y T035 deja propuestos el mensaje del commit de squash y la
  descripción de la PR.
