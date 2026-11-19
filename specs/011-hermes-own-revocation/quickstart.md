# Quickstart: validar que Hermes nota la revocación urgente de su certificado

Escenarios que demuestran el arreglo de extremo a extremo. El modelo está en [data-model.md](data-model.md) y lo que no
cambia, en [contracts/](contracts/README.md). Las órdenes se ejecutan desde la raíz del repositorio.

## Requisitos

- Los de la feature 008 para Hermes: uv 0.13, Python 3.15, Docker con Colima (`colima start`) y Docker Compose.
- El binario de Hécate para macOS: `cargo build --release --manifest-path hecate/Cargo.toml`. En los pasos,
  `HECATE_BIN="$PWD/hecate/target/release/hecate"`.
- Para la pasada en Linux, el binario de Hécate compilado para Linux en el volumen de Docker `cerbero-hecate-linux`
  (apartado 1).

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

En Linux ([research R9](research.md#r9-la-suite-en-linux)), primero el binario de Hécate, una vez, en un volumen fuera
del repositorio:

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -v cerbero-hecate-linux:/target -w /work -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=1 rust:1.99.1-bookworm cargo build --release --locked --bin hecate
```

Después, las pruebas que no necesitan Docker, sobre una copia del repositorio dentro del contenedor, para no tocar el
`.venv` ni las cachés de macOS:

```bash
docker run --rm -v "$PWD":/src:ro -v cerbero-hecate-linux:/target:ro -e HECATE_BIN=/target/release/hecate --ulimit memlock=-1:-1 python:3.15.2-slim-trixie sh -c 'pip install --quiet --root-user-action=ignore uv==0.13.1 && mkdir /tmp/cerbero && tar -C /src --exclude=.git --exclude=target --exclude=.venv --exclude=__pycache__ --exclude=.pytest_cache --exclude=.mypy_cache --exclude=.ruff_cache --exclude=.hypothesis --exclude=hermes/deploy/.secrets -cf - . | tar -C /tmp/cerbero -xf - && cd /tmp/cerbero && uv run --package cerbero-hermes pytest hermes/tests -m "not docker"'
```

- `--ulimit memlock=-1:-1`: las pruebas que lanzan la Hécate real la necesitan en Linux, como dice el
  [README de Hécate](../../hecate/README.md#la-suite-en-linux).
- Las pruebas de Docker (el PostgreSQL de usar y tirar, Envoy y el compose) pasan desde macOS: sus servicios ya corren
  en contenedores Linux, y el E2E ejecuta Hermes en Linux.

**Esperado:** pasa todo, sin avisos y sin dependencias nuevas: en macOS, la suite entera; en Linux, todo salvo las de
Docker. Las pruebas de las features anteriores siguen pasando.

## 2. Las pruebas del arreglo, solas

```bash
uv run --package cerbero-hermes pytest hermes/tests/unit/application/test_identity_state.py hermes/tests/unit/application/test_secret_cache.py hermes/tests/unit/test_serve_enrollment.py
```

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests/integration/hecate/test_client.py
```

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests/e2e/test_flow.py
```

**Esperado:**

- las unitarias cubren FR-001 a FR-008 con `FakeHecate`: cada camino rechazado marca la identidad, ningún otro fallo
  la marca, tras la marca no sale ninguna llamada, el alta funciona, el rechazo atrasado no cuenta y el reinicio vuelve
  a comprobarla; y `hermes serve` vuelve a pedir la contraseña de la base tras el alta
  ([research R8](research.md#r8-pruebas));
- la integración demuestra contra una Hécate real que, tras `hecate certificates revoke <serie> --urgent`, Hécate
  responde `auth`, que cada llamada rechazada deja una fila en su registro de accesos y la bloqueada ninguna, y que un
  token nuevo da de alta a Hermes otra vez sobre los mismos directorios;
- el E2E recorre la revocación en el compose: la salud dice `not ready: needs a new enrollment token` en 300 s como
  mucho, un envío responde `unavailable` sin enviar nada, el alta funciona sin vaciar nada, Envoy presenta el
  certificado nuevo sin que se reinicie ningún contenedor y un envío llega después.

## 3. Recorrido en el compose (historias 1 a 3)

Siguiendo solo [la operación de Hermes](../../docs/hermes/operacion.md), ya consolidada:

1. Pon en marcha el compose de pruebas con el fake, Hermes dado de alta, la configuración aplicada y un componente con
   acceso que envía, como dicen la puesta en marcha y «El compose de pruebas».
2. Lee la serie del certificado de Hermes: `docker compose exec hermes cat /var/lib/hermes/state/identity.json`
   (campo `serial`).
3. Revócalo con urgencia: `docker compose exec hecate hecate certificates revoke <serie> --urgent`, que escribe
   `revoked <serie> urgent`.
4. Consulta la salud cada pocos segundos, `docker compose exec hermes hermes health`, hasta que diga
   `not ready: needs a new enrollment token` y salga con 1: como mucho 240 s después de la revocación, más lo que tarde
   ese refresco (SC-001).
5. `docker compose logs hermes` tiene una sola línea `identity.refused`, con la serie y la llamada rechazada. Desde
   ella, Hermes no llama a Hécate con esa identidad: en dos copias del directorio de datos de Hécate separadas por más
   de 240 s (`docker compose cp hecate:/var/lib/hecate <copia>`), las filas de Hermes en la tabla `access_log` de
   `hecate.db`, las de `subject` = `spiffe://cerbero.test/hermes`, no crecen
   ([el registro de accesos](../../docs/hecate/auditoria.md#el-registro-de-accesos)). Las copias llevan la clave de
   Hécate en claro: se borran al terminar.
6. Un envío del componente responde `unavailable` y no aparece nada en la bandeja de salida del fake.
7. Sigue «La identidad de Hermes deja de valer»: `docker compose exec hecate hecate tokens create hermes` y
   `docker compose exec -T hermes hermes enroll`, con el token por la entrada estándar, sin parar Hermes ni vaciar
   nada. Escribe `enrolled spiffe://cerbero.test/hermes serial <serie nueva> until <fecha>`.
8. La salud vuelve a `ready` en 2 minutos como mucho, y un envío del componente llega a la bandeja de salida, con la
   misma configuración y el canal verificado de antes. Envoy no se ha reiniciado.
9. El reinicio (FR-007): revoca otra vez el certificado nuevo y reinicia Hermes enseguida,
   `docker compose restart hermes`. A los pocos segundos de arrancar, la salud dice
   `not ready: needs a new enrollment token`, sin esperar al refresco; otra alta lo deja listo.

**Esperado:** cada paso da lo que dice. El paso 4 cumple SC-001; el 5 y el 6, SC-002 y SC-006; el 7 y el 8, SC-003; y
recorrerlo solo con el documento, SC-005.

## 4. Los documentos

```bash
grep -rn -e "vacía su directorio de estado" -e "Vaciar el directorio de estado" -e "no l[ao] nota hasta" -e "rechazó su renovación, como tras" docs/hermes.md docs/hermes hermes/README.md
```

```bash
grep -n "Revocación urgente de Hermes" DECISIONS.md
```

```bash
grep -n "identity.refused" docs/hermes/operacion.md
```

**Esperado:** las dos primeras órdenes no encuentran nada, y la tercera encuentra la línea de log en el procedimiento
«La identidad de Hermes deja de valer». Los documentos de R10 describen el comportamiento
([research R10](research.md#r10-documentos)).
