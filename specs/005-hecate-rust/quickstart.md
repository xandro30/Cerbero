# Quickstart: validar la migración de Hécate a Rust

Guía para comprobar el cambio 005. Se ejecuta desde la raíz del repositorio, en la rama `refactor/hecate-rust`. Los pasos
de los quickstarts de [001](../001-hecate-bootstrap/quickstart.md) y [002](../002-hecate-secrets/quickstart.md) están
traducidos aquí a la nueva CLI, porque las specs fusionadas no se editan (FR-009). Los contratos están en
[contracts/](contracts/README.md) y los formatos, en [data-model.md](data-model.md).

## Requisitos

- Rust 1.99.1: rustup lo instala solo al entrar en `hecate/`, por su `rust-toolchain.toml`.
- cargo-deny 0.20.2: `cargo install cargo-deny --version 0.20.2 --locked`.
- Docker, para la pasada en Linux.
- uv y la versión en Python, solo para generar los vectores antes de borrarla.

## 1. Vectores (una sola vez, antes de borrar la versión en Python)

```bash
uv run --package cerbero-hecate python specs/005-hecate-rust/vectors/generate.py hecate/tests/vectors
```

**Esperado:** los ficheros de research R5 en `hecate/tests/vectors/` y la tabla
`hecate/crates/domain/src/printable_table.rs`, con las versiones de Python y de Unicode en la cabecera.

## 2. Pruebas

Las órdenes de cargo de los pasos 2 y 3 se ejecutan dentro de `hecate/`, para que rustup use su `rust-toolchain.toml`:

```bash
cd hecate
```

```bash
cargo test --workspace
```

**Esperado:** todas pasan, incluidas las de vectores y esquema (SC-008) y las de los tipos que se borran al soltarse
(SC-004).

## 3. Calidad (FR-010, SC-006)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

**Esperado:** sin salida de error ni avisos. Los pasos siguientes vuelven a la raíz del repositorio.

## 4. Linux (FR-017)

```bash
docker run --rm -v "$PWD/hecate":/work -w /work -e CARGO_TARGET_DIR=/tmp/target --ulimit memlock=-1:-1 rust:1.99.1-bookworm cargo test --workspace
```

**Esperado:** el mismo resultado que en macOS. El `target/` va dentro del contenedor para no mezclarse con el de macOS.
Si la imagen necesita paquetes para compilar AWS-LC o libsodium, se añaden a esta orden.

## 5. Mapeo de pruebas (FR-008, SC-001)

