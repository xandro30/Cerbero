# Guía de revisión: Hécate documentada antes de pausarla

Rama `docs/hecate-reference`. Guía para revisar el cambio: por dónde empezar, cómo comprobarlo y qué conviene saber.
Es un cambio de documentación: no toca código, configuración ni manifiestos.

## Por dónde empezar a revisar

Se va de lo que decide a lo que se escribió:

1. **La spec** ([spec.md](spec.md)). Seis historias (entender Hécate, integrar un componente, la CLI, la operación, el
   desarrollo y el estado), 19 requisitos y 10 criterios de éxito. Las 4 aclaraciones fijan el alcance:
   - el estado de lo diseñado y sin construir vive solo en DECISIONS y en el README; los documentos de diseño no dicen
     nada de su estado, y las referencias de uso solo describen lo que existe;
   - la seguridad tiene su propio documento;
   - la guía de integración no lleva código;
   - no se describe ningún gestor de servicios: cómo corren los componentes en el home lab pasa a DECISIONS.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md) y
   [quickstart.md](quickstart.md)):
   - la estructura y los nombres de los nueve documentos (R1) y el esquema del documento de alto nivel (R2);
   - el mapa de cada apartado del documento anterior a su destino (R3), que es la base de «nada se pierde»;
   - lo que lleva cada documento de uso (R4 a R8), el estado (R9) y la constitución (R10);
   - cómo se comprueba: comprobaciones automáticas y un recorrido con el binario (R11);
   - los datos del documento anterior que no coincidían con el código (R12).
3. **Los documentos**, en este orden:
   - [docs/hecate.md](../../docs/hecate.md): el documento de alto nivel (2.433 palabras, antes 6.486);
   - los de concepto: [sellado](../../docs/hecate/sellado.md), [secretos](../../docs/hecate/secretos.md),
     [identidades](../../docs/hecate/identidades.md), [acceso](../../docs/hecate/acceso.md),
     [auditoría](../../docs/hecate/auditoria.md) y [seguridad](../../docs/hecate/seguridad.md);
   - los de uso: [integración](../../docs/hecate/integracion.md), [CLI y configuración](../../docs/hecate/cli.md) y
     [operación](../../docs/hecate/operacion.md), y el [README de desarrollo](../../hecate/README.md). Son los que
     más necesitará quien retome Hécate o construya el siguiente componente.
