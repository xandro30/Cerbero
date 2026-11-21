# Research: el contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

Decisiones técnicas del cambio 012. Cada una dice qué se decidió, por qué y qué se descartó. Los nombres concretos de
métodos, mensajes y ficheros se fijan aquí.

## R0. Decisiones del propietario

- **Al especificar (2026-11-19):**
  - el camino 1: la clave solo guarda lo que se envió o pudo enviarse, un envío completado o un resultado desconocido,
    y un fallo sin envío, reintentable o no, la deja libre, como un fallo antes del proveedor;
  - un fallo inesperado de Hermes mientras llama al proveedor responde `timeout` con `outcome_unknown` y queda en la
    clave como un resultado desconocido; `internal` queda para los fallos inesperados antes del proveedor;
  - entran en el cambio el `kid` en el contrato de Hermes y la fila de la 009 en el README de Hécate.
- **Al revisar el plan (2026-11-19):** aprobó la liberación de la clave en una transacción con su auditoría (R2), el
  fallo inesperado como `ProviderFailure` con `timeout` (R3), los textos del contrato 1.0.2 (R4), los documentos y el
  diseño de Hades sin extender la regla a Estigia (R6), el ADR 0014 (R7) y las pruebas, con la de la API sobre
  PostgreSQL y sin escenario nuevo en el E2E (R8).
- **Al revisar el análisis (2026-11-20):** aprobó la remediación de sus 13 hallazgos con las cuatro opciones
  recomendadas:
  - la referencia de la clave en un apartado propio de las desviaciones de DECISIONS, con las dos diferencias del
    borrador de la IETF (R6);
  - la prueba de la API sobre PostgreSQL con el arnés `NetworkApi`, que recibe el repositorio de las claves (R8);
  - la viñeta de `docs/hades.md` en una sola línea, como las demás del fichero (R6);
  - la spec ajustada: un error de Hermes con la clave reservada es un fallo sin envío, la tabla de códigos entra en
    FR-009 y SC-005, y FR-002 deja de repetir lo que dicen FR-001 y FR-003.
- **Al revisar el segundo análisis (2026-11-20):** aprobó sus cinco correcciones: la condición de `conflict` en la
  descripción de `IdempotencyKey` y en el glosario (R4, R6), el texto fijo de la viñeta de `docs/hermes/llamadas.md`
  (R6), el fichero de la prueba que cita T013, el docstring de `CutKeys` y `conflict` e `internal` entre los nombres
  del contrato de la spec.
- **La rama:** `fix/hermes-call-contract`, creada desde `main` (`5e7be30`) por el propietario.

## R1. La causa

Confirmada en el código de `main` (`5e7be30`):

- `_Reserved.after_send` ([send_message.py](../../hermes/src/hermes/application/send_message.py)) completa la fila de
  la clave con cualquier resultado: `Reservation.complete(render(outcome), entry)` tanto con un `SendResult` como con
  un `HermesError`. Una repetición recibe el fallo guardado durante 24 horas.
- `_execute` convierte una excepción inesperada tras reservar la clave en `HermesError(INTERNAL, "an unexpected
  failure")`, que se guarda con HTTP 500 y `retryable: true`: la guía de integración manda repetir con la misma clave,
  y la repetición recibe el mismo `internal`.
