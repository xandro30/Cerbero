# Quickstart: validar la repetición con la misma clave y el `kid` en el contrato de Hermes

Escenarios que demuestran el cambio de extremo a extremo. El modelo está en [data-model.md](data-model.md) y los
contratos, en [contracts/](contracts/README.md). Las órdenes se ejecutan desde la raíz del repositorio.

## Requisitos

- Los de la feature 008 para Hermes: uv 0.13, Python 3.15, Docker con Colima (`colima start`) y Docker Compose.
- El binario de Hécate para macOS: `cargo build --release --manifest-path hecate/Cargo.toml`. En los pasos,
  `HECATE_BIN="$PWD/hecate/target/release/hecate"`.
- Para la pasada en Linux, el binario de Hécate compilado para Linux en el volumen de Docker `cerbero-hecate-linux`
  (apartado 1).
- `jq`, para el apartado 3.

## 1. Pruebas y calidad

En macOS, la suite entera, con Docker en marcha:

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

```bash
uv run --package cerbero-hermes ruff check hermes && uv run --package cerbero-hermes ruff format --check hermes
```

```bash
uv run --package cerbero-hermes mypy --strict hermes/src
```

```bash
uv run --package cerbero-hermes pip-audit
```

En Linux ([research R10](research.md#r10-la-suite-en-linux)), primero el binario de Hécate, una vez, en un volumen
fuera del repositorio:

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -v cerbero-hecate-linux:/target -w /work -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=1 rust:1.99.1-bookworm cargo build --release --locked --bin hecate
```

Después, las pruebas que no necesitan Docker, sobre una copia del repositorio dentro del contenedor:

```bash
docker run --rm -v "$PWD":/src:ro -v cerbero-hecate-linux:/target:ro -e HECATE_BIN=/target/release/hecate --ulimit memlock=-1:-1 python:3.15.2-slim-trixie sh -c 'pip install --quiet --root-user-action=ignore uv==0.13.1 && mkdir /tmp/cerbero && tar -C /src --exclude=.git --exclude=target --exclude=.venv --exclude=__pycache__ --exclude=.pytest_cache --exclude=.mypy_cache --exclude=.ruff_cache --exclude=.hypothesis --exclude=hermes/deploy/.secrets -cf - . | tar -C /tmp/cerbero -xf - && cd /tmp/cerbero && uv run --package cerbero-hermes pytest hermes/tests -m "not docker"'
```

**Esperado:** pasa todo, sin avisos y sin dependencias nuevas: en macOS, la suite entera; en Linux, todo salvo las de
Docker. Las pruebas de las features anteriores siguen pasando, salvo las tres que el cambio sustituye (research R8).

## 2. Las pruebas del cambio, solas

```bash
uv run --package cerbero-hermes pytest hermes/tests/unit/application/test_idempotency.py hermes/tests/unit/application/test_send_message.py hermes/tests/unit/application/test_authentication.py hermes/tests/integration/api/test_idempotency.py hermes/tests/contract/test_openapi_schema.py
```

```bash
uv run --package cerbero-hermes pytest hermes/tests/integration/persistence/test_idempotency_repository.py hermes/tests/integration/api/test_idempotency_postgres.py
```

**Esperado:**

- **Historia 1:** tras un fallo sin envío del `fake` (no conecta durante todo el timeout, o pide esperar más de lo que
  cabe), la repetición con la misma clave envía una vez, y la auditoría tiene una entrada por intento; una petición más
  con la misma clave devuelve el envío sin volver a enviar.
- **Historia 2:** tras rechazar las credenciales o la persona, quitado el fallo, la repetición con la misma clave envía;
  sin quitarlo, responde el mismo fallo, llega otra vez al proveedor sin enviar y deja otra entrada.
- **Historia 3:** un envío completado, un resultado desconocido, un corte y un fallo inesperado al enviar devuelven a
  toda repetición la misma respuesta sin volver a enviar; el fallo inesperado responde 200 con `timeout` y
  `outcome_unknown`.
- **Los casos límite:** la misma clave con otro cuerpo tras un fallo sin envío es una petición nueva; un reintento que
  acaba en un corte tras un fallo sin envío deja el `timeout` en la clave; y si la liberación no se puede escribir, la
  petición responde `unavailable` y la repetición, `timeout` con `outcome_unknown`, sin llamar al proveedor.
- **La base real:** la liberación borra la fila y escribe la entrada en una transacción; si la entrada no se puede
  escribir, la fila sigue reservada.
- **El contrato:** la versión 1.0.2, el `kid` en `callToken`, la regla en `IdempotencyKey` y la 409 del envío.
- **La autenticación:** un token sin `kid` responde `auth` sin preguntar a Hécate, como hoy.

## 3. El contrato

```bash
jq -r '.info.version' shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json
```

```bash
jq -r '.components.securitySchemes.callToken.description, .components.parameters.IdempotencyKey.description' shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json
```

```bash
jq -r '.paths["/chat/v1/send_message"].post.responses["200","409","500"].description' shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json
```

**Esperado:** `1.0.2`; la descripción de `callToken` nombra la cabecera con `kid` y dice que sin él responde `auth`; la
de `IdempotencyKey` dice qué guarda la clave, qué recibe una repetición y que otro cuerpo es `conflict` mientras la
clave se guarda; y las respuestas 200, 409 y 500 del envío dicen los textos de [contracts/](contracts/README.md).
`git diff main -- shared/contracts` solo cambia descripciones y la versión.

## 4. El E2E

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests/e2e/test_flow.py
```

**Esperado:** pasa como antes: un envío por Envoy llega, y su repetición con la misma clave no envía dos veces
(research R8).

## 5. Los documentos

```bash
grep -n 'otra clave' docs/hermes/integracion.md docs/hermes/llamadas.md
```

```bash
grep -n 'ya se usó con otra petición' docs/hermes/integracion.md
```

```bash
grep -n 'Reintento de Hades' DECISIONS.md; grep -n '0014' DECISIONS.md docs/adr/README.md docs/hermes.md hermes/README.md
```

```bash
grep -n 'Stripe' DECISIONS.md docs/hermes.md
```

```bash
grep -n '009-app-access' hecate/README.md; grep -n '012-hermes-call-contract' hermes/README.md; grep -n 'Idempotency Key' GLOSSARY.md
```

Las palabras de tiempo, primero en lo que el cambio añade a los documentos que git sigue y después en el ADR nuevo, que
`git diff` no ve mientras no está en git:

```bash
git diff main -- docs GLOSSARY.md DECISIONS.md hermes/README.md hecate/README.md | grep '^+' | grep -i -w -e 'hoy' -e 'todavía' -e 'por ahora' -e 'más adelante' -e 'en el futuro'
```

```bash
grep -i -w -e 'hoy' -e 'todavía' -e 'por ahora' -e 'más adelante' -e 'en el futuro' docs/adr/0014-clave-de-idempotencia-hermes.md
```

**Esperado:**

- `otra clave` solo aparece para una petición distinta (la fila de `conflict` 409) o para avisar de que tras un
  `timeout` puede duplicar el mensaje; ninguna regla manda reintentar un fallo del proveedor con otra clave.
- La guía ya no dice que la clave «ya se usó con otra petición»: dice que la guarda.
- DECISIONS ya no tiene la fila abierta del reintento de Hades; tiene la del ADR 0014 en Tomadas y el apartado de la
  clave frente a Stripe y al borrador de la IETF, que el diseño de alto nivel de Hermes enlaza; el índice de ADR, el
  diseño de alto nivel y el README de Hermes nombran el ADR 0014.
- El README de Hécate tiene la fila de la 009 y el de Hermes, la de la 012; el glosario dice que en Hermes la clave
  guarda lo que se envió o pudo enviarse.
- Las dos órdenes de las palabras de tiempo no imprimen nada; si el ADR no existe, la segunda da error.
- Para cada código de error, la guía de integración, el diseño de las llamadas, la tabla de códigos, el contrato y el
  diseño de Hades dan la misma regla de repetición ([contracts/](contracts/README.md#la-regla-de-repetición)): se
  revisa leyéndolos juntos (SC-005).
