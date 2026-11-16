# Quickstart: validar la lista de acceso por app

Escenarios que demuestran la feature de extremo a extremo. Los contratos están en [contracts/](contracts/README.md) y
el modelo, en [data-model.md](data-model.md). Las órdenes se ejecutan desde la raíz del repositorio.

## Requisitos

- Los de la feature 006 para Hécate: el toolchain de `hecate/rust-toolchain.toml` (Rust 1.99.1), un compilador de C y
  Docker (o Colima) para la pasada en Linux.
- Los de la feature 008 para Hermes: uv 0.13, Python 3.15 y Docker con Colima.
- El binario: `cargo build --release --manifest-path hecate/Cargo.toml`. En los pasos, `hecate` es
  `hecate/target/release/hecate`.
- El componente de prueba de Hécate: `cargo run --manifest-path hecate/Cargo.toml --example component -- <orden>`. En
  los pasos, `component <orden>`, con sus claves en `--state <dir>`. Esta feature le añade `callers`.

## 1. Pruebas y calidad

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

```bash
cargo fmt --manifest-path hecate/Cargo.toml --all --check
```

```bash
cargo clippy --manifest-path hecate/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

```bash
cargo deny --manifest-path hecate/Cargo.toml check
```

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -w /work -e CARGO_TARGET_DIR=/tmp/target -e CARGO_BUILD_JOBS=1 --ulimit memlock=-1:-1 rust:1.99.1-bookworm cargo test --workspace
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

**Esperado:** pasa todo, en macOS y en Linux para Hécate, sin avisos y sin dependencias nuevas. Las pruebas de las
features anteriores siguen pasando; las de Hermes conceden antes el acceso a quien llama.

## 2. Recorrido: conceder y retirar (historia 1)

Con una Hécate inicializada, desbloqueada y con su CA, como en su
[puesta en marcha](../../docs/hecate/operacion.md):

```bash
hecate access grant caller hermes
```

```bash
hecate access grant caller hermes
```

```bash
hecate access get hermes
```

```bash
hecate access revoke caller hermes
```

```bash
hecate access list
```

**Esperado:**

- la primera orden escribe `granted caller access to hermes (access version 1)` y la segunda, `unchanged: caller
  already calls hermes`, sin versión nueva;
- `get` enseña `access hermes version 1`, la línea `caller caller` y el historial con quién y cuándo;
- tras retirarlo, `list` enseña `hermes version 2 callers 0`;
- `hecate access grant hermes hermes`, `hecate access grant admin hermes` y `hecate access grant caller hades/x` dan
  `error (validation): ...` y salida 1;
- con Hécate sellada, cualquier orden da `error (conflict): ...`;
- cada orden deja su fila en la tabla `access_log` de la base de Hécate (`access_grant`, `access_revoke`,
  `access_list` y `access_get`), como explica su [auditoría](../../docs/hecate/auditoria.md#el-registro-de-accesos).

## 3. Recorrido: una app consulta su lista (historia 2)

Con el componente de prueba dado de alta como `hermes` (un token de alta `hecate tokens create hermes`) y como en la
[guía de integración](../../docs/hecate/integracion.md):

```bash
hecate access grant caller hermes
```

```bash
component callers --state /tmp/hermes-component
```

**Esperado:** `{"app": "hermes", "callers": ["caller"], "version": 3}` en el sobre común, sin dejar fila en el registro
de accesos; con Hécate sellada, `unavailable`. Ninguna orden del componente permite pedir la lista de otra app.

## 4. Recorrido: Hermes atiende solo a quien tiene acceso (historia 3)

Con el entorno de extremo a extremo de la 008 (`hermes/deploy/`), siguiendo su
[quickstart](../008-hermes-send-message/quickstart.md) hasta el alta de Hermes y la configuración, y con dos tokens de
alta para componentes de prueba (`caller` y `other`):

```bash
docker compose -f hermes/deploy/compose.yaml exec hecate hecate access grant caller hermes
```

**Esperado:**

- antes de conceder nada, el cliente de referencia como `caller` recibe `auth` (401) sin que salga ningún mensaje;
- tras conceder, envía en la llamada siguiente, sin esperar al refresco;
- como `other`, sin acceso, recibe `auth`, y el log de Hermes dice `auth.refused` con el motivo `not_allowed` y el
  `request_id`;
- tras `hecate access revoke caller hermes`, `caller` recibe `auth` como mucho a los 240 s, sin reiniciar Hermes;
- con `hecate seal`, Hermes sigue aplicando la última lista;
- con Hermes recién instalado (sin `access.json` en su directorio de estado) y Hécate parada, `hermes health` escribe
  `not ready: access list not loaded` y los envíos responden `unavailable`.

## Dónde se comprueba cada criterio

| Criterio | Dónde |
|---|---|
| SC-001 | Pruebas de la API de Hermes y E2E: quien no está en la lista recibe `auth` sin efectos; paso 4 |
| SC-002 | La suite de Hermes de la 008, con el acceso concedido, sigue pasando |
| SC-003 | Pruebas de la aplicación de Hermes con el reloj inyectado y E2E: concesión en la llamada siguiente, retirada en 240 s |
| SC-004 | Pruebas de la aplicación de Hermes con Hécate sin responder, y paso 4 con `hecate seal` |
| SC-005 | Pruebas de la API de Hécate: la consulta solo da la lista de quien pregunta; paso 3 |
| SC-006 | Pruebas del dominio de Hécate: el historial y la lista en un instante pasado; paso 2 |
| SC-007 | Pruebas de contrato de Hécate (`listCallers`) y la comprobación de contrato de las suites de Hermes |