- La prueba `test_a_failure_of_the_provider_is_the_response_of_its_key`
  ([test_idempotency.py](../../hermes/tests/integration/api/test_idempotency.py)) fija hoy ese comportamiento, y la
  [guía de integración](../../docs/hermes/integracion.md#la-idempotency-key) lo esquiva pidiendo otra clave para
  reintentar un fallo del proveedor.
- Hades deriva la clave de `response_id + paso + hash(input resuelto)`
  ([hades.md](../../docs/hades.md#relanzar-idempotencia-y-compensación)): un reintento del bloque, o relanzar el paso
  con el mismo input, lleva la misma clave.

## R2. La regla en el código: liberar la clave

**Decisión:**

- **El repositorio** gana `release(caller, key, audit)` en el port `IdempotencyRepository`
  ([ports.py](../../hermes/src/hermes/application/ports.py)): borra la fila `in_progress` de la clave e inserta la
  entrada de auditoría del intento, en una sola transacción. Si la entrada no se puede escribir, no borra nada; con la
  base caída, `unavailable`, como los demás métodos. `SqlIdempotencyRepository` lo implementa con un `DELETE` de la
  fila `in_progress` e `insert_entry` en la misma transacción, y el doble `MemoryIdempotency` de las pruebas, igual.
- **La reserva** gana `Reservation.release(audit)`: llama al repositorio y deja `response` en `None`.
- **Qué se libera:** `_Reserved.after_send` completa la fila, como hoy, con un `SendResult` o con un `HermesError` con
  `outcome_unknown`; con cualquier otro `HermesError` la libera. Es un fallo sin envío: un `ProviderFailure` sin
  `outcome_unknown` (el proveedor no conecta, pide esperar más de lo que cabe, rechaza las credenciales o la petición,
  o la persona no admite el bot) o un error de Hermes antes de que la petición salga, como la instancia sin token del
  bot. El contrato de cada proveedor ya obliga a marcar con `outcome_unknown` lo que pudo salir
  ([proveedores](../../docs/hermes/proveedores.md)), así que la clasificación no cambia (FR-012).
- **La respuesta del primer intento no cambia:** con la clave liberada, `execute_once` vuelve a lanzar el fallo, como
  hoy un fallo antes del proveedor, y la ruta lo responde con su manejador de errores: un fallo del proveedor, con HTTP
  200 y el sobre `failed` con su `retry_after_s`; uno de Hermes, con el estado de su código. El cuerpo es el mismo
  `failed_body` que hoy se guardaba.
- **Las peticiones iguales a la vez:** el bloqueo en memoria de la clave sigue igual; al liberarse, la siguiente que
  esperaba no encuentra fila y hace su propio intento (historia 1, caso límite).
- **La misma clave con otro cuerpo** tras liberarla es una petición nueva, como tras un fallo antes del proveedor.
- **Las filas guardadas antes del cambio** con un fallo sin envío lo siguen devolviendo hasta que la limpieza las borra,
  a las 24 horas. No se migran: el efecto dura como mucho un día y no duplica nada.

**Por qué:** la clave protege un efecto; un fallo que garantiza que no hubo efecto no tiene nada que proteger, que es la
regla que Hermes ya aplica a los fallos antes del proveedor (R0, R9).

**Alternativas:**

- **Guardar el fallo y volver a intentar al repetir** si lo guardado es un fallo sin envío: el mismo efecto para quien
  llama, pero la clave seguiría ocupada (otro cuerpo, `conflict`), habría que interpretar la respuesta guardada y
  aparecería una carrera entre leerla y reintentar.
- **Un estado `released` en la fila** en lugar de borrarla: un estado y una migración nuevos sin nada que leer de
  ellos, porque la auditoría ya registra el intento.
- **Liberar solo los fallos reintentables:** descartado por el propietario (R0).

## R3. El fallo inesperado durante el envío

**Decisión:** en `_execute`, una excepción que no es `HermesError` tras reservar la clave pasa a ser un resultado
desconocido: `ProviderFailure(ErrorCode.TIMEOUT, UNEXPECTED_WHILE_SENDING, retryable=False, outcome_unknown=True)`, con
`UNEXPECTED_WHILE_SENDING = "an unexpected failure while sending: the message may have arrived"`.

- **Por qué un `ProviderFailure`:** va en el sobre con HTTP 200, como todo `timeout` con `outcome_unknown`; es lo mismo
  que ya hace `_unknown_outcome` de la ruta con la respuesta de una clave que un corte dejó a medias.
- **La fila se completa con él** (R2), y su entrada de auditoría dice
  `{"status": "failed", "code": "timeout", "outcome_unknown": true}`.
- **El log** sigue dejando `send.failed` con el tipo de la excepción, y `_execute` vuelve a lanzar la excepción
  original, como hoy.
- **El estado de la instancia no lo cuenta,** como hoy: es un fallo de Hermes, no del proveedor.
- **`internal`** queda para lo inesperado antes de reservar la clave (al preparar el envío o al resolver los secretos),
  que no deja fila y se repite con la misma clave.

**Alternativas** (descartadas por el propietario, R0): `internal` con la clave libre, que puede duplicar un mensaje que
salió; y `internal` guardado sin `retryable`, que daría dos significados a `internal`.

## R4. El contrato

**Decisión:** `hermes.v1.json` pasa de 1.0.1 a 1.0.2: solo cambian descripciones, sin esquemas ni operaciones, como la
1.0.1 de la 009. Los textos están en [contracts/](contracts/README.md).

| Dónde | Qué dice |
|---|---|
| `info.description` | El token corto lleva en su cabecera el `kid` de la clave de firma de llamadas de quien llama; un fallo al enviar, del proveedor o con resultado desconocido, va en el sobre con HTTP 200 |
| `callToken` | La cabecera del token: `alg` EdDSA, `typ` JWT y el `kid` que Hécate dio a quien llama con su certificado; sin él, `auth` |
| `IdempotencyKey` (descripción nueva) | La regla de la repetición: qué guarda la clave, qué recibe una repetición y cuándo otro cuerpo con ella es `conflict` |
| Respuesta 200 de `sendMessage` | El `timeout` con `outcome_unknown`, también tras un fallo inesperado al llamar al proveedor |
| Respuesta 409 de `sendMessage` | El canal sin verificar, o la `Idempotency-Key` que guarda otra petición |
| Respuesta 500 de `sendMessage` | Un fallo inesperado antes del proveedor, o el secreto pasado su tiempo duro |

- **La 409** decía que la clave «se usó» con otra petición, y eso deja de bastar: una clave usada con una petición que
  falló sin envío queda libre, y otra petición con ella es nueva. El conflicto es con la petición que la clave guarda.
- **No cambian:** `common.v1.json` (el sobre y los códigos son los mismos), `hecate.v1.json` y la guía de integración de
  Hécate, que describe lo que exige Hécate; las respuestas 500 de las capacidades y de la prueba de conexión, que no
  envían nada.
- **Pruebas de contrato:** `test_openapi_schema.py` pasa a esperar la 1.0.2 y comprueba que la descripción de
  `callToken` nombra el `kid`, que `IdempotencyKey` describe la repetición y que la 409 del envío dice que la clave
  guarda otra petición.

**Alternativas:**

- **Un campo `kid` en el esquema de seguridad:** OpenAPI no describe la cabecera de un JWT; la descripción es el sitio.
- **Subir a 1.1.0:** no hay nada nuevo que usar, solo se describe lo que Hermes ya hace o pasa a hacer con los mismos
  códigos.

## R5. La regla de repetición, código a código

La que deben dar a la vez la guía de integración, el diseño de las llamadas, el contrato y el diseño de Hades (FR-009):

| Código | ¿Lo guarda la clave? | Qué hace quien llama |
|---|---|---|
| `completed` | Sí | Nada: una repetición devuelve lo mismo |
| `timeout` con `outcome_unknown` (también tras un fallo inesperado al enviar) | Sí | No reenviar sin más: con la misma clave recibe lo mismo, y con otra puede duplicar el mensaje |
| `rate_limited` del proveedor (200) | No | Esperar `retry_after_s` y repetir con la misma clave |
| `provider_unavailable` con `retryable` | No | Reintentar más tarde con la misma clave |
| `provider_unavailable` sin `retryable` (credenciales o petición rechazadas) | No | Avisar al admin y, arreglada la causa, repetir con la misma clave |
| `conflict` del proveedor (200: la persona no admite el bot) | No | Avisar al admin y, cuando la persona vuelva a admitir el bot, repetir con la misma clave |
| Los fallos antes del proveedor (`validation`, `auth`, `not_found`, `conflict` 409, `rate_limited` 429, `unavailable`, `internal`) | No | Como hoy |

## R6. Documentos

Al cerrar (FR-009 a FR-011), con los textos del apartado siguiente:

| Documento | Qué cambia |
|---|---|
| `docs/hermes/integracion.md` | «La `Idempotency-Key`»: la clave guarda solo lo que se envió o pudo enviarse, y tras un fallo sin envío se repite con la misma, con la viñeta nueva de la misma clave con otro cuerpo; la tabla de errores, con la regla de R5 (`rate_limited` y `provider_unavailable` del proveedor, `conflict` 200, `timeout` e `internal`) y la fila de `conflict` 409, que pasa a decir «…o la `Idempotency-Key` ya guarda otra petición» |
| `docs/hermes/llamadas.md` | «La idempotencia» (la reserva se completa o se libera, y la viñeta de la misma clave con otro cuerpo, con el texto de abajo), «Los reintentos seguros» (Hades reintenta con la misma clave) y «El resultado desconocido» (el fallo inesperado al enviar) |
| `docs/hermes/contratos.md` | La tabla de códigos: `conflict`, «…o una clave de idempotencia que ya guarda otra petición»; `internal`, antes del proveedor; y `timeout`, también tras un fallo inesperado al llamar al proveedor |
| `docs/hades.md` | «Relanzar, idempotencia y compensación»: un reintento o un relanzamiento con el mismo input usa la misma clave, y en Hermes la clave solo guarda lo que se envió o pudo enviarse, con su enlace; la viñeta sigue en una sola línea, como las demás del fichero |
| `GLOSSARY.md` | Idempotency Key: en Hermes, la clave guarda la respuesta de lo que se envió o pudo enviarse; con otro cuerpo, mientras la guarda, `conflict` |
| `docs/adr/0014-clave-de-idempotencia-hermes.md` y `docs/adr/README.md` | El ADR nuevo (R7) y su fila |
| `DECISIONS.md` | El apartado de la clave frente a Stripe y al borrador de la IETF, tras el del acceso por app frente a Istio (principio VII, R9); la fila del ADR 0014 en Tomadas; se retira «Reintento de Hades tras un fallo reintentable de Hermes» de las abiertas |
| `docs/hermes.md` | «Decisiones y riesgos»: la viñeta de la clave de idempotencia tras «Frente a SecOps» y el ADR 0014 en «Tomadas»; sus demás frases sobre la clave siguen siendo ciertas |
| `hermes/README.md` | La fila de la 012 en «Las specs», el ADR 0014 en la frase de las decisiones y, en la tabla de ayudantes, las claves de `network_api.py` (R8) |
| `hecate/README.md` | La fila de la 009 en la tabla de specs |

- **El diseño de Hades** nombra la regla de Hermes y no la extiende a Estigia, que también comprueba claves: hacerlo
  resolvería de forma implícita cómo guarda Estigia las suyas, que nadie ha decidido.
- **No cambian:** ARCHITECTURE, la operación, la CLI, la seguridad, los proveedores, la configuración y la comunicación
  de Hermes, los documentos de Hécate y la constitución.

### Los textos

**La viñeta de la misma clave con otro cuerpo**, en «La `Idempotency-Key`» de `docs/hermes/integracion.md`:

```markdown
- **La misma clave con otro cuerpo** responde `conflict` mientras la clave guarda otra petición; tras un fallo sin
  envío o antes del proveedor, es una petición nueva.
```

**La misma viñeta en «La idempotencia» de `docs/hermes/llamadas.md`**, que conserva su referencia al contrato común:

```markdown
- **La misma clave con otro cuerpo** responde `conflict` mientras la clave guarda otra petición, como fija el contrato
  común; tras un fallo sin envío o antes del proveedor, es una petición nueva.
```

**El apartado de `DECISIONS.md`**, tras «Acceso por app frente a las mallas de servicios (Istio)» y con su forma:

```markdown
### La clave de idempotencia de Hermes frente a Stripe y al borrador de la IETF

SecOps SOAR no tiene clave de idempotencia en sus acciones. La referencia es la idempotencia de Stripe: guarda la
respuesta de una petición que empezó a ejecutarse, también un 500, que pide tratar como indeterminado, y no la de una
que no llegó a ejecutarse. Hermes sigue ese criterio con el proveedor en el lugar de la ejecución
([ADR 0014](docs/adr/0014-clave-de-idempotencia-hermes.md)). El borrador de la IETF de la cabecera `Idempotency-Key`
deja a cada servicio qué guarda la clave, y Hermes se aparta de él en dos cosas:

| Desviación | Justificación |
|---|---|
| La misma clave con otro cuerpo responde `conflict` con 409 (el borrador: 422) | Las APIs internas comparten un conjunto cerrado de códigos de error, sin 422 (principio II), y Hécate responde lo mismo. |
| Una repetición que llega con la primera en curso la espera y recibe su respuesta (el borrador: 409) | Hermes es una sola instancia y le basta un bloqueo en memoria por clave: cien peticiones iguales a la vez envían un mensaje, y quien llama no tiene que repetir tras un conflicto. |
```

**La fila de Tomadas** de `DECISIONS.md`:

```markdown
| Lo que guarda la clave de idempotencia de Hermes: lo que se envió o pudo enviarse; un fallo que garantiza que no se envió nada la deja libre | [0014](docs/adr/0014-clave-de-idempotencia-hermes.md) |
```

**En `docs/hermes.md`**, la viñeta tras «Frente a SecOps»:

```markdown
- **La clave de idempotencia** (SecOps SOAR no la tiene): el criterio de Stripe, guardar solo lo que se envió o pudo
  enviarse; frente al borrador de la IETF, otro cuerpo con la misma clave responde `conflict`, y una repetición con la
  primera en curso la espera
  ([DECISIONS](../DECISIONS.md#la-clave-de-idempotencia-de-hermes-frente-a-stripe-y-al-borrador-de-la-ietf)).
```

Y al final de «Tomadas», tras el ADR 0013: «; lo que guarda la clave de idempotencia, solo lo que se envió o pudo
enviarse ([ADR 0014](adr/0014-clave-de-idempotencia-hermes.md))».

**En `hermes/README.md`:**

- la fila de «Las specs»:

  ```markdown
  | El contrato de llamada: repetir con la misma clave y el `kid` del token | [specs/012-hermes-call-contract](../specs/012-hermes-call-contract/) |
  ```

- la frase de las decisiones: «Las decisiones de Hermes están en los ADR 0011, 0012, 0013 y 0014», cada uno con su
  enlace;
- la fila de `network_api.py`: «La API de red con dobles de sus ports, el fake y Telegram con respuestas grabadas; las
  claves, en memoria o en el PostgreSQL que le pase la prueba».

## R7. El ADR 0014

**Decisión:** `docs/adr/0014-clave-de-idempotencia-hermes.md`, «Lo que guarda la clave de idempotencia de Hermes»,
aceptado el 2026-11-19, con las cuatro secciones de la plantilla de [docs/adr](../../docs/adr/README.md#plantilla):

- **Contexto:** la contradicción con Hades (R1), el relanzamiento tras arreglar la causa y el `internal` guardado; y las
  referencias de R9: el criterio de Stripe y lo que el borrador de la IETF deja a cada servicio.
- **Opciones:** los tres caminos de la clave y las tres respuestas al fallo inesperado (R0, R2, R3).
- **Decisión:** la de R0.
- **Consecuencias:** una sola regla de repetición para quien llama (R5), cada intento que llega al proveedor en la
  auditoría, una petición con otro cuerpo tras un fallo sin envío como petición nueva, y las filas anteriores que
  caducan solas.

**Por qué:** la constitución lleva a un ADR lo que resuelve una decisión abierta («Consolidación»). Las referencias van
en el contexto porque la plantilla no tiene una sección para ellas y ningún ADR la añade.

## R8. Pruebas

TDD en cada nivel. Las pruebas de hoy siguen pasando salvo las tres que fijan lo anterior, que cambian: el fallo del
proveedor guardado en la clave, el `internal` de un fallo inesperado al enviar y la versión 1.0.1 del contrato.

- **Unitarias de la idempotencia** (`test_idempotency.py` de la aplicación):
  - `Reservation.release` borra la fila y deja su entrada de auditoría (FR-002, FR-005);
  - con dos peticiones iguales a la vez, si la primera se libera, la segunda llega al proveedor (caso límite).
- **Unitarias del envío** (`test_send_message.py`). Su `World` gana las claves en memoria (`MemoryIdempotency` sobre su
  auditoría) y un render de prueba que guarda el resultado que recibe; el HTTP 200 lo pone la ruta, así que lo
  comprueban las pruebas de la API:
  - `execute_once` con cada fallo sin envío (`provider_unavailable` con y sin `retryable`, el `rate_limited` y el
    `conflict` del proveedor, y un error de Hermes antes de que la petición salga) libera la clave, escribe la entrada
    de auditoría y lanza el fallo (FR-002);
  - con `outcome_unknown` completa la fila (FR-004);
  - una excepción inesperada al enviar completa la fila, el render recibe un `ProviderFailure` con `timeout`,
    `retryable` falso, `outcome_unknown` y `UNEXPECTED_WHILE_SENDING`, y la auditoría lo dice (FR-004); la prueba
    `test_an_unexpected_failure_while_sending_is_audited_too` pasa a esperar ese resultado.
- **Integración del repositorio** (`test_idempotency_repository.py`, PostgreSQL de usar y tirar):
  - `release` quita la fila y escribe la entrada en una transacción;
  - si la entrada no se puede escribir, la fila sigue `in_progress`;
  - con la base caída, `unavailable`.
- **Integración de la API** (`test_idempotency.py`):
  - `test_a_failure_of_the_provider_is_the_response_of_its_key` pasa a comprobar lo contrario: tras cada fallo sin
    envío del `fake` (`unreachable` todo el timeout, `rate_limited` con una espera que no cabe,
    `credentials_rejected` y `recipient_refuses`), la repetición con la misma clave envía una vez y deja dos entradas
    de auditoría (historias 1 y 2);
  - tras ese envío, una petición más con la misma clave lo devuelve sin volver a enviar (historia 1, escenario 3);
  - sin arreglar la causa, la repetición responde el mismo fallo, llega otra vez al proveedor sin enviar y deja otra
    entrada (historia 2, escenario 3);
  - la misma clave con otro cuerpo tras un fallo sin envío es una petición nueva;
  - tras un fallo sin envío, un reintento que acaba en un corte deja el `timeout` en la clave, y la repetición siguiente
    lo devuelve sin enviar (caso límite);
  - si la liberación no se puede escribir, la petición responde `unavailable`, y la repetición, `timeout` con
    `outcome_unknown`, sin llamar al proveedor (caso límite);
  - un fallo inesperado al enviar responde 200 con `timeout` y `outcome_unknown`, y su repetición, lo mismo sin enviar
    (historia 3); la excepción se provoca con `monkeypatch` sobre `send` del `FakeProvider` de la API de las pruebas,
    y `FakeFailure` no cambia (FR-012);
  - las de un envío completado, un corte y la clave de otro cuerpo siguen como hoy.
- **Integración de la API sobre PostgreSQL:** el arnés `NetworkApi` de `support/` gana el repositorio de las claves
  como parámetro, en memoria si no se le pasa, como hoy. Con `SqlIdempotencyRepository` y la auditoría leída de la base
  de usar y tirar con `audit_rows`, recorre la historia 1 (un fallo sin envío y la repetición que envía) y la historia 3
  (un resultado desconocido que no se reenvía), para probar la liberación con la base real. Alternativas: un arnés
  propio en el fichero nuevo, que duplicaría unas 30 líneas de `NetworkApi`, o llevar la `Api` de `test_idempotency.py`
  a `support/`, que añadiría otro módulo de ayuda.
- **Contrato** (`test_openapi_schema.py`): la versión 1.0.2, el `kid` en `callToken`, la regla en `IdempotencyKey` y la
  409 del envío.
- **Autenticación** (`test_authentication.py`): un token sin `kid` responde `auth` sin preguntar a Hécate, como hoy.
  Ninguna prueba lo fijaba, y es lo que el contrato pasa a describir (historia 4, FR-007).
- **E2E en el compose:** sin escenario nuevo. El `fake` del compose no puede fallar a demanda, y la regla vive en la
  aplicación y en el repositorio, que cubren los niveles de arriba; el E2E sigue comprobando que la repetición de un
  envío completado no envía dos veces a través de Envoy.
- **Calidad:** ruff, el formato, mypy estricto y pip-audit, sin avisos.

## R9. Referencias

- **Stripe** ([peticiones idempotentes](https://docs.stripe.com/api/idempotent_requests),
  [errores](https://docs.stripe.com/error-low-level)): no guarda el resultado de una petición que no llegó a ejecutarse
  (parámetros que no valen, un conflicto con otra en curso, el límite de uso), que se repite tras arreglar la causa; sí
  guarda la de una que empezó a ejecutarse, también un 500, y pide tratar ese 500 como un resultado indeterminado. Es
  el criterio de este cambio, con el proveedor en el lugar de la ejecución.
- **El borrador de la IETF** de la cabecera `Idempotency-Key`
  ([datatracker](https://datatracker.ietf.org/doc/html/draft-ietf-httpapi-idempotency-key-header)), en su versión 08,
  de octubre de 2026, sigue sin ser un RFC. Coincide en repetir con la misma clave y deja a cada servicio qué guarda la
  clave. Hermes ya se aparta en dos cosas que no cambian aquí: otro cuerpo con la misma clave es `conflict` con 409, el
  código del contrato común (el borrador recomienda 422), y una repetición que llega con la primera en curso la espera
  en lugar de responder un conflicto (el borrador recomienda 409).
- **SecOps:** SecOps SOAR no tiene un equivalente a la clave de idempotencia de sus acciones; el diseño es propio, con
  el criterio de Stripe, y DECISIONS lo recoge con las dos diferencias del borrador (principio VII, R6).

## R10. La suite en Linux

Como en la 011 ([R9 de la 011](../011-hermes-own-revocation/research.md#r9-la-suite-en-linux)): las pruebas que no
necesitan Docker, en `python:3.15.2-slim-trixie` con un `HECATE_BIN` compilado para Linux, sobre una copia del
repositorio; las de Docker, desde macOS. Las órdenes están en el [quickstart](quickstart.md#1-pruebas-y-calidad).

## R11. Sin tecnología nueva

Ninguna dependencia, variable de entorno, tabla, columna ni migración nuevas: un método más en el repositorio de las
claves y en su port. La constitución (1.2.1) no cambia; el ADR 0014 recoge la decisión (R7).
