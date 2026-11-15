# Quickstart: validar la base de Hermes y el envío por Telegram

Cómo comprobar de extremo a extremo que la feature funciona. Los detalles de cada operación están en los
[contratos](contracts/README.md) y en el [modelo de datos](data-model.md); aquí solo van los pasos y lo que se espera.

## Requisitos

- Docker; en macOS, con Colima (`colima start`).
- uv 0.13 y Python 3.15 (`uv python install 3.15`).
- Rust 1.99.1, para compilar Hécate.
- Para los pasos con Telegram (5 a 7): un bot creado con BotFather y una cuenta de Telegram.

## 1. Las suites automáticas

```bash
cargo build --release --manifest-path hecate/Cargo.toml
```

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

**Se espera:** todo pasa. Las suites cubren las seis historias con el proveedor fake, Telegram con respuestas grabadas,
un PostgreSQL de usar y tirar, una Hécate real lanzada con `HECATE_BIN` y el E2E en contenedores con Envoy. Lo que mueve
el reloj (SC-004, SC-005) usa dobles de Hécate con el reloj inyectado.

El cambio del sobre común (`common.v1.json` 1.1.0) toca la prueba de contrato de Hécate, así que su suite se pasa
también, en macOS y en Linux, como dice su [README](../../hecate/README.md):

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

## 2. El entorno de extremo a extremo

El compose de `hermes/deploy/` levanta PostgreSQL, una Hécate de pruebas, Hermes y Envoy. Es un entorno de pruebas: no
fija cómo corre Hermes en el home lab.

```bash
hermes/deploy/prepare.sh
```

```bash
docker compose -f hermes/deploy/compose.yaml up -d --build postgres hecate
```

- `prepare.sh` genera la contraseña del usuario `hermes` de PostgreSQL en `hermes/deploy/.secrets/` (fuera de Git) para
  que el contenedor de PostgreSQL cree el usuario y su base.
- **Se espera:** PostgreSQL con la base `hermes` y su usuario, sin acceso a otras bases, y Hécate escuchando.

## 3. La Hécate de pruebas

Dentro del contenedor (`docker compose -f hermes/deploy/compose.yaml exec hecate ...`), sigue la
[puesta en marcha de Hécate](../../docs/hecate/operacion.md) con el trust domain `cerbero.test`: inicializar,
desbloquear y crear la CA. Después, guarda los dos secretos de Hermes y crea los dos tokens de alta:

```bash
docker compose -f hermes/deploy/compose.yaml exec -T hecate hecate secrets create secret://hermes/postgres/password --type string < hermes/deploy/.secrets/postgres_hermes_password
```

```bash
docker compose -f hermes/deploy/compose.yaml exec hecate hecate secrets create secret://hermes/telegram/family_bot/bot_token --type string
```

```bash
docker compose -f hermes/deploy/compose.yaml exec hecate hecate tokens create hermes
```

```bash
docker compose -f hermes/deploy/compose.yaml exec hecate hecate tokens create caller
```

**Se espera:** los secretos creados con su versión 1 y dos tokens `HET1....`, uno para Hermes y otro para el cliente de
referencia, que hace de componente que llama.

## 4. Hermes y Envoy

```bash
docker compose -f hermes/deploy/compose.yaml up -d --build hermes envoy
```

```bash
docker compose -f hermes/deploy/compose.yaml exec hermes hermes enroll
```

```bash
docker compose -f hermes/deploy/compose.yaml exec hermes hermes health
```

**Se espera:**

- antes del alta, `hermes health` escribe `not ready: needs a new enrollment token` y sale con 1 (FR-010);
- `hermes enroll` pide el token sin eco y escribe `enrolled spiffe://cerbero.test/hermes serial <serie> until <fecha>`
  (historia 3);
- después, `hermes health` escribe `ready` y sale con 0 (FR-015);
- Hermes no publica ningún puerto: `docker compose -f hermes/deploy/compose.yaml ps hermes` no muestra puertos, y solo
  Envoy llega a su socket (historia 2).

## 5. La configuración

Guarda el ejemplo de [config-file.md](contracts/config-file.md) como `config.yaml`, fuera del repositorio, y aplícalo:

```bash
docker compose -f hermes/deploy/compose.yaml exec -T hermes hermes config apply /dev/stdin --yes < config.yaml
```

