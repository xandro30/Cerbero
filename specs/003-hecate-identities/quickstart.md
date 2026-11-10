# Quickstart: validar las identidades de Hécate

Escenarios que demuestran la feature de extremo a extremo. Los contratos están en [contracts/](contracts/) y el modelo,
en [data-model.md](data-model.md). Las órdenes se ejecutan desde la raíz del repositorio.

## Requisitos

- El toolchain de `hecate/rust-toolchain.toml` (Rust 1.99.1) y un compilador de C (ADR 0007).
- Docker (o Colima) para la pasada en Linux.
- El binario: `cargo build --release --manifest-path hecate/Cargo.toml`. En los pasos, `hecate` es
  `hecate/target/release/hecate`.
- El componente de prueba: `cargo run --manifest-path hecate/Cargo.toml --example component -- <orden>`. En los pasos,
  `component <orden>`. Guarda sus claves y certificados en `--state <dir>`, con permisos `0600`.

## 1. Pruebas (principio IV)

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

**Esperado:** pasan todas: las de 001 y 002 (FR-030) y las nuevas de unidad, propiedades, aplicación, integración de la
API con el componente de prueba, contrato y CLI.

## 2. Calidad (FR-031)

```bash
cargo fmt --manifest-path hecate/Cargo.toml --all --check
```

```bash
cargo clippy --manifest-path hecate/Cargo.toml --workspace --all-targets -- -D warnings
```

```bash
cargo deny --manifest-path hecate/Cargo.toml check
```

```bash
cargo tree --manifest-path hecate/Cargo.toml --workspace -e normal,features -i rustls | grep -c tls12
```

**Esperado:** las tres primeras sin avisos, con las licencias nuevas dentro de la política del ADR 0008, y la última
imprime `0`: el binario no compila TLS 1.2 (solo las pruebas, para su cliente).

## 3. Linux (FR-031)

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -w /work -e CARGO_TARGET_DIR=/tmp/target --ulimit memlock=-1:-1 rust:1.99.1-bookworm cargo test --workspace
```

**Esperado:** el mismo resultado que en macOS. Se monta `shared/` porque las pruebas de contrato leen los OpenAPI. Con
la VM por defecto de Colima (2 GiB), añade `-e CARGO_BUILD_JOBS=1`: dos enlazados de pruebas a la vez agotan su memoria.

## 4. Contrato (principio II)

```bash
cargo test --manifest-path hecate/Cargo.toml -p hecate-infrastructure --test 'contract_*' --test 'api_*'
```

**Esperado:** pasan las pruebas de contrato del canal local (`contract_*_ops.rs` y `contract_status_op.rs`), la de los
OpenAPI (`contract_schema.rs`) y las de la API (`api_*.rs`), que validan cada respuesta, también las de error y
`unavailable`, contra `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` y `common.v1.json`.

## 5. Recorrido: CA y alta (historias 1 y 2)

Terminal A:

```bash
export HECATE_DATA_DIR="$(mktemp -d)" HECATE_KDF_PROFILE=moderate HECATE_API_ADDR=127.0.0.1:8443 && hecate serve
```

Terminal B, con las mismas variables; `hecate init` y `hecate unseal` como en la feature 001:

1. `hecate ca create --trust-domain cerbero.test --out "$HECATE_DATA_DIR/../root-ca.pem"` y dar una frase distinta al
   comprobar la copia. **Esperado:** `error: the saved copy does not open with that passphrase`, sin CA, sin la copia y
   con una fila `ca_create` de `backup_check_failed` (historia 1, escenario 8). Con una frase de 11 caracteres o igual a
   la de desbloqueo, `error (validation)` y sin CA (historia 1, escenario 2).
2. La misma orden con la frase de la CA. Cuando pida la ruta de una copia guardada, la misma ruta y otra vez la frase.
   **Esperado:** `created ca for cerbero.test` y la huella.
3. `hecate status`. **Esperado:** `ca: cerbero.test sha256:<hex>`.
4. `hecate tokens create hermes`. **Esperado:** el token, una sola vez. `hecate tokens list` lo muestra sin su valor.
   `hecate tokens create admin` da `error (validation)`.
5. `component enroll --token <token> --state /tmp/hermes`. **Esperado:**
   - el componente comprueba a Hécate con la huella del token;
   - recibe un certificado de 3 días con `spiffe://cerbero.test/hermes`;
   - la misma orden otra vez, que envía una solicitud nueva con el mismo token, da `error (auth)`;
   - con `--repeat`, que repite exactamente la primera petición, devuelve la misma serie.
6. `component enroll --token <otro token para hermes> --state /tmp/hermes --asks-for spiffe://cerbero.test/hades`.
   **Esperado:** `error (validation)`, y el token sigue en `hecate tokens list` (historia 2, escenario 3).