4. **Lo que cambia fuera de Hécate:**
   - [DECISIONS](../../DECISIONS.md#abiertas): seis filas de Hécate «Diseñado, no implementado» y la del despliegue en
     el home lab;
   - [README](../../README.md): una casilla en «Estado del Proyecto» con lo que Hécate ya hace;
   - la [constitución](../../.specify/memory/constitution.md), en la 1.2.1: los documentos de detalle en
     `docs/<componente>/`;
   - [ARCHITECTURE](../../ARCHITECTURE.md): dos enlaces que llevan a la auditoría y a la operación de Hécate;
   - [GLOSSARY](../../GLOSSARY.md): dos entradas corregidas (ver «Decisiones mías»).

## Cómo comprobarlo

Desde la raíz del repositorio, los apartados de [quickstart.md](quickstart.md):

- **1 y 2:** las 32 órdenes y las 6 variables están en la referencia de la CLI, y ningún documento nombra otras;
- **3:** las 8 operaciones del OpenAPI están en la guía de integración, y no sobra ninguna ruta;
- **4 y 5:** los enlaces y las anclas llevan a sitios que existen, y no cambia nada fuera de Markdown;
- **6:** las filas de DECISIONS, la línea del README y la constitución 1.2.1;
- **10:** el documento de alto nivel no pasa de 2.500 palabras, y ningún documento usa palabras de tiempo.

Todos pasan. El 7 (nada se pierde), el 8 (el recorrido) y el 9 (el README de desarrollo) se hicieron con scripts que
viven fuera del repositorio (FR-001): sus resultados están abajo.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit: los documentos de `docs/hecate.md` y `docs/hecate/`,
  `hecate/README.md`, `README.md`, `DECISIONS.md`, `ARCHITECTURE.md`, `GLOSSARY.md`, la constitución y esta carpeta.
- **Resultados:**
  - nada se pierde: cada apartado y cada dato del `docs/hecate.md` de `main` (161 entradas) está en su destino; 164
    comprobaciones sobre el texto, que sobre `main` fallan;
  - el recorrido de la operación, con el binario release en macOS y el componente de prueba, siguiendo
    `operacion.md` tal como está escrito: 10 procedimientos y 61 comprobaciones, todas bien;
  - siguiendo solo `hecate/README.md`: compila, la suite pasa en macOS (840 pruebas) y en el contenedor Linux (840),
    y `fmt`, `clippy -D warnings` y `cargo deny` pasan. Es el resultado de la feature 006: el código no cambia.
- **Los datos que cambian por no coincidir con el código** (el documento pasa a describir el código, FR-004). Ninguno
  contradice una spec fusionada:
  - el formato JSON de un secreto cifrado no existe: el material va en columnas de `secret_material` y `key_material`,
    sin campo de algoritmo; los datos asociados llevan además `purpose`;
  - `max_versions` destruye la versión viva más antigua, también una `deprecated`, no solo una `revoked`; el diagrama
    del ciclo de vida lo recoge, con la promoción y la revocación desde `deprecated`;
  - el periodo de gracia (24 h) y los tiempos de caché (300 s y 3600 s) son fijos, y el registro de accesos no se purga:
    no se presentan como configurables;
  - la caducidad (`expires_at`) es informativa: un secreto caducado se sigue resolviendo, como dice la spec 002;
  - Hécate acepta renovar cualquier certificado válido nunca renovado; los dos tercios son la pauta del componente. La
    repetición exacta de un alta o de una renovación devuelve el mismo certificado, y una `Idempotency-Key` con otra
    petición es `conflict`;
  - `secrets list` no muestra huellas, solo `secrets get`; la CLI lee sola la versión esperada antes de cambiar un
    secreto; la copia de la clave de la CA raíz usa el `HECATE_KDF_PROFILE` de la CLI que la escribe;
  - el árbol del apartado 7 nombraba módulos que no existen (cadenas, checkpoints, `wrap`): el README describe los
    crates reales, y el diseño de esas piezas sigue en auditoría y secretos.
- **Tus decisiones durante la implementación** (propuestas que aceptaste al seguir):
  - la cabecera de `docs/hecate.md` ya no dice «Estado: diseñado»: el estado vive en DECISIONS y en el README; los
    demás docs de componente la mantienen;
  - la rotación de la raíz, que tampoco está construida, va en la fila de la rotación de las claves de ámbito, así que
    DECISIONS sigue teniendo siete filas «Diseñado, no implementado»;
  - el apartado 7 se repitió entero al final (T027).
- **Decisiones mías durante la implementación:**
  - el GLOSSARY gana dos correcciones, aunque R13 solo preveía tocarlo si una entrada remitía a un detalle movido:
    sellada responde el estado agregado (feature 006) y la gracia son 24 h fijas. Sin ellas, SC-009 no se cumplía;
  - en la guía de integración, la lista de revocación incluye los certificados revocados de Hécate como un hecho, sin
    decir que el componente de prueba los comprueba: no lo hace, comprueba a Hécate por la huella de la CA raíz;
  - el ejemplo de `hecate status` va en un bloque de texto: escrita entre comillas invertidas, su línea
    `hecate certificate ...` la tomaba por una orden la comprobación del apartado 1;
  - la clave de ámbito comprometida propone, si hace falta una clave nueva, una identidad nueva para el componente: es
    la única forma con lo que existe.
- **Lo que encontró el recorrido:** tras sustituir la CA, un componente sin el alta nueva no recibe `auth`: no conecta,
  porque Hécate se presenta con una CA raíz que él no reconoce. Se corrigió el «Cómo comprobarlo» y se repitió el
  recorrido.
- **Leído, no ejecutado:** «Se pierden la frase y las partes» (es la puesta en marcha tras borrar los datos) y el paso
  3 de la clave de ámbito comprometida. Cada orden que usan existe en la CLI.
- **Lo que encontró converge (fase 10):** nada sin escribir; cuatro precisiones, ya corregidas:
  - **La repetición con otra petición.** En `identidades.md` y en la guía de integración, la misma `Idempotency-Key`
    con otra petición no siempre es `conflict`: con un token ya gastado es `auth`, y con un certificado ya renovado,
    un segundo intento. `conflict` solo llega si otra petición de la misma identidad ya obtuvo un certificado con esa
    clave.
  - **La intermedia ya caducada.** Ningún certificado dura más que la intermedia que lo firma: al caducar dejan de
    valer el de Hécate (la API no escucha, también desbloqueada) y los de los componentes, que vuelven a darse de alta
    tras renovarla. Lo recogen `identidades.md` y el procedimiento de operación. Se contrastó con el código y con las
    pruebas `with_the_intermediate_expired_the_renewal_is_the_way_out` y
    `with_the_intermediate_expired_the_api_stops_listening_also_unsealed`, sin ejecutarlo con el binario: la intermedia
    dura un año.
  - **Una frase de `cli.md`** que no se entendía: el error de un valor que no vale.
  - **Las dos entradas del GLOSSARY**, que el commit y la PR recogen con su motivo.

  Las comprobaciones del quickstart y las 164 de «nada se pierde» siguen pasando.
- **Lo que encontró el segundo converge (fase 11):** una precisión más, ya corregida. El «Cómo comprobarlo» de
  «Arrancar y desbloquear» daba por hecho que, al desbloquear, Hécate renueva su certificado y la API vuelve a
  escuchar; con la intermedia caducada no puede renovarlo, así que ahora remite a «La intermedia va a caducar».
- **Fuera de alcance**, como fija la spec: cambiar el código o el comportamiento de Hécate, documentar aparte el
  protocolo del canal local, modificar las specs fusionadas y reestructurar los docs de otros componentes.

## Dónde mirar con más sospecha

- **La guía de integración:** el formato del token corto, la regla de repetir la misma renovación si no llega la
  respuesta y la tabla de errores. Es lo que seguirá quien construya el siguiente componente, sin abrir el código.
- **La referencia de la CLI:** las salidas salen de una sesión real con el binario de `main`; si la CLI cambia, la
  referencia se queda atrás.
- **Los procedimientos de la pérdida del disco y de la clave de ámbito:** son los que dicen lo que Hécate no hace y lo
  que se puede hacer, con las decisiones que siguen abiertas.
- **El estado:** que los documentos de diseño cuenten Ténaro, `wrap`/`unwrap`, los checkpoints, las identidades hijas y
  la rotación de las claves de ámbito como diseño, sin decir que faltan, y que DECISIONS lo diga.
- **«Nada se pierde»:** el mapa de R3 y las diferencias de arriba son donde un dato del documento anterior pudo cambiar
  de sentido.

## Después de la revisión

El tercer `/speckit-converge` salió limpio. T029 propone el mensaje del commit de squash `docs:` y la descripción de la
PR, que recogen también las dos entradas del GLOSSARY (T033). El commit, el push y la PR los haces tú.
