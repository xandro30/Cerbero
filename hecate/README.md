# Hécate: secretos y claves

Hécate custodia los secretos de terceros de Cerbero, da identidad a sus componentes con su CA interna y firma su
auditoría. Qué es, por qué existe y cómo se usa está en [docs/hecate.md](../docs/hecate.md), que enlaza a cada
documento de detalle de [docs/hecate/](../docs/hecate/). Este README explica cómo se desarrolla: compilarla, probarla
en macOS y en Linux, las comprobaciones de calidad, el componente de prueba y cómo se organiza el código.

Las órdenes de este README se ejecutan desde la raíz del repositorio.

## Herramientas

- **Rust 1.99.1**, que fija `hecate/rust-toolchain.toml` con `rustfmt` y `clippy`: con rustup se instala solo la
  primera vez que se compila.
- **Un compilador de C:** aws-lc-rs, libsodium y SQLite (`rusqlite` con `bundled`) se compilan desde su código.
- **cargo-deny**, para las licencias, los avisos de seguridad y las dependencias: `cargo install cargo-deny`.
- **Docker**, con Colima en macOS, para pasar la suite en Linux.

## Compilar

```bash
cargo build --manifest-path hecate/Cargo.toml
```

```bash
cargo build --release --manifest-path hecate/Cargo.toml
```

El binario queda en `hecate/target/debug/hecate` o en `hecate/target/release/hecate`. Cómo se lanza y qué hace cada
orden está en [CLI y configuración](../docs/hecate/cli.md).

## La suite en macOS

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

- Hay pruebas unitarias junto al código de cada crate, pruebas de integración en `hecate/crates/*/tests/` y
  doctests que comprueban en compilación que los tipos con secretos no se pueden copiar ni mostrar.
- Las pruebas crean sus directorios temporales en `/tmp` con nombres cortos, para que quepa la ruta del socket, y no
  necesitan ninguna variable de entorno.
- Las pruebas del endurecimiento usan un doble del sistema operativo; la memoria bloqueada de verdad solo se ejercita en
  Linux, en las pruebas que lanzan `hecate serve`.

## La suite en Linux

Con Docker en marcha (en macOS, `colima start`):

```bash
docker run --rm -v "$PWD/hecate":/work -v "$PWD/shared":/shared -w /work -e CARGO_TARGET_DIR=/tmp/target -e CARGO_BUILD_JOBS=1 --ulimit memlock=-1:-1 rust:1.99.1-bookworm cargo test --workspace
```

- `-v "$PWD/shared":/shared`: las pruebas de contrato leen el OpenAPI de `shared/contracts`.
- `--ulimit memlock=-1:-1`: `hecate serve` bloquea su memoria en Linux, y sus pruebas necesitan el permiso.
- `CARGO_BUILD_JOBS=1`: con la VM por defecto de Colima (2 GiB), compilar en paralelo se queda sin memoria.
- `CARGO_TARGET_DIR=/tmp/target`: la compilación de Linux no se mezcla con la de macOS en `hecate/target`.

El resultado tiene que ser el mismo que en macOS.

## Comprobaciones de calidad

```bash
cargo fmt --manifest-path hecate/Cargo.toml --all --check
```

