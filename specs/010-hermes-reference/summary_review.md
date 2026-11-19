# Guía de revisión: la documentación de Hermes

Rama `docs/hermes-reference`. Guía para revisar el cambio: por dónde empezar, cómo comprobarlo y qué conviene saber.
Es un cambio de documentación: no toca código, configuración ni contratos.

## Por dónde empezar a revisar

Se va de lo que decide a lo que se escribió:

1. **La spec** ([spec.md](spec.md)). Seis historias (entender Hermes, integrar un componente, la CLI, la operación,
   añadir una librería y el desarrollo), 19 requisitos y 8 criterios de éxito. Las 3 aclaraciones fijan el reparto:
   - nueve documentos en `docs/hermes/`, con llamadas y comunicación separados;
   - la referencia de la CLI se llama «CLI y variables de entorno de Hermes» (`cli.md`);
   - los procedimientos de operación usan las órdenes de `hermes` y de `hecate`, y un apartado dice cómo se ejecutan en
     el compose de pruebas, sin describir cómo corre Hermes en el home lab.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md) y
   [quickstart.md](quickstart.md)):
   - tus decisiones (R0), entre ellas el bot de Telegram de pruebas, con su token en
     `hermes/deploy/.secrets/telegram-test.env` y solo para el happy path;
   - la estructura de los nueve documentos (R1) y el esquema del documento de alto nivel (R2);
   - el mapa de cada apartado del documento anterior a su destino (R3), que es la base de «nada se pierde»;
   - lo que lleva cada documento de uso y el de seguridad (R4 a R8), el estado (R9) y la constitución, sin enmienda
     (R10);
   - cómo se comprueba (R11) y los datos que no coinciden con el código (R12).
3. **Los documentos**, en este orden:
   - [docs/hermes.md](../../docs/hermes.md): el documento de alto nivel (2.376 palabras, antes 6.359);
   - los de concepto: [contratos](../../docs/hermes/contratos.md), [llamadas](../../docs/hermes/llamadas.md),
     [comunicación](../../docs/hermes/comunicacion.md), [configuración](../../docs/hermes/configuracion.md),
     [proveedores](../../docs/hermes/proveedores.md) y [seguridad](../../docs/hermes/seguridad.md);
   - los de uso: [integración](../../docs/hermes/integracion.md), [CLI y variables de entorno](../../docs/hermes/cli.md)
     y [operación](../../docs/hermes/operacion.md), y el [README de desarrollo](../../hermes/README.md). Son los que más
     necesitará quien llame a Hermes, lo opere o lo retome.
