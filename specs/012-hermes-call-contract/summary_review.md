# Guía de revisión: el contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

Rama `fix/hermes-call-contract`. Guía para revisar el cambio: por dónde empezar, cómo probarlo y qué conviene saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Cinco historias (un reintento tras un fallo sin envío llega a enviar, repetir tras
   arreglar la causa, lo que pudo enviarse no se repite, el contrato dice cómo es el token y los documentos dicen lo
   mismo), 12 requisitos y 6 criterios de éxito. Las 2 aclaraciones fijan la regla:
   - la clave solo guarda lo que se envió o pudo enviarse, y un fallo sin envío, reintentable o no, la deja libre con su
     entrada de auditoría;
   - un fallo inesperado de Hermes mientras llama al proveedor responde `timeout` con `outcome_unknown` y queda en la
     clave; `internal` queda para lo inesperado antes del proveedor.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md) y
   [contratos](contracts/README.md)). No hay dependencia, variable, tabla ni migración nuevas (R11), y sí un ADR, el
   0014 (R7). R1 explica la causa, y R5 da la regla de repetición código a código. Lo que decidiste:
   - al especificar: las dos aclaraciones (R0);
   - al revisar el plan: la liberación en una transacción con su auditoría (R2), el fallo inesperado como
     `ProviderFailure` con `timeout` (R3), los textos del contrato 1.0.2 (R4), los documentos sin extender la regla a
     Estigia (R6), el ADR 0014 (R7) y las pruebas, con la de la API sobre PostgreSQL y sin escenario nuevo en el E2E
     (R8);
   - al revisar el primer análisis: el apartado propio de DECISIONS frente a Stripe y al borrador de la IETF,
     `NetworkApi` con el repositorio de las claves, la viñeta de `docs/hades.md` en una línea y la spec ajustada;
   - al revisar el segundo: la condición de `conflict` en `IdempotencyKey` y en el glosario, y cuatro precisiones.
3. **El contrato**, `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`, en la 1.0.2: solo descripciones y
   la versión (`info.description`, `callToken`, la descripción nueva de `IdempotencyKey` y las respuestas 200, 409 y 500
   del envío).
4. **El código**, en `hermes/src/hermes/` y en este orden:
   - `application/send_message.py`: `_Reserved.after_send` libera la clave con un `HermesError` sin `outcome_unknown` y
     la completa con lo demás, y `_execute` convierte una excepción inesperada al enviar en
     `ProviderFailure(TIMEOUT, UNEXPECTED_WHILE_SENDING, retryable=False, outcome_unknown=True)`. Es el fichero más
     importante;
   - `application/idempotency.py`: `Reservation.release`;
   - `application/ports.py`: `IdempotencyRepository.release`;
   - `infrastructure/persistence/idempotency_repository.py`: `release`, con el `DELETE` de la fila `in_progress` e
     `insert_entry` en la misma transacción;
   - `infrastructure/api/routes_chat.py`: solo su docstring.
5. **Lo que toca código de las features anteriores**, que ya está en `main`:
   - `execute_once` no cambia: con la clave liberada, `reservation.response` sigue vacía, así que vuelve a lanzar el
     fallo, y la ruta lo responde con su manejador de errores, con el mismo sobre que antes se guardaba;
   - la constante `UNEXPECTED` desaparece; el log `send.failed`, la excepción que se vuelve a lanzar y que el estado de
     la instancia no cuente el fallo inesperado siguen igual.

   Se sustituyen a propósito tres pruebas (R8):
   - `test_a_failure_of_the_provider_is_the_response_of_its_key` pasa a ser la repetición tras arreglar la causa;
   - `test_an_unexpected_failure_while_sending_is_audited_too` espera `timeout` con `outcome_unknown`;
   - `test_both_documents_are_openapi_3_1_with_their_license_and_version` espera la 1.0.2.

   En los ayudantes: `MemoryIdempotency.release`; `NetworkApi` gana el parámetro `keys`, en memoria si no se le pasa; y
   `CutKeys`, el doble de las claves de las pruebas de la API, corta también la liberación.