```bash
cargo clippy --manifest-path hecate/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

```bash
cargo deny --manifest-path hecate/Cargo.toml check
```

- El workspace prohíbe el código `unsafe` (`unsafe_code = "forbid"`) y activa todos los avisos de clippy, que la
  comprobación trata como errores.
- `hecate/deny.toml` fija las licencias admitidas, los avisos de seguridad y las dependencias duplicadas que se
  aceptan, cada una con su motivo.

## El componente de prueba

La feature `test-support` de `hecate-infrastructure` añade, solo para las pruebas:

- en `hecate/crates/application/src/testing.rs`, dobles en memoria de los ports;
- en `hecate/crates/infrastructure/src/testing.rs`, lo que montan las pruebas de integración (la base de datos, el
  canal local en un hilo, el contrato de la API para validar cada respuesta) y el componente de prueba: un cliente de
  la API que hace lo que haría un componente real, desde generar sus claves y comprobar a Hécate con la huella del token
  hasta renovar, resolver y guardar la lista de revocación.

El ejemplo `component` lo usa desde la línea de órdenes, contra la API de `HECATE_API_ADDR` (`127.0.0.1:8443` por
defecto) y con sus claves en el directorio de `--state`:

```bash
cargo run --manifest-path hecate/Cargo.toml --example component -- enroll --token <token> --state /tmp/hermes
```

Sus órdenes son `enroll`, `renew`, `keys`, `key <kid>`, `revocations`, `resolve <referencia>...` y `status`, todas con
`--state <dir>`. Cómo se integra un componente de verdad está en la
[guía de integración](../docs/hecate/integracion.md), que lo toma como cliente de referencia.

## Organización

Un workspace de Cargo con un crate por capa de la arquitectura hexagonal; cada capa solo depende de las de dentro.

| Crate | Qué tiene |
|---|---|
| `hecate-domain` (`crates/domain`) | Los aggregates y sus reglas, sin E/S: `sealing` (estados, frase, partes, freno de intentos, protección de la raíz y el registro de accesos), `secrets` (Secret, rutas, tipos de valor y `rotate_every`), `keys` (las claves de ámbito), `ca` (la CA y la copia de su clave raíz), `identity` (Identity, Certificate, tokens de alta, claves de firma de llamadas, nombres y freno de altas), `access_policy` y los eventos (`events.rs`) |
| `hecate-application` (`crates/application`) | Los casos de uso, uno por servicio: `sealing`, `secrets` (con el cifrado por sobre en `envelope.rs`), `ca`, `identity` (alta, renovación, revocación, resolución por la API y estado) y `access_policy`; y los ports (`ports.rs`) |
| `hecate-infrastructure` (`crates/infrastructure`) | Los adapters: `crypto.rs` (aws-lc-rs, libsodium y vsss-rs), `pki.rs` (rcgen y x509-parser), `store/` (SQLite), `control/` (el canal local), `api/` (axum sobre rustls), `call_token.rs` (jsonwebtoken), la configuración, el endurecimiento del proceso, los logs y las tareas de cada minuto |
| `hecate-cli` (`crates/cli`) | Las órdenes y el binario `hecate` |

- **Pruebas:** las de cada crate en su `tests/`. Las de integración cubren el desbloqueo con la frase y con partes,
  los cambios de protección, las ceremonias de la CA, el alta, la renovación, la revocación, la resolución (también su
  tiempo), el estado, la Access Policy, los cortes a mitad de un cambio, la concurrencia, el borrado de la memoria y que
  ningún secreto llegue a los logs ni al registro; las de la API y del canal local validan cada respuesta contra su
  contrato. Las de manipulación comprueban que mover un dato cifrado a otra ruta, versión o ámbito, o cambiarle un
  byte, impide descifrarlo. Muchas llevan un comentario `// py:` con la prueba de la versión en Python de la que
  vienen, para conservar el comportamiento del port.
- **Vectores** en `hecate/tests/vectors/`: valores conocidos del cifrado y la derivación de claves, los datos
  asociados, la huella, los eventos, los instantes, los textos imprimibles y el esquema de SQLite, que las pruebas
  comparan byte a byte.

| Patrón | Dónde | Por qué |
|---|---|---|
| Event Sourcing | Los aggregates y el registro de accesos | La historia es la auditoría; ningún evento lleva valores |
| Ports y adapters | `CryptoProvider`, `PkiProvider`, los almacenes, el reloj y el registro de accesos (`ports.rs`) | Cambiar de librería, de almacén o a hardware sin tocar el dominio, y probar con dobles |
| Cifrado por sobre | Raíz → clave de ámbito → DEK | Rotar barato y aislar el daño por ámbito |

## Las specs

Cada cambio de Hécate tiene su carpeta en [specs/](../specs/), con su spec, su plan, sus tareas y su guía de
validación:

| Spec | Qué hizo |
|---|---|
| [001-hecate-bootstrap](../specs/001-hecate-bootstrap/) | El sellado: inicializar, desbloquear con la frase o las partes, sellar y el canal local |
| [002-hecate-secrets](../specs/002-hecate-secrets/) | Los secretos: cifrado por sobre, versiones, ciclo de vida y huella |
| [003-hecate-identities](../specs/003-hecate-identities/) | Las identidades: la CA en dos niveles, el alta, la renovación única, la revocación y la API |
| [004-hecate-orthros-rust](../specs/004-hecate-orthros-rust/) | La decisión de pasar Hécate y Orthros a Rust |
| [005-hecate-rust](../specs/005-hecate-rust/) | El port de Hécate a Rust, con el comportamiento de la 001 y la 002 |
| [006-hecate-access-policy](../specs/006-hecate-access-policy/) | La Access Policy, la resolución por la API y el estado agregado |
| [007-hecate-reference](../specs/007-hecate-reference/) | Esta documentación |

Las specs son el registro de su fecha y no se modifican; el diseño vigente está en [docs/hecate.md](../docs/hecate.md).