4. **Lo que cambia fuera de los documentos de Hermes:** en [DECISIONS](../../DECISIONS.md#abiertas), los enlaces de las
   filas de `chat.ask`, las tasks y los grupos de contactos llevan a comunicación y a llamadas, sin cambiar su texto, y
   dos filas nuevas recogen los hallazgos (la revocación urgente de Hermes y el reintento de Hades). No cambian
   ARCHITECTURE, GLOSSARY ni el README: ninguno enlazaba a un apartado de `docs/hermes.md`, y la línea de Hermes del
   estado ya decía lo que hace.

## Cómo comprobarlo

Desde la raíz del repositorio, los apartados de [quickstart.md](quickstart.md):

- **1 y 2:** las 6 órdenes y las 11 variables están en la referencia de la CLI, y ningún documento nombra otras;
- **3:** las 3 operaciones del OpenAPI están en la guía de integración, y ninguna referencia de uso nombra otra ruta;
- **4 y 5:** los enlaces y las anclas llevan a sitios que existen, y solo cambian documentos Markdown;
- **6:** DECISIONS ya no enlaza a apartados de `docs/hermes.md`, y el README enlaza al documento de alto nivel;
- **8:** ningún documento lleva el token del bot de pruebas;
- **11:** el documento de alto nivel no pasa de 2.500 palabras, y ningún documento usa palabras de tiempo.

Todos pasan. El 7 (nada se pierde), el 9 (el recorrido) y el 10 (el README de desarrollo) se hicieron con scripts que
viven fuera del repositorio (FR-001): sus resultados están abajo.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit: `docs/hermes.md`, `docs/hermes/`, `hermes/README.md`, `DECISIONS.md` y
  esta carpeta.
- **Resultados:**
  - nada se pierde: cada apartado y cada dato del `docs/hermes.md` de `main` (146 comprobaciones sobre el texto, que
    sobre `main` fallan) está en su destino, y la lectura de cada apartado frente a su destino lo confirma;
  - el recorrido, siguiendo `operacion.md` tal como está escrito, con el compose de `hermes/deploy/`:
    - con el fake, 35 comprobaciones: la puesta en marcha, verificar un canal, conceder y retirar el acceso (la
      retirada valió a los 235 s), Hécate sellada y caída, PostgreSQL caído y arrancar sin copia de la lista de acceso;
      la identidad revocada, con los pasos corregidos (ver los hallazgos);
    - el happy path con tu bot de pruebas: el canal verificado con «Canal verificado.» tras enviar tú el código, el
      envío del cliente de referencia por Envoy (`completed`, `provider_message_ref` 28) y su repetición con la misma
      clave sin un segundo envío; confirmaste que llegaron los dos mensajes;
    - rotar el token y una instancia que falla, con mocks, como decidiste: pasan las 53 pruebas de Telegram grabado, de
      la prueba de conexión y de la caché de secretos, y las 12 del estado de una instancia;
  - siguiendo solo `hermes/README.md`: `cargo build --release` de Hécate, la suite con `HECATE_BIN` y Docker (866
    pruebas, E2E incluido, ninguna saltada), ruff, el formato, `mypy --strict` y pip-audit. Es el resultado de la 009:
    el código no cambia.
- **Hallazgos que contradicen una spec o un diseño fusionados** (no se corrigen aquí, R12):
  - **Hermes no detecta su propia revocación urgente** (FR-010 de la 008). Tras `hecate certificates revoke <serie>
    --urgent`, Hécate le rechaza el refresco de las publicaciones (`publications.refresh_failed` con `auth`), así que
    nunca recibe la lista de revocación con su serie; solo lo nota cuando Hécate rechaza su renovación, a los dos
    tercios de la vida del certificado, y mientras `hermes enroll` responde `conflict`. Los documentos describen ese
    comportamiento y el camino que funciona (parar Hermes, vaciar su directorio de estado, arrancarlo y darlo de alta
    otra vez, comprobado en el recorrido). Propuse el arreglo como un cambio aparte, después de la 010, y la fila
    «Revocación urgente de Hermes» de «Abiertas» lo recoge.
  - **El reintento de Hades con la misma clave.** `docs/hades.md` deriva la clave de idempotencia del paso y su input
    y reintenta un bloque si el sobre dice `retryable`; Hermes guarda en la clave toda respuesta que llegó al proveedor,
    también un fallo reintentable, así que ese reintento recibiría el fallo guardado. Queda para tu decisión: que
    Hermes no guarde los fallos que garantizan que no se envió nada, o que Hades cambie la clave en cada intento. La
    fila «Reintento de Hades tras un fallo reintentable de Hermes» de «Abiertas» lo recoge.
- **Los datos que cambian por no coincidir con el código** (el documento pasa a describir el código, FR-004):
  - un fallo del proveedor va en un 200 dentro del sobre, también su `rate_limited` (con `retry_after_s`, sin la
    cabecera `Retry-After`) y su `conflict` (la persona no admite el bot);
  - `provider_unavailable` no siempre es `retryable`: Telegram lo da sin reintento cuando rechaza las credenciales o la
    petición;
  - una respuesta que llegó al proveedor queda en su clave, también un fallo, así que un fallo `retryable` del
    proveedor se reintenta con otra clave; lo que falla antes del proveedor se repite con la misma;
  - el token corto lleva en su cabecera el `kid` de la clave de firma de llamadas: Hermes busca la clave por él. El
    documento anterior y el `api.md` de la 008 remitían al formato de la guía de Hécate, que no lo lleva, y la
    descripción de `callToken` en `hermes.v1.json` tampoco lo dice;
  - una aplicación sin cambios no escribe nada, tampoco en la auditoría;
  - Hermes prueba sus instancias al arrancar, no al aplicar la configuración; y una prueba que funciona deja la
    instancia en `degraded` mientras haya un fallo entre sus diez llamadas últimas, o en `unavailable` si fallaron las
    tres últimas;
  - la prueba de conexión de una instancia es una operación de la API, para componentes con acceso: la CLI no tiene
    una orden para ella (R12);
  - los diagramas de secuencia llevaban `;` dentro de los mensajes, que Mermaid toma como fin de sentencia: no se
    dibujaban.
- **Tus decisiones durante la implementación:** aprobaste cada fase, hiciste el pulido antes del happy path mientras
  faltaba el fichero del bot, y enviaste el código para el recorrido completo.
- **Decisiones mías durante la implementación:**
  - la función de anclas del apartado 4 de quickstart.md quitaba los `_`, y GitHub los conserva: se corrigió;
  - la fila de la identidad revocada del apartado 9 pasa a los pasos que funcionan;
  - los enlaces de DECISIONS conservan la etiqueta `hermes`, porque el texto de las filas no cambia;
  - las órdenes del compose usan `COMPOSE_FILE` desde la raíz y `docker compose`, con la nota de `docker-compose`, que
    es lo que hay en tu equipo; ver el plan antes de aplicar es lanzar la orden sin `--yes`, porque `docker compose cp`
    deja el fichero ilegible para el usuario de Hermes;
  - datos nuevos, contrastados con el código: los límites de los textos de una plantilla y la validación al arrancar
    (configuración), lo que hace quien llama con cada código (integración), los errores de cada orden, los motivos de
    la salud y las órdenes que esperan la base (CLI), y el acceso local al equipo como algo que la lista de acceso no
    cubre (seguridad);
  - el campo `mode` del manifiesto se describe con sus dos valores de diseño, `sync` o `task`;
  - en las referencias, los enlaces largos van con etiqueta, para que ninguna línea pase de 120 caracteres.
- **Leído, no ejecutado:** «Restaurar una copia» (cada paso existe, y lo que falta remite a DECISIONS) y la identidad
  caducada o de otra CA (siguen los pasos de la revocada, sin vaciar el directorio de estado).
- **Lo que encontró converge (fase 10):** nada sin escribir; tres precisiones, ya corregidas:
  - **Los permisos del directorio del proxy:** Hermes lo crea como el de estado, 0700 y cada `gen-<n>/` 0700, con
    ficheros 0600; lo dicen la referencia de la CLI y la tabla de datos de seguridad (FR-007).
  - **Una repetición:** el procedimiento de una instancia que falla repetía qué dice cada estado, que desarrolla la guía
    de integración; ahora la enlaza (FR-003).
  - **Un enlace:** la pregunta frecuente de la salud enlaza a la salud de la CLI, como las demás (R2).

  Los apartados 1 a 8 y 11 del quickstart y las 146 comprobaciones de «nada se pierde» siguen pasando.
- **Lo que encontró el segundo converge (fase 11),** al releer las afirmaciones absolutas de todos los documentos, ya
  corregido:
  - **La regla de los nombres** de configuración decía que no se repiten los proveedores; sí se repiten, en cada una de
    sus instancias. No se repiten las variables, las instancias, las plantillas, los contactos ni las versiones de una
    plantilla, ni un contacto tiene dos canales en la misma instancia.
  - **El cliente de referencia** no hace todo lo de la guía de integración: hace el alta, el token corto de cada
    llamada y el envío con su clave, sin capacidades ni prueba de conexión.

  Las comprobaciones siguen pasando.
- **Lo que encontró el tercer converge (fase 12):** seguridad no decía, entre lo que no cubre, que Hermes no nota la
  revocación urgente de su propio certificado hasta que Hécate rechaza su renovación, como ya decían operación y el
  documento de alto nivel. Lo dice ahora como un hecho, sin presentarlo como riesgo aceptado, porque esa decisión es
  tuya con el arreglo propuesto. Las comprobaciones siguen pasando.
- **Lo que encontró el cuarto converge (fase 13):** los dos hallazgos solo estaban en esta guía. R9 manda añadir a
  «Abiertas» una pieza de Hermes sin construir que aparezca al implementar, y la constitución, que DECISIONS recoja lo
  abierto: las dos filas nuevas lo hacen. Las comprobaciones siguen pasando.
- **Fuera de alcance**, como fija la spec: cambiar el código, los contratos o el comportamiento de Hermes (también la
  descripción del `kid` y los dos hallazgos), modificar las specs fusionadas y reestructurar los docs de otros
  componentes.

## Dónde mirar con más sospecha

- **La guía de integración:** la tabla de qué hace quien llama con cada código y las reglas de la `Idempotency-Key`. Es
  lo que seguirá Hades sin abrir el código, y es donde se nota el hallazgo del reintento con la misma clave.
- **El procedimiento de la identidad que deja de valer:** describe un rodeo hasta que se arregle el código.
- **La referencia de la CLI:** las salidas y los errores salen del código y de la ayuda; si la CLI cambia, la
  referencia se queda atrás.
- **El estado:** que los documentos de diseño cuenten las tasks, `chat.ask`, los grupos, el cifrado de las copias con
  `wrap` y los checkpoints como diseño, sin decir que faltan, y que DECISIONS lo diga.
- **«Nada se pierde»:** el mapa de R3 y los datos de arriba son donde un dato del documento anterior pudo cambiar de
  sentido.

## Después de la revisión

El quinto `/speckit-converge` salió limpio. T027 propone el mensaje del commit de squash `docs:` y la descripción de la
PR, con los hallazgos y el resultado del recorrido. El commit, el push y la PR los haces tú.