## 6. Recorrido: renovación, claves y revocación (historias 3 a 5)

1. `component renew --state /tmp/hermes`. **Esperado:** una serie nueva con la misma identidad.
2. `component renew --state /tmp/hermes --from <serie anterior>`. **Esperado:**
   - `error (auth)`;
   - `hecate identities get hermes` muestra todos sus certificados `revoked`;
   - `hecate status` cuenta un intento sospechoso (historia 3, escenario 5).
3. Dar de alta otra vez `hermes` con un token nuevo y pedir `component keys` y `component revocations`. **Esperado:**
   las claves válidas con `expires_at` a 5 minutos, y una CRL con las series revocadas del paso 2.
4. `hecate certificates revoke <serie> --urgent`. **Esperado:** la serie aparece en la CRL y su clave deja de estar en
   `component keys`.
5. `hecate identities disable hermes --urgent`, y después `component renew` y `hecate tokens create hermes`.
   **Esperado:** `error (auth)` y `error (conflict)`. `hecate identities enable hermes` permite un alta nueva, solo con
   un token nuevo.
6. `hecate certificates revoke <serie> --urgent`, con la serie de la línea `hecate certificate` de `hecate status`.
   **Esperado:** `reissued <serie nueva>`. El componente ve la cadena nueva y la serie antigua en la CRL (historia 5,
   escenario 7). Sin `--urgent`, `error (validation)`.

## 7. Recorrido: intermedia y sustitución de la CA (historias 6 y 7)

1. `hecate ca renew-intermediate --root-key "$HECATE_DATA_DIR/../root-ca.pem"`. **Esperado:**
   - las altas y las renovaciones nuevas usan la versión 2;
   - los certificados anteriores siguen valiendo y se renuevan con la intermedia nueva;
   - con una frase incorrecta, error sin cambios.
2. `hecate ca replace --out "$HECATE_DATA_DIR/../root-ca-2.pem"`, con la frase de desbloqueo y una frase nueva de la
   CA. **Esperado:**
   - `replaced ca for cerbero.test generation 2`;
   - `component renew` con un certificado anterior ya no llega a Hécate: el componente fija la CA raíz anterior y
     rechaza la nueva (`error: cannot reach Hécate`). Un componente que fije la raíz nueva y presente un certificado
     anterior recibe `auth` (SC-006, en las pruebas de la API);
   - los tokens pendientes ya no sirven: llevan la huella de la raíz anterior;
   - un alta con un token nuevo funciona;
   - con una prueba incorrecta, `error (auth)` y cuenta en el contador común.

## 8. Recorrido: Hécate sellada y freno de altas (FR-023, FR-034)

1. `hecate seal` y `component renew`. **Esperado:** `error (unavailable)`, y el componente conserva su certificado.
   `hecate status` sigue mostrando el bloque `ca`. Si el certificado propio de Hécate caduca estando sellada, la API
   deja de aceptar conexiones; al desbloquear, Hécate lo renueva y vuelve a escuchar.
2. Desbloquear y lanzar 10 altas con tokens inventados (`component enroll --token HET1.hermes.<...>` en un bucle).
   **Esperado:**
   - la undécima recibe `error (rate_limited)` con la espera, aunque traiga un token válido;
   - `hecate status` muestra `enrollment throttled: yes`;
   - el registro guarda una fila por tanda, que la tarea de cada minuto escribe al acabar la espera;
   - `hecate unseal` no se ve afectado.

## 9. Seguridad y memoria (SC-003, SC-012, FR-028)

```bash
sqlite3 "$HECATE_DATA_DIR/hecate.db" .dump | grep -c -E 'HET1\.|HECATE ROOT CA KEY|<frase de la CA>'
```

**Esperado:** `0`. La base no contiene tokens, ni la copia de la clave de la CA raíz, ni la frase de la CA.

Además:
- `ca_material` solo tiene claves cifradas, y ninguna de una intermedia retirada.
- Las pruebas de memoria de la suite comprueban que los tipos de la frase, de la clave de la CA raíz, de las
  intermedias y de los tokens se borran al soltarse.

## 10. Consolidación (FR-032, FR-033)

Al cerrar el cambio:

```bash
grep -n -E 'siempre por mTLS|/health/live|\*\*Pública\*\*|ceremonia offline|llega con las identidades' docs/hecate.md ARCHITECTURE.md
```

**Esperado:**
- La orden no imprime nada.
- DECISIONS ya no tiene la fila de la 003 en «Cambios pausados», y sí el riesgo aceptado de la clave del certificado de
  Hécate y las desviaciones de esta feature.