6. **Las pruebas:** 937 en Hermes (las 907 de partida, con una sustituida, y 31 casos nuevos). Por fichero:
   - `hermes/tests/integration/persistence/test_idempotency_repository.py` (3): `release` quita la fila y escribe la
     entrada en una transacción; si la entrada no se puede escribir, la fila sigue `in_progress`; y solo toca la clave
     de su `caller`. La prueba de la base caída cubre también `release`;
   - `hermes/tests/unit/application/test_idempotency.py` (2): la reserva liberada no deja fila y sí su entrada; y, con
     dos peticiones iguales a la vez, si la primera libera, la segunda reserva y envía;
   - `hermes/tests/unit/application/test_authentication.py` (1): un token sin `kid` responde `auth` sin preguntar a
     Hécate, con el motivo `unknown_key` en el log;
   - `hermes/tests/contract/test_openapi_schema.py` (3): el `kid` en `callToken`, la regla en `IdempotencyKey` y las
     respuestas 200, 409 y 500 del envío; las 500 de las capacidades y de la prueba de conexión no cambian;
   - `hermes/tests/unit/application/test_send_message.py`, 10 casos de 6 pruebas: los fallos sin envío liberan la clave
     (dos reintentables y cuatro que piden arreglar la causa, entre ellos el `internal` de una instancia sin el token
     del bot); el envío completado y el resultado desconocido la completan; el fallo inesperado al enviar la completa
     con el `ProviderFailure` de R3, sin contar en el estado de la instancia; y el inesperado antes de reservar no deja
     fila;
   - `hermes/tests/integration/api/test_idempotency.py`, 10 casos de 8 pruebas: tras `unreachable` y tras el
     `rate_limited` del proveedor, la repetición envía una vez y una petición más no reenvía; un reintento que acaba en
     un corte deja el `timeout`; la liberación que no se puede escribir deja la clave reservada, y la repetición
     responde el resultado desconocido sin llamar al proveedor; tras arreglar la causa (las credenciales o la persona)
     la repetición envía, y sin arreglarla vuelve a llegar al proveedor y falla igual; otro cuerpo tras un fallo sin
     envío es una petición nueva; y el fallo inesperado al enviar responde 200 con `timeout`, y su repetición, lo mismo
     sin llamar al proveedor;
   - `hermes/tests/integration/api/test_idempotency_postgres.py` (2, fichero nuevo, marca `docker`): con
     `SqlIdempotencyRepository` y la auditoría en la base, un fallo sin envío no deja fila y sí su entrada, y la
     repetición envía; un corte tras enviar deja la fila `completed` con el `timeout`, y la repetición no reenvía.

## Cómo probarlo

Las órdenes del apartado 1 de [quickstart.md](quickstart.md): en Hermes ruff, el formato, `mypy --strict`, pip-audit y
la suite entera en macOS, con el binario release de Hécate y Docker:

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

En Linux, las pruebas sin Docker en el contenedor `python:3.15.2-slim-trixie`, con el binario de Hécate para Linux que
compila la primera orden de ese apartado. El apartado 2 tiene las pruebas del cambio solas; el 3, el contrato con `jq`;
el 4, el E2E; y el 5, las comprobaciones de los documentos.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, investigación, modelo, contratos, tareas, checklist y esta guía;
  - el contrato `hermes.v1.json` 1.0.2;
  - el código y las pruebas de Hermes;
  - los documentos: el ADR 0014 y su fila en el índice; `docs/hermes/integracion.md`, `llamadas.md` y `contratos.md`;
    `docs/hades.md`; `docs/hermes.md`; `GLOSSARY.md`; `DECISIONS.md`, que gana el apartado frente a Stripe y al
    borrador y la fila del ADR 0014 y pierde la fila abierta del reintento; y `hermes/README.md` y `hecate/README.md`.
- **Resultados:**
  - la línea base, sobre `main` (`5e7be30`): 907 pruebas en macOS;
  - Hermes en macOS: 937 pruebas con el binario release de Hécate, Docker y el E2E, sin ninguna saltada; ruff, el
    formato, `mypy --strict` y pip-audit, sin avisos ni vulnerabilidades;
  - Hermes en Linux, en el contenedor `python:3.15.2-slim-trixie` con el binario de Hécate para Linux: 856 pruebas,
    todas las que no necesitan Docker; las 81 de Docker son las que pasan desde macOS;
  - las pruebas de contrato de Hécate (`contract_schema`, 11) siguen pasando, y el diff de `shared/contracts` solo
    cambia descripciones y la versión;
  - las comprobaciones de los apartados 2, 3 y 5 dieron lo esperado, y la lectura conjunta de los documentos para cada
    código no encontró contradicciones (SC-005).
