# Quickstart: validar la Access Policy y la resolución de secretos

Escenarios que demuestran la feature de extremo a extremo. Los contratos están en [contracts/](contracts/) y el modelo,
en [data-model.md](data-model.md). Las órdenes se ejecutan desde la raíz del repositorio.

## Requisitos

- Los de la feature 003: el toolchain de `hecate/rust-toolchain.toml` (Rust 1.99.1), un compilador de C y Docker (o
  Colima) para la pasada en Linux.
- El binario: `cargo build --release --manifest-path hecate/Cargo.toml`. En los pasos, `hecate` es
  `hecate/target/release/hecate`.
- El componente de prueba: `cargo run --manifest-path hecate/Cargo.toml --example component -- <orden>`. En los pasos,
  `component <orden>`, con sus claves en `--state <dir>`. Esta feature le añade `resolve` y `status`.

## 1. Pruebas (principio IV)

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

**Esperado:** pasan todas, también las de las features 001, 002 y 003, con las ampliaciones a propósito: el contrato
de `status` y la lista de operaciones de la API, y `"90d"` cambiado a `"P90D"` en las pruebas de la 002 (FR-020).

## 2. Calidad (FR-021)

```bash
cargo fmt --manifest-path hecate/Cargo.toml --all --check
```

```bash
cargo clippy --manifest-path hecate/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

```bash
cargo deny --manifest-path hecate/Cargo.toml check
```

**Esperado:** sin avisos y sin dependencias nuevas.

## 3. Linux (FR-021)

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -w /work -e CARGO_TARGET_DIR=/tmp/target -e CARGO_BUILD_JOBS=1 --ulimit memlock=-1:-1 rust:1.99.1-bookworm cargo test --workspace
```

**Esperado:** el mismo resultado que en macOS. `CARGO_BUILD_JOBS=1` hace falta con la VM por defecto de Colima (2 GiB).

## 4. Contrato (principio II)

```bash
cargo test --manifest-path hecate/Cargo.toml -p hecate-infrastructure --test 'contract_*' --test 'api_*'
```

**Esperado:** cada respuesta de `POST /v1/secrets:resolve` y `GET /v1/status`, también las de error, valida contra
`hecate.v1.json`, y las operaciones `policies.*` y el `status` ampliado pasan sus pruebas de contrato.

## 5. Recorrido: resolver (historia 1)

Terminal A:

```bash
export HECATE_DATA_DIR="$(mktemp -d)" HECATE_KDF_PROFILE=moderate HECATE_API_ADDR=127.0.0.1:8443 && hecate serve
```

Terminal B, con las mismas variables; `hecate init`, `hecate unseal` y `hecate ca create` como en las features 001 y
003:

1. `hecate secrets create secret://hermes/telegram/bot_token --type string --rotate-every 90d`. **Esperado:**
   `error (validation)` que pide una duración como `P90D` (FR-023). Con `--rotate-every P90D`, la versión 1 y su huella.
2. `hecate secrets create secret://hades/smtp/password --type string`. **Esperado:** la versión 1.
3. `hecate tokens create hermes` y `component enroll --token <token> --state /tmp/hermes`, como en la feature 003.
4. `component resolve --state /tmp/hermes secret://hermes/telegram/bot_token`. **Esperado:** el valor, la versión 1 y
   los tiempos de caché 300 y 3600.
5. `component resolve --state /tmp/hermes secret://hades/smtp/password`, `... secret://hermes/no/existe` y
   `... secret://hermes/telegram/bot_token@9`. **Esperado:** las tres dan `error (not_found)` con el mismo mensaje, y
   ninguna antes de unos 250 ms.
6. `component resolve --state /tmp/hermes secret://hermes/telegram/bot_token secret://hades/smtp/password`.
   **Esperado:** `error (not_found)`, sin ningún valor (todo o nada).

## 6. Recorrido: compartir (historia 2)

1. `hecate policies grant-read hermes secret://hades/smtp/password`. **Esperado:** `granted read ... (policy version
   1)`. La misma orden otra vez: `unchanged: ...`.
2. `component resolve --state /tmp/hermes secret://hades/smtp/password`. **Esperado:** el valor.
3. `hecate policies get hermes`. **Esperado:** `policy hermes version 1 status no`, la línea `read
   secret://hades/smtp/password` y su historial con `by admin`.
4. `hecate policies revoke-read hermes secret://hades/smtp/password` y resolver otra vez. **Esperado:** `revoked ...
   (policy version 2)` y `error (not_found)`.
5. `hecate policies grant-read admin secret://hades/smtp/password`, `... grant-read hermes secret://hades/no/existe` y
   `... grant-read hades secret://hades/smtp/password`. **Esperado:** `error (validation)`, `error (not_found)` y
   `error (validation)`, sin cambiar ninguna política.

## 7. Recorrido: estado (historia 3)

1. `component status --state /tmp/hermes`. **Esperado:** `error (auth)`, con una fila `status_get`.
2. `hecate policies grant-status hermes` y `component status --state /tmp/hermes`. **Esperado:** `sealed false` y los
   recuentos, sin rutas ni nombres.
3. `hecate seal` y repetir `component status`. **Esperado:** `sealed true` y los mismos recuentos (FR-012).
   `component resolve` da `error (unavailable)`.
4. `hecate status`. **Esperado:** la línea `secrets due rotation: 0`.

## 8. Tiempos, memoria y fugas

```bash
cargo test --manifest-path hecate/Cargo.toml -p hecate-infrastructure --test api_resolve --test api_resolve_timing --test performance --test api_server --test identity_no_leak
```

```bash
cargo test --manifest-path hecate/Cargo.toml -p hecate-application --test memory
```

**Esperado:**

- la prueba de tiempos (SC-003): la diferencia entre dos grupos de rechazos no supera la mayor de las diferencias
  entre las mitades de los dos, más 1 ms;
- la de la primera fase: 100 referencias por debajo de 50 ms (research R2);
- la de rendimiento (SC-001);
- las de memoria y fugas: la respuesta se borra al soltarse. Ningún valor resuelto aparece en la base, el registro,
  los logs ni las respuestas de error, y su huella tampoco en el registro, los logs ni las respuestas. La resolución no
  escribe eventos: la huella solo está en la base en los eventos de su secreto, como desde la feature 002 (SC-004).

## 9. Al cerrar (FR-022)

```bash
grep -n "caso de uso interno sin canal" docs/hecate.md
```

**Esperado:** no imprime nada. Además, DECISIONS ya no tiene la decisión «Access Policy de Hécate y resolución por el
canal autenticado» en «Abiertas», y existe el ADR 0010 con su fila en el índice de ADR.