**Se espera (historia 4):**

- un cambio por línea (`+ variable case_base_url`, `+ instance family_bot`, `+ template_version device_unknown v1`,
  `+ contact contacto_demo`, `+ channel contacto_demo telegram/family_bot`) y `applied 5 changes`;
- aplicar otra vez el mismo fichero escribe `no changes`;
- con `bot_token: "123:abc"` en lugar de la referencia, `error: instances[0].secrets.bot_token: ...`, código de salida 1
  y nada aplicado;
- `hermes config export` devuelve el mismo contenido, sin direcciones ni secretos.

## 6. Verificar el canal

```bash
docker compose -f hermes/deploy/compose.yaml exec hermes hermes contacts code contacto_demo --instance family_bot
```

**Se espera:** `code XXXX-XXXX for contacto_demo on family_bot expires <fecha>`. Enviado al bot desde la cuenta de
Telegram, el bot contesta «Canal verificado.». Un código inventado no recibe respuesta (historia 4).

## 7. Enviar como un componente

El cliente de referencia de las pruebas (`hermes/tests/support/caller.py`) se da de alta en la Hécate de pruebas con su
token, firma su token corto y llama a Hermes a través de Envoy:

```bash
uv run --package cerbero-hermes python hermes/tests/support/caller.py enroll --hecate https://localhost:8443 --state /tmp/caller
```

```bash
uv run --package cerbero-hermes python hermes/tests/support/caller.py send --hermes https://localhost:9443 --state /tmp/caller --recipient contacto_demo --template device_unknown --data '{"mac": "AA:BB:CC:DD:EE:FF", "ip": "192.168.1.57", "case_id": "case-1234"}' --key qs-1
```

```bash
uv run --package cerbero-hermes python hermes/tests/support/caller.py bench --hermes https://localhost:9443 --state /tmp/caller --count 20
```

**Se espera:**

- `send` devuelve el sobre `completed` con `provider_message_ref`, y el mensaje llega al chat con el título, los hechos
  y el enlace (historia 1);
- repetir `send` con `--key qs-1` devuelve la misma respuesta y no llega un segundo mensaje; con la misma clave y otros
  datos, `conflict` (historia 5);
- `bench` envía 20 mensajes y dice que el 95 % llegó en menos de 5 segundos (SC-001);
- antes del paso 6, el mismo `send` responde `conflict` porque el canal no está verificado (FR-020).

## 8. Fallos

**Se espera:**

- una llamada sin certificado de cliente, o con un token caducado o de otra audiencia, responde `auth` (401) sin enviar
  nada. El E2E prueba las de la historia 2 que el entorno puede producir, también un certificado revocado con urgencia
  una vez que Hermes refresca su lista de revocación; el certificado caducado lo prueban las pruebas de la API
  (`hermes/tests/integration/api/test_auth_routes.py`), porque Hécate solo emite certificados vigentes (SC-002);
- con `hecate seal`, los envíos siguen con el token del bot en caché; pasada la hora desde la última resolución, fallan
  con `internal` sin llegar a Telegram (SC-004, comprobado con el reloj inyectado en las suites);
- con PostgreSQL parado, `hermes health` escribe `not ready: database unavailable` y los envíos responden `unavailable`.

## 9. Limpiar

```bash
docker compose -f hermes/deploy/compose.yaml down -v
```

## Dónde se comprueba cada criterio

| Criterio | Dónde |
|---|---|
| SC-001 | Paso 7, `bench` con Telegram real |
| SC-002 | E2E de la historia 2, pruebas de la API y paso 8 |
| SC-003 | Prueba de integración: 100 repeticiones a la vez y con un corte simulado |
| SC-004 | Prueba de aplicación con el reloj inyectado, y paso 8 |
| SC-005 | Prueba de aplicación: 15 renovaciones con el reloj inyectado y una renovación real contra Hécate |
| SC-006 | Batería de conformidad, con el fake y con Telegram grabado |
| SC-007 | Prueba que busca los tokens, los códigos y las direcciones en los logs, las respuestas, la auditoría y lo exportado |
| SC-008 | Pruebas de la historia 4 y paso 7 |
| SC-009 | Pruebas de contrato de todas las respuestas |
| SC-010 | Pruebas de la historia 4 y paso 5 |