```bash
grep -c '^| `' specs/005-hecate-rust/test-mapping.md
grep -rhoE '// py: [^ ]+' hecate/crates | sort -u | wc -l
```

**Esperado:** 346 en las dos, y ninguna fila sin prueba en Rust.

## 6. Recorrido manual de 001, traducido

```bash
(cd hecate && cargo build --release)
export PATH="$PWD/hecate/target/release:$PATH"
export HECATE_DATA_DIR="$(mktemp -d)"
hecate serve
```

Opcional, para ir más rápido: `export HECATE_KDF_PROFILE=moderate` antes de `hecate serve` (`sensitive`, el valor por
defecto, tarda decenas de segundos por derivación). El tiempo del paso 3 se mide con el perfil por defecto.

En otra terminal, con el mismo `PATH` y `HECATE_DATA_DIR`:

| Paso | Comando | Resultado esperado |
|---|---|---|
| 1 | `hecate status` | `state: uninitialized` |
| 1b | `hecate init` y Ctrl-C cuando pide confirmar las partes | Aborta la inicialización, el terminal vuelve a mostrar lo que se escribe y `hecate status` sigue en `uninitialized` |
| 2 | `hecate init` | Pide la frase dos veces, avisa de que perder frase y dos partes es irrecuperable, muestra tres partes `HEC2-…` y pide confirmar dos; termina en `sealed` |
| 3 | `time hecate unseal` con la frase correcta | `state: unsealed`, en menos de 1 minuto con el perfil por defecto (SC-003) |
| 4 | `hecate seal` | `state: sealed` |
| 5 | `hecate unseal` tres veces con una frase incorrecta | El tercer fallo provoca una espera de 1 minuto; un cuarto intento inmediato responde con la espera pendiente |
| 6 | Parar y arrancar `hecate serve`; `hecate status` | `sealed`, con la espera pendiente |
| 6b | Esperar el minuto del paso 5 | `hecate status` ya no muestra espera |
| 7 | `hecate unseal --shares` con las partes 1 y 3 | `unsealed` y recomienda cambiar la frase |
| 8 | `hecate passphrase change --with-shares` con dos partes como prueba | Frase nueva; tras reiniciar, la anterior falla y la nueva desbloquea |
| 9 | `hecate shares rekey` | Partes `HEC2` nuevas; una parte antigua se rechaza como de otro juego |
| 10 | `hecate unseal --shares` con un carácter cambiado en una parte | Aviso de parte no válida, sin contar como intento |
| 11 | `hecate unseal --shares` con una parte `HEC1` | `validation` («must start with HEC2»), sin contar como intento |

## 7. Recorrido manual de 002, traducido

Con `hecate serve` en marcha, inicializada y desbloqueada:

| Paso | Comando | Resultado esperado |
|---|---|---|
| 1 | `hecate secrets create secret://hermes/telegram/family_bot/bot_token --type string` | Pide el valor sin mostrarlo; versión 1 y su huella |
| 2 | `hecate secrets create secret://hermes/certs/proxy --type pem --from-file proxy.pem` | Versión 1; con un fichero que no es PEM, error de validación |
| 3 | `hecate secrets list --identity hermes` | Las dos rutas, sin valores |
| 4 | `hecate secrets put secret://hermes/telegram/family_bot/bot_token` | Versión 2 activa; la 1, `deprecated` |
| 5 | `hecate secrets get secret://hermes/telegram/family_bot/bot_token` | Versiones con estado, fecha y huella; ningún valor |
| 6 | `hecate secrets revoke … --version 1` | La 1 pasa a `revoked` |
| 7 | `hecate secrets destroy … --version 1` | Pide la frase; con una incorrecta, `auth` y sigue `revoked`; con la correcta, `destroyed` |
| 8 | `hecate seal` y `hecate secrets list` | `conflict`: está sellada |
| 9 | `hecate status` | Incluye los recuentos de caducados y por caducar |

## 8. Comprobaciones de seguridad, traducidas

- Buscar la frase, una parte y los valores escritos en `$HECATE_DATA_DIR` y en los logs del proceso: no deben aparecer
  (SC-006 de 001 y SC-002 de 002).
- Matar el proceso durante `hecate init` antes de confirmar: al arrancar, `status` es `uninitialized` (SC-008 de 001).
- Tras destruir una versión, ni restaurando una copia de `hecate.db` hecha después se recupera su valor (SC-004 de 002).
- Revisar que cada secreto de paso de [data-model.md](data-model.md) vive en un tipo que se borra al soltarse y que no se
  copia fuera de él (FR-007).

## 9. Retirada de Python y docs (FR-011 a FR-015)

```bash
find hecate -path hecate/target -prune -o \( -name '*.py' -o -name pyproject.toml \) -print
grep -n '"hecate"' pyproject.toml
grep -n -E 'PyCryptodome|PyNaCl|getpass|Python no garantiza' README.md docs/hecate.md DECISIONS.md
git diff --stat main -- orthros specs/001-hecate-bootstrap specs/002-hecate-secrets
```

**Esperado:** ninguna salida (`hecate/target` se salta porque la compilación descarga allí las fuentes de libsodium, que traen scripts `.py`). Además, a mano: el README presenta Hécate como workspace de Cargo y remite al ADR 0005; el
doc de Hécate describe las librerías en Rust, cómo se pide el valor de un secreto y el riesgo de memoria nuevo; en
DECISIONS solo cambian el riesgo de memoria y la fila de Shamir, y la fila de la feature 003 sigue igual.