- **El estado intermedio del MVP:** entre T016 y T020, el fallo inesperado al enviar, que era un `internal` sin
  `outcome_unknown`, liberaba la clave. T020 lo cerró y no queda en el resultado, pero es por lo que US3 va justo
  después del MVP.
- **El TDD:** cada prueba nueva falló antes de su implementación, salvo las que fijan lo que ya existía, que lo dicen:
  - el token sin `kid` (T009);
  - el envío completado y el resultado desconocido, que se completan (T011);
  - la liberación que no se puede escribir (T013), porque antes el corte caía en `complete`;
  - el fallo inesperado antes de reservar (T017) y el corte sobre PostgreSQL (T019).
- **Tus decisiones durante la implementación:** aprobaste el MVP y las fases 5 y 6 en sus puntos de revisión.
- **Decisiones mías durante la implementación**, en las pruebas:
  - un doble de los estados de las instancias, `CountedStatuses`, en las unitarias del envío, para comprobar que el
    fallo inesperado no llega a `record_call`;
  - el fallo inesperado de la API se provoca con `monkeypatch` sobre `send` del `FakeProvider`, sin tocar `FakeFailure`
    (FR-012);
  - el checkpoint de la fase 2 pasó la suite sin Docker y las pruebas del repositorio sobre PostgreSQL; la suite
    entera, con Docker y el E2E, se pasó en los checkpoints del MVP y de la fase 5 y en el polish.
- **Los tiempos son de un Mac de desarrollo:** la suite de Hermes tarda entre 9 min y medio y 12 min y medio, según la
  pasada; en el contenedor Linux, las 856 pruebas tardan unos 28 s, más lo que tarda en instalar uv y las dependencias.
- **Fuera de alcance**, como fija la spec: construir Hades, cambiar la clasificación de los fallos de los proveedores,
  la vida de las claves y Hécate o su API. El diseño de Hades nombra la regla de Hermes sin extenderla a Estigia (R6).

## Dónde mirar con más sospecha

- **Qué libera la clave (`_Reserved.after_send`):** decide por `outcome_unknown`. Un `HermesError` sin él libera; un
  `SendResult` o un fallo con él completa. Depende de que cada proveedor marque con `outcome_unknown` todo lo que pudo
  salir, como obliga su contrato ([proveedores](../../docs/hermes/proveedores.md)): un proveedor que no lo marcara
  liberaría la clave tras un envío que sí salió.
- **La liberación que falla:** si la entrada no se puede escribir, la transacción no borra la fila y `release` lanza el
  error. La petición responde `unavailable` si la base no responde, o `internal` si rechaza la entrada; la fila sigue
  `in_progress`, y la repetición responde `timeout` con `outcome_unknown`, como tras un corte. Lo fijan la prueba del
  repositorio y la de la API con `CutKeys`.
- **La primera respuesta:** con la clave liberada, el fallo lo responde el manejador de errores de la ruta, no la fila
  guardada. El sobre es el mismo, con HTTP 200 y `retry_after_s` en el cuerpo, sin la cabecera `Retry-After`: lo fija
  `test_a_failure_of_the_provider_answers_200_inside_the_envelope`, que pasa sin cambios.
- **Las peticiones iguales a la vez:** el bloqueo en memoria no cambia. Tras una liberación, la siguiente que esperaba
  no encuentra fila y hace su propio intento, así que, con el proveedor caído, las peticiones iguales que esperaban
  llegan al proveedor por turno. Lo acotan el límite de la instancia y, en Hades, la espera entre reintentos y el
  timeout del bloque (caso límite de la spec).
- **Las filas anteriores:** las claves que guardaron un fallo sin envío antes de este cambio lo siguen devolviendo hasta
  que la limpieza las borra, a las 24 horas, sin migrarlas (R2).
- **El contrato:** la descripción de `IdempotencyKey` es nueva, y la de la 409 pasa de «was used with another request»
  a «is kept for another request». Los esquemas y los códigos no cambian, así que ningún cliente se rompe.

## Después de la revisión

- La primera pasada de `/speckit-converge` salió limpia: la implementación cumple la spec, el plan y las tareas, y los
  textos del contrato son, uno a uno, los de [contratos](contracts/README.md).
- T034 deja propuestos el mensaje del commit de squash y la descripción de la PR.
