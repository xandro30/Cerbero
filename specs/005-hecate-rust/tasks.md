---

description: "Tareas del cambio 005: Hécate en Rust con paridad de las features 001 y 002"
---

# Tasks: Hécate en Rust: migración con paridad de las features 001 y 002

**Input**: Design documents from `/specs/005-hecate-rust/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas trasladadas de Python, las
nuevas y las de vectores se escriben primero y deben fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario. Las 346 funciones de prueba de Python se reparten así:
19 en la fase fundacional, 122 en US1 y 205 en US2. Hay además pruebas nuevas, sin original en Python: las del protocolo
y del registro con JSON no estándar (FR-018), las de la CLI como proceso, las de la CLI de secretos, que la versión en
Python no probaba, y la de tiempos (SC-003).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US4)
- Rutas relativas a la raíz del repositorio; las órdenes de cargo se ejecutan dentro de `hecate/`

## Convenciones para todas las tareas

- **Prueba trasladada:** comprueba lo mismo que su original de Python (`main` en `0287c38`, `hecate/tests/`), lleva el
  comentario `// py: <fichero>::<función>` y rellena su fila en `specs/005-hecate-rust/test-mapping.md`. Las propiedades
  de hypothesis pasan a proptest. Los únicos cambios a propósito son el formato `HEC2` de las partes y el rechazo de las
  entradas que no siguen su formato estándar (FR-018), y se anotan en el mapeo.
- **Prueba nueva:** no lleva `// py:` ni fila en el mapeo; las que cubren un cambio a propósito se citan en la sección
  «Cambios a propósito» del mapeo.
- **Dónde van las pruebas:** las de dominio, en un módulo `#[cfg(test)]` del propio fichero; las de aplicación, en
  `hecate/crates/application/tests/` (usan la infraestructura, que es dev-dependency, y Cargo no deja hacerlo desde las
  pruebas unitarias de `src/` sin duplicar los tipos del crate); las de integración, contrato, protocolo y vectores, en
  `hecate/crates/infrastructure/tests/`; las de la CLI, en `hecate/crates/cli/tests/`; y las de memoria (US3), en `tests/`
  de los crates de dominio y de aplicación, porque comprueban tipos de varios módulos. Los ayudantes comunes están en
  los módulos `testing` de la aplicación y de la infraestructura, tras la feature `test-support` (research R15).
- **Paridad:** campos, valores, códigos y mensajes de las respuestas del canal, filas del registro de accesos, códigos de
  salida, textos que comprueban las pruebas y orden de comprobación, iguales que en Python (research R3 a R12); lo
  cifrado, los datos asociados y el esquema, iguales byte a byte. El JSON (respuestas, logs y lo guardado) se escribe con
  serde_json y es igual en contenido, sin imitar `json.dumps`. Lo que no se sepa, se mira en el código de Python antes de
  borrarlo.
- **Seguridad:** `unsafe_code = "forbid"`; los secretos de paso, en tipos de zeroize o secrecy sin `Clone` ni `Debug`
  (research R14); ningún secreto en logs ni en errores.
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.
  Los ficheros de Python se borran con `rm`.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Comprobar el entorno: rama `refactor/hecate-rust`, `.specify/feature.json` apuntando a `specs/005-hecate-rust`, `rustc 1.99.1`, Docker disponible y cargo-deny 0.20.2 instalado (`cargo install cargo-deny --version 0.20.2 --locked`)
- [X] T002 Crear el workspace de Cargo según plan.md (research R1, R2): `hecate/Cargo.toml` (resolver 3; miembros `crates/domain`, `crates/application`, `crates/infrastructure` y `crates/cli`; `workspace.package` con edition 2024, `rust-version = "1.99"`, `license = "AGPL-3.0-or-later"` y `publish = false`; `workspace.dependencies` con las versiones de plan.md; lints `unsafe_code = "forbid"` y clippy `all` como aviso), `hecate/rust-toolchain.toml` (canal 1.99.1 con rustfmt y clippy) y los cuatro crates con su `Cargo.toml` y un `src/lib.rs` vacío, más `hecate/crates/cli/src/main.rs` con el binario `hecate`; `hecate-application` y `hecate-infrastructure` con la feature `test-support`; `cargo build` compila
- [X] T003 Crear `hecate/deny.toml` según research R17 (licencias permisivas habituales; la de OpenSSL solo en `aws-lc-sys`, y en `ring` si llegara a entrar; los crates propios, privados; avisos de RUSTSEC rechazados; versiones duplicadas como aviso; solo crates.io) y comprobar que `cargo deny check` pasa con el workspace vacío
- [X] T004 Escribir el generador `specs/005-hecate-rust/vectors/generate.py` (research R5 y R7), que importa la Hécate en Python de `hecate/src`, recibe el directorio de vectores y escribe, cada JSON con la cabecera de versión de Python, versión de Unicode y origen `main@0287c38`: `kdf.json` (límites mínimos y perfil `moderate`), `root.json` (protección por frase y por clave de recuperación, con nonce fijo, con material aleatorio que abrir y con las protecciones tal como las guarda `seal_config`), `aad.json`, `envelope.json` (con nonces fijos y con material aleatorio que abrir), `fingerprint.json`, `events.json` (cada tipo de evento de `Secret` y `Key` tal como lo guarda el almacén, incluido un `rotate_every` con caracteres no ASCII), `instants.json` (`isoformat` con y sin microsegundos y con desfases; entradas que `fromisoformat` acepta, con su resultado, o rechaza, con las rarezas de research R6 marcadas como cambio a propósito de FR-018), `values.json` (valores `string`, `json` y `pem` con el resultado de Python, incluidos los casos de research R8, números enormes, escapes de sustitutos sueltos y 32.768 niveles de anidamiento; marcados como cambio a propósito de FR-018, los `json` con `NaN`, `Infinity`, `-Infinity`, números fuera de rango, sustitutos sueltos o más de 127 niveles y los `pem` con espacios que no son ASCII o con un bloque vacío), `printable.json` (rangos de `str.isprintable` en todo Unicode) y `schema.sql` (`sqlite_master` de una base de datos nueva creada por el almacén de Python); además escribe la tabla de rangos en `hecate/crates/domain/src/printable_table.rs`, con una cabecera de fichero generado, en la ruta que calcula desde su propia ubicación en el repositorio
- [X] T005 Ejecutar el generador con la orden del apartado 1 de `specs/005-hecate-rust/quickstart.md` y comprobar que existen los diez ficheros en `hecate/tests/vectors/` y `hecate/crates/domain/src/printable_table.rs`, y que sus cabeceras dicen Unicode 17.0.0
- [X] T006 Crear `specs/005-hecate-rust/test-mapping.md` con el formato de data-model.md: una fila por cada una de las 346 funciones de prueba de `hecate/tests/` (obtenidas con `uv run --package cerbero-hecate pytest hecate/tests --collect-only -q`, una por función aunque esté parametrizada), con la columna de Rust vacía, y una sección final «Cambios a propósito», en lista y no en tabla, para `HEC2` y FR-018

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: errores, instantes, puertos y dobles, proveedor criptográfico, configuración, logs, proceso, almacén base,
canal local y esqueleto de la CLI, que usan las dos historias

**⚠️ CRITICAL**: ninguna historia puede empezar hasta terminar esta fase

### Pruebas de la fase (primero)

- [X] T007 [P] Trasladar `unit/test_config.py` (5 pruebas) a `hecate/crates/infrastructure/tests/config.rs`
- [X] T008 [P] Trasladar `integration/test_crypto_software.py` (7 pruebas, hypothesis a proptest) a `hecate/crates/infrastructure/tests/crypto_software.rs`, y añadir la prueba de `kdf.json` en `hecate/crates/infrastructure/tests/vectors_kdf.rs`
- [X] T009 [P] Trasladar `unit/test_hardening.py` (7 pruebas: credencial del par en Linux, permisos del socket en otros sistemas, par rechazado cerrado sin leer, sin volcados de memoria, `mlockall` en Linux con aviso si falla y aviso en macOS) a `hecate/crates/infrastructure/tests/hardening.rs`
- [X] T010 [P] Añadir en `hecate/crates/domain/src/instant.rs` las pruebas de `instants.json`: cada instante da su `isoformat` y cada entrada se acepta (con el mismo resultado) o se rechaza igual que en Python, salvo las marcadas como cambio a propósito, que se rechazan
- [X] T011 [P] Añadir `hecate/crates/infrastructure/tests/schema.rs`: una base de datos nueva creada por el almacén en Rust tiene exactamente el `sqlite_master` de `schema.sql`, y la conexión usa `synchronous = FULL` y `secure_delete = ON`
- [X] T012 [P] Añadir `hecate/crates/infrastructure/tests/protocol.rs` (pruebas nuevas, FR-018): una petición en JSON estándar se lee con su `op`, sus `args` y su `request_id`; `NaN`, `Infinity`, `1e400`, escapes de sustitutos sueltos, 128 niveles de anidamiento, BOM, UTF-16 y una línea vacía dan `validation` «request is not valid JSON»; los demás errores de forma dan los mensajes de Python en su orden; los sobres `completed` y `failed` llevan los campos en el orden de Python, e `internal` no revela la causa

### Implementación de la fase

- [X] T013 [P] Implementar `hecate/crates/domain/src/sealing/errors.rs` (como `domain/sealing/errors.py`): los errores de Hécate con sus códigos (`validation`, `auth`, `rate_limited` con `retry_after_s`, `not_found`, `conflict` con el estado y la versión actual, `internal`), `retryable` como en Python y los mensajes de la versión en Python
- [X] T014 [P] Implementar `hecate/crates/domain/src/instant.rs` con `time` (research R6): `isoformat` sin fracción si los microsegundos son 0 y con seis cifras si no, desfase `+HH:MM`, y un analizador del ISO 8601 extendido de research R6 (`AAAA-MM-DD`, o fecha y hora con `T` o espacio, fracción truncada al microsegundo y desfase opcional `Z` o `±HH:MM`; sin desfase, UTC), que lee también los instantes guardados
- [X] T015 Implementar `hecate/crates/application/src/ports.rs` (research R13): los traits `CryptoProvider`, `Clock`, `SealRepository`, `AccessLog`, `EventStore`, `MaterialStore` y `SecretStore`, los errores `CryptoError` y `ConcurrencyConflict`, los registros `EventRecord`, `KeyMaterial` y `SecretMaterial`, y las transacciones como cierres con un manejador de escritura
- [X] T016 Implementar `hecate/crates/application/src/testing.rs` (feature `test-support`) con los dobles de `fakes.py` que solo dependen de los puertos: `FakeClock`, `InMemorySealRepository`, `InMemoryAccessLog` e `InMemorySecretStore`
- [X] T017 Implementar `hecate/crates/infrastructure/src/crypto.rs` (research R3): aleatorios con `SystemRandom`, Argon2id con libsodium-rs, AES-256-GCM con nonce explícito, HKDF-SHA256 con sal vacía, HMAC-SHA256 y Shamir 2 de 3 de 32 bytes con vsss-rs en GF(256), con un adaptador de `rand_core` sobre `SystemRandom`
- [X] T018 [P] Implementar `hecate/crates/infrastructure/src/clock.rs` (UTC truncado al microsegundo), `hecate/crates/infrastructure/src/config.rs` (`HECATE_DATA_DIR` obligatoria, `HECATE_SOCKET_PATH` con `control.sock` por defecto, `HECATE_KDF_PROFILE` `sensitive` o `moderate`, directorio de datos con permisos 0700 y base de datos `hecate.db`, con los mensajes de Python), `hecate/crates/infrastructure/src/logs.rs` (research R11: una línea JSON por evento con serde_json y las claves de Python) y `hecate/crates/infrastructure/src/process.rs` (research R12)
- [X] T019 Implementar la base del almacén en `hecate/crates/infrastructure/src/store/mod.rs`: crea `hecate.db` con permisos 0600 antes de abrirla; conexión con espera de 10 s si la base de datos está ocupada, transacciones explícitas, `synchronous = FULL` y `secure_delete = ON`; y el esquema con las mismas sentencias SQL que la versión en Python (tablas y disparadores append-only de `access_log` y `events`)
- [X] T020 Implementar `hecate/crates/infrastructure/src/control/protocol.rs` (research R8, R9): cada línea se lee con serde_json como JSON estándar en UTF-8 y lo demás da `validation` «request is not valid JSON» (FR-018); después, en el orden de Python, «request must be an object with a string 'op'», «'args' must be an object» y «'request_id' must be a string»; los argumentos que Python pasa sin tocar a los servicios siguen siendo valores JSON; sobres `completed` y `failed` con los campos en el orden de Python (`status`, `data`, `error` con `code`, `message`, `retryable` y, si hay espera, `retry_after_s`; en los conflictos, `state` y `current_version` en `data`; `meta` con `request_id` y `duration_ms` en milisegundos enteros); `internal` con «internal error» y `retryable` verdadero; y escritura con serde_json y salto de línea
- [X] T021 Implementar `hecate/crates/infrastructure/src/control/server.rs` (research R9: tokio, socket privado, credencial del par en Linux, una petición por línea y una operación detrás de otra; una línea de más de 1 MiB responde `validation` «request too large» y cierra la conexión; tabla de operaciones que rellenan las historias y `unknown operation '<op>'` para las demás), `hecate/crates/infrastructure/src/control/client.rs` (conexión y petición, con el error de canal no disponible de Python) y `hecate/crates/infrastructure/src/testing.rs` (feature `test-support`) con `short_dir` y `ServerThread` (el servidor en un hilo con el manejador que se le pase)
- [X] T022 Implementar el esqueleto de la CLI según [contracts/cli.md](contracts/cli.md) y research R15: en `hecate/crates/cli/src/lib.rs`, todas las órdenes y opciones con clap, la configuración desde el entorno (salida 2 con `error: <mensaje>`), la impresión de errores `error (<código>): <mensaje>` con espera y estado, y el trait de terminal que reciben las órdenes (pedir una línea, pedir sin eco y escribir en stdout y stderr); en `hecate/crates/cli/src/main.rs`, el terminal real: sin eco con `nix::sys::termios` si la entrada es una terminal y una línea de stdin si no; el fin de la entrada cancela como `EOFError` en Python, y Ctrl-C llega por una ayuda de señales de la infraestructura (tokio, research R13) que restaura el eco y cancela la orden en curso, como `KeyboardInterrupt`

**Checkpoint**: pasan las 19 pruebas trasladadas de la fase, las del protocolo, `instants.json` y el esquema.

---

## Phase 3: User Story 1 - El admin arranca, desbloquea y sella Hécate como hasta ahora (Priority: P1) 🎯 MVP

**Goal**: paridad con la feature 001: inicializar, desbloquear con la frase o con dos partes `HEC2`, sellar, consultar el
estado y cambiar la frase o las partes, con las mismas esperas y registros.

**Independent Test**: pasan las 122 pruebas de 001 trasladadas, las nuevas de la CLI como proceso y `root.json`, y el
recorrido de 001 de quickstart.md (apartado 6) se completa con la nueva CLI.

### Tests for User Story 1 ⚠️

- [X] T023 [US1] Añadir a los módulos de soporte de pruebas lo que depende del sellado, como en `fakes.py`: `make_service` e `initialize` en `hecate/crates/application/src/testing.rs` y la parte de sellado de `SqliteStack` en `hecate/crates/infrastructure/src/testing.rs`
- [X] T024 [P] [US1] Trasladar `unit/domain/test_attempts.py` (5) a `hecate/crates/domain/src/sealing/attempts.rs`
- [X] T025 [P] [US1] Trasladar `unit/domain/test_passphrase.py` (4) a `hecate/crates/domain/src/sealing/passphrase.rs`
- [X] T026 [P] [US1] Trasladar `unit/domain/test_share.py` (5, hypothesis a proptest) a `hecate/crates/domain/src/sealing/share.rs`, con el formato `HEC2` de [contracts/recovery-share.md](contracts/recovery-share.md) y una prueba más: una parte `HEC1` da `validation` (cambio a propósito, anotado en el mapeo)
- [X] T027 [P] [US1] Añadir `hecate/crates/infrastructure/tests/vectors_root.rs` con `root.json`: los mismos datos asociados, el mismo cifrado con nonce fijo, la apertura del material generado por Python, y que Rust lee las protecciones guardadas por Python y las vuelve a guardar con el mismo contenido JSON
- [X] T028 [P] [US1] Trasladar `unit/application/test_initialize.py` (8) a `hecate/crates/application/tests/initialize.rs`
- [X] T029 [P] [US1] Trasladar `unit/application/test_unseal_passphrase.py` (7) a `hecate/crates/application/tests/unseal_passphrase.rs`
- [X] T030 [P] [US1] Trasladar `unit/application/test_unseal_shares.py` (7) a `hecate/crates/application/tests/unseal_shares.rs`
- [X] T031 [P] [US1] Trasladar `unit/application/test_seal.py` (4) a `hecate/crates/application/tests/seal.rs`
- [X] T032 [P] [US1] Trasladar `unit/application/test_passphrase_change.py` (6) a `hecate/crates/application/tests/passphrase_change.rs`
- [X] T033 [P] [US1] Trasladar `unit/application/test_shares_rekey.py` (9) a `hecate/crates/application/tests/shares_rekey.rs`
- [X] T034 [P] [US1] Trasladar `unit/application/test_sealing_public_ops.py` (10) a `hecate/crates/application/tests/sealing_public_ops.rs`
- [X] T035 [P] [US1] Trasladar `integration/test_sqlite_store.py` (4) a `hecate/crates/infrastructure/tests/sqlite_store.rs`
- [X] T036 [P] [US1] Trasladar `integration/test_restart.py` (3), `integration/test_init_crash.py` (2), `integration/test_crash_during_change.py` (2) y `integration/test_concurrent_unseal.py` (1) a `hecate/crates/infrastructure/tests/restart.rs`, `init_crash.rs`, `crash_during_change.rs` y `concurrent_unseal.rs`
- [X] T037 [P] [US1] Trasladar `integration/test_no_secret_leak.py` (2) y `integration/test_control_channel.py` (3) a `hecate/crates/infrastructure/tests/no_secret_leak.rs` y `control_channel.rs`, y añadir en `control_channel.rs` una prueba nueva (FR-018): una petición que no es JSON estándar no deja fila en el registro de accesos
- [X] T038 [P] [US1] Trasladar `contract/test_init_ops.py` (5), `contract/test_unseal_ops.py` (8), `contract/test_seal_op.py` (3), `contract/test_recovery_ops.py` (5) y `contract/test_rekey_ops.py` (4) a `hecate/crates/infrastructure/tests/contract_init_ops.rs`, `contract_unseal_ops.rs`, `contract_seal_op.rs`, `contract_recovery_ops.rs` y `contract_rekey_ops.rs`
- [X] T039 [P] [US1] Trasladar de `contract/test_status_op.py` las cuatro pruebas del sellado (`test_status_when_uninitialized`, `test_status_when_sealed_unsealed_and_after_recovery`, `test_status_reports_failures_and_the_pending_wait` y `test_internal_error_hides_the_cause_and_is_retryable`) a `hecate/crates/infrastructure/tests/contract_status_op.rs`
- [X] T040 [P] [US1] Trasladar `integration/test_cli.py` (11) a `hecate/crates/cli/tests/cli.rs`: las órdenes se llaman dentro del proceso, como en Python, con un terminal guionizado que responde a la petición de partes con las que la CLI acaba de imprimir y con el servidor en un hilo (`ServerThread`); la interrupción, con la cancelación del terminal; las partes, en `HEC2`
- [X] T041 [P] [US1] Añadir `hecate/crates/cli/tests/process.rs` (pruebas nuevas) con assert_cmd sobre el binario `hecate`: salida 2 y `error: …` sin `HECATE_DATA_DIR`; salida 2 con una orden o una opción que no existe; salida 1 y `error: …` sin canal; y un recorrido de humo: `hecate serve` en marcha, `hecate status` y la parada con SIGTERM (enviada con `nix`), que sale con 0

### Implementation for User Story 1

- [X] T042 [P] [US1] Implementar en `hecate/crates/domain/src/sealing/` `state.rs` (sin inicializar, sellada y desbloqueada), `passphrase.rs` (al menos 12 caracteres, «passphrase must have at least 12 characters», en un tipo que se borra al soltarse), `attempts.rs` (contador común de frase y partes que sobrevive a reinicios: sin espera hasta el segundo fallo seguido; desde el tercero, 1, 2, 4 y 8 minutos, y después 15) y `access_log.rs` (operación, vía y resultado con los valores de Python)
- [X] T043 [P] [US1] Implementar `hecate/crates/domain/src/sealing/share.rs` según [contracts/recovery-share.md](contracts/recovery-share.md): cuerpo de 42 bytes (versión `0x02`, índice de 1 a 3, juego de 4 bytes y 32 valores), suma de 4 bytes, Crockford Base32 en grupos de cuatro tras `HEC2`, y lectura con limpieza y confusiones
- [X] T044 [US1] Implementar `hecate/crates/domain/src/sealing/protection.rs`: protección por frase (límites de Argon2id, sal, AEAD, nonce y texto cifrado) y por recuperación (juego, umbral 2 de 3), configuración del sellado con su versión, y datos asociados `{"config_version","protection","purpose":"root"}` en JSON ordenado y compacto, iguales byte a byte
- [X] T045 [US1] Implementar `hecate/crates/application/src/sealing/settings.rs` (perfiles `sensitive`, de 4 pasadas y 1 GiB, y `moderate`, de 3 pasadas y 256 MiB; caducidad de 15 minutos de las inicializaciones y repartos pendientes) y `hecate/crates/application/src/sealing/root_holder.rs` (la raíz solo mientras está desbloqueada; se borra al sellar)
- [X] T046 [US1] Implementar `hecate/crates/application/src/sealing/service.rs`: inicializar (empezar, confirmar con dos partes y abortar), desbloquear con frase o partes, sellar, cambiar la frase, repartir de nuevo las partes (empezar, confirmar y abortar), estado, `root()` y `verify_proof`, con el contador y las esperas, los registros del sellado (solo los errores de Hécate dejan fila, research R10) y los mensajes de Python
- [X] T047 [US1] Implementar `SealRepository` y `AccessLog` sobre SQLite en `hecate/crates/infrastructure/src/store/sealing.rs`: tablas `seal_config` (las protecciones en JSON, igual en contenido que en Python), `attempts` y `access_log`, con transacciones y sin modificar ni borrar el registro
- [X] T048 [US1] Registrar en `hecate/crates/infrastructure/src/control/operations.rs` las operaciones del sellado (`status`, `init.begin`, `init.confirm`, `init.abort`, `unseal`, `seal`, `passphrase.change`, `shares.rekey.begin`, `shares.rekey.confirm` y `shares.rekey.abort`), con sus comprobaciones de argumentos y la fila de rechazo con su operación y su vía (research R8, R10), y los campos de `status`
- [X] T049 [US1] Implementar en `hecate/crates/cli/src/` las órdenes `serve`, `status`, `init`, `unseal`, `seal`, `passphrase change` y `shares rekey` sobre el trait de terminal, con las salidas de Python ([contracts/cli.md](contracts/cli.md)), la cancelación con `init.abort` o `shares.rekey.abort` donde Python la hace, el arranque de `serve` (proceso endurecido, siempre sellada) y su parada ordenada con SIGINT o SIGTERM, que sale con 0 (señales con tokio en la infraestructura, research R13)
- [X] T050 [US1] Pasar las pruebas de US1 y `root.json` con `cargo test --workspace`, completar las 122 filas de US1 en `specs/005-hecate-rust/test-mapping.md` y recorrer el apartado 6 de `specs/005-hecate-rust/quickstart.md`

**Checkpoint**: el sellado de 001 funciona en Rust con la nueva CLI.

---

## Phase 4: User Story 2 - El admin gestiona secretos y una identidad los resuelve como hasta ahora (Priority: P1)

**Goal**: paridad con la feature 002: guardar, versionar, listar, consultar, revocar, restaurar, promover y destruir
secretos, y resolverlos por identidad, con los mismos formatos cifrados.

**Independent Test**: pasan las 205 pruebas de 002 trasladadas, las nuevas de la CLI de secretos y los vectores
`envelope.json`, `aad.json`, `fingerprint.json`, `events.json`, `values.json` y `printable.json`, y el recorrido de 002 de
quickstart.md (apartado 7) se completa.

### Tests for User Story 2 ⚠️

- [X] T051 [US2] Añadir a los módulos de soporte de pruebas lo que depende de los secretos, como en `fakes.py`: `SecretsRig` en `hecate/crates/application/src/testing.rs` y la parte de secretos de `SqliteStack` en `hecate/crates/infrastructure/src/testing.rs`
- [X] T052 [P] [US2] Trasladar `unit/domain/test_secret_path.py` (7, hypothesis a proptest) a `hecate/crates/domain/src/secrets/path.rs`
- [X] T053 [P] [US2] Trasladar `unit/domain/test_value_type.py` (9) a `hecate/crates/domain/src/secrets/value_type.rs` y añadir las pruebas de `values.json`, donde los casos marcados como cambio a propósito se esperan rechazados; los casos de `test_string_rejects_empty_and_non_text` que Rust no puede recibir (bytes y sustitutos sueltos: el tipo `&str` los impide; en el canal, un `value` que no es texto lo rechaza la comprobación de argumentos y un sustituto suelto, la lectura del JSON) se anotan en el mapeo
- [X] T054 [P] [US2] Trasladar `unit/domain/test_secret_aggregate.py` (23, hypothesis a proptest) a `hecate/crates/domain/src/secrets/secret.rs`
- [X] T055 [P] [US2] Añadir en `hecate/crates/domain/src/events.rs` las pruebas de `events.json` (cada evento se lee y se guarda con el mismo contenido JSON) y en `hecate/crates/domain/src/printable.rs` la de `printable.json` (la tabla coincide con los rangos de Python)
- [X] T056 [P] [US2] Trasladar `unit/application/test_secret_create.py` (13) a `hecate/crates/application/tests/secrets_create.rs`
- [X] T057 [P] [US2] Trasladar `unit/application/test_secret_list_get.py` (10) a `hecate/crates/application/tests/secrets_list_get.rs`
- [X] T058 [P] [US2] Trasladar `unit/application/test_secret_rotate.py` (16) a `hecate/crates/application/tests/secrets_rotate.rs`
- [X] T059 [P] [US2] Trasladar `unit/application/test_secret_lifecycle.py` (15) a `hecate/crates/application/tests/secrets_lifecycle.rs`
- [X] T060 [P] [US2] Trasladar `unit/application/test_secret_destroy.py` (15) a `hecate/crates/application/tests/secrets_destroy.rs`
- [X] T061 [P] [US2] Trasladar `unit/application/test_secret_resolve.py` (15) a `hecate/crates/application/tests/secrets_resolve.rs`
- [X] T062 [P] [US2] Trasladar `integration/test_envelope.py` (11, hypothesis a proptest) a `hecate/crates/infrastructure/tests/envelope.rs` y añadir `hecate/crates/infrastructure/tests/vectors_envelope.rs` con `envelope.json`, `aad.json` y `fingerprint.json`
- [X] T063 [P] [US2] Trasladar `integration/test_secret_store.py` (10) y `integration/test_secret_resolve_sqlite.py` (3) a `hecate/crates/infrastructure/tests/secret_store.rs` y `secret_resolve_sqlite.rs`, con la prueba de tiempo de SC-003 de `test_secret_resolve_sqlite.py` (con Hécate desbloqueada sobre SQLite, resolver 10 secretos en una sola llamada tarda menos de 1 s) en `hecate/crates/infrastructure/tests/performance.rs`
- [X] T064 [P] [US2] Trasladar `integration/test_secret_concurrency.py` (2), `integration/test_secret_crash.py` (4), `integration/test_secret_crypto_shred.py` (3) y `integration/test_secret_no_leak.py` (3) a `hecate/crates/infrastructure/tests/secret_concurrency.rs`, `secret_crash.rs`, `secret_crypto_shred.rs` y `secret_no_leak.rs`
- [X] T065 [P] [US2] Trasladar `integration/test_access_log_migration.py` (5) a `hecate/crates/infrastructure/tests/access_log_migration.rs`
- [X] T066 [P] [US2] Trasladar `contract/test_secret_ops.py` (18) y `contract/test_secret_lifecycle_ops.py` (15) a `hecate/crates/infrastructure/tests/contract_secret_ops.rs` y `contract_secret_lifecycle_ops.rs`
- [X] T067 [P] [US2] Trasladar de `contract/test_status_op.py` las ocho pruebas de los recuentos de caducidad (de `test_status_counts_expired_and_expiring_soon_secrets_without_paths` a `test_a_handler_without_secrets_keeps_the_original_status_fields`) a `hecate/crates/infrastructure/tests/contract_status_op.rs`
- [X] T068 [P] [US2] Añadir `hecate/crates/cli/tests/cli_secrets.rs` (pruebas nuevas: la versión en Python no prueba estas órdenes), con el terminal guionizado y el servidor en un hilo: cada orden de secretos con la salida y el código de salida de la CLI de Python; el valor pedido sin eco o leído de `--from-file`, con `\r\n` y `\r` convertidos en `\n`; el error de lectura del fichero, con su motivo; y la cancelación

### Implementation for User Story 2

- [X] T069 [P] [US2] Implementar `hecate/crates/domain/src/printable.rs` sobre `printable_table.rs`: un texto es imprimible si lo son todos sus caracteres, con la regla de `str.isprintable`, y las longitudes se cuentan en caracteres
- [X] T070 [P] [US2] Implementar `hecate/crates/domain/src/secrets/path.rs`: identidad `[a-z][a-z0-9-]{0,62}`, segmentos `[a-z0-9_-]{1,64}`, sin segmentos vacíos, prefijo `secret://`, referencias con `@<n>` (`[1-9][0-9]{0,8}`) y el dueño es el primer segmento
- [X] T071 [P] [US2] Implementar `hecate/crates/domain/src/secrets/value_type.rs` (research R8): todo valor no vacío («the value must not be empty») y de 65.536 bytes como mucho en UTF-8 («the value is larger than 64 KiB»); `json`, JSON estándar: se lee con serde_json como `Value`, así que se rechazan `NaN`, `Infinity`, los números fuera de rango, los sustitutos sueltos y más de 127 niveles («the value is not valid JSON»; FR-018); `pem`, la expresión de Python `-----BEGIN ([A-Z0-9 ]+)-----\s*\n(?P<body>[A-Za-z0-9+/=\s]+?)\s*\n-----END \1-----` bloque a bloque sobre el valor sin espacios en los extremos, con `\s` y los espacios solo de RFC 7468 (espacio, tabulador, CR, LF, VT y FF; FR-018), y cada cuerpo sin espacios en base64 estricto, como `b64decode(validate=True)` (relleno solo al final), y no vacío («a PEM block is empty»; FR-018); mensajes de Python («the value is not a valid PEM», «a PEM block does not contain valid base64», «the value type must be 'string', 'json' or 'pem'»)
- [X] T072 [US2] Implementar `hecate/crates/domain/src/secrets/secret.rs` (transiciones `active` → `deprecated` | `revoked`, `deprecated` → `active` | `revoked`, `revoked` → `deprecated` | `destroyed`; una sola `active`; una `deprecated` cuenta como `revoked` al superar la gracia de 24 h; restaurar la deja `deprecated` y reinicia la gracia; `max_versions` de 2 a 100, 10 por defecto; versión del stream por evento), `hecate/crates/domain/src/keys/key.rs` (aggregate `Key`) y `hecate/crates/domain/src/events.rs` (los eventos y su JSON, con las claves y los valores de Python, escrito con serde_json)
- [X] T073 [US2] Implementar `hecate/crates/application/src/secrets/envelope.rs` (research R3): clave de ámbito cifrada con la raíz, valor cifrado con una clave de versión cifrada con la de ámbito, datos asociados por capa iguales byte a byte y huella de 8 bytes en grupos de cuatro
- [X] T074 [US2] Implementar `hecate/crates/application/src/secrets/service.rs`: crear, añadir versión, listar, consultar, revocar, restaurar, promover, destruir con prueba (`verify_proof` del sellado), resolver (caso de uso interno, sin canal; hasta 100 referencias) y recuentos de caducidad; `rotate_every` de 1 a 64 caracteres imprimibles, `expires_at` informativo con su desfase y leído en ISO 8601 extendido (research R6, FR-018); cada error deja fila como fallo (research R10) y el orden de comprobación es el de Python (research R8)
- [X] T075 [US2] Implementar `EventStore` y `MaterialStore` sobre SQLite en `hecate/crates/infrastructure/src/store/secrets.rs` (eventos con versión esperada y conflicto, streams, `key_material` y `secret_material`, borrado del material) y, al abrir cualquier conexión (`open_database` en `hecate/crates/infrastructure/src/store/mod.rs`, como `_connect` en Python), la migración que añade `subject`, `path` y `version` a un `access_log` antiguo
- [X] T076 [US2] Registrar en `hecate/crates/infrastructure/src/control/operations.rs` las operaciones de secretos (`secrets.create`, `secrets.add_version`, `secrets.list`, `secrets.get`, `secrets.revoke`, `secrets.restore`, `secrets.promote` y `secrets.destroy`), con sus comprobaciones de argumentos y su fila de rechazo, y los recuentos `secrets_expired` y `secrets_expiring_soon` de `status`, disponibles también sellada
- [X] T077 [US2] Implementar en `hecate/crates/cli/src/` las órdenes `secrets create`, `list`, `get`, `put`, `revoke`, `restore`, `promote` y `destroy` con las salidas de Python; `--from-file` lee el fichero como texto UTF-8 y convierte `\r\n` y `\r` en `\n`, como `Path.read_text`, y si no puede, escribe `error: cannot read <fichero>: <motivo>` (`not found`, `permission denied`, `is a directory`, `not a directory`, `not valid UTF-8` o el texto del error del sistema) y sale con 1
- [X] T078 [US2] Pasar las pruebas de US2 y sus vectores con `cargo test --workspace`, completar las 205 filas de US2 y la sección «Cambios a propósito» de `specs/005-hecate-rust/test-mapping.md` y recorrer el apartado 7 de `specs/005-hecate-rust/quickstart.md`

**Checkpoint**: los secretos de 002 funcionan en Rust con los mismos formatos cifrados.

---

## Phase 5: User Story 3 - Los secretos de paso no se quedan en memoria (Priority: P2)

**Goal**: cada secreto de paso vive en un tipo que se borra al soltarse, también si la operación falla, y la raíz solo
vive en memoria mientras Hécate está desbloqueada (FR-007).

**Independent Test**: pasan las pruebas de memoria de los dos crates y la revisión no encuentra copias fuera de esos
tipos.

### Tests for User Story 3 ⚠️

- [X] T079 [P] [US3] Añadir `hecate/crates/domain/tests/memory.rs` y `hecate/crates/application/tests/memory.rs`: cada tipo de la tabla «Secreto de paso» de data-model.md implementa `ZeroizeOnDrop` (comprobado al compilar) y `zeroize` deja sus bytes a cero; el guardián de la raíz no la conserva tras sellar
- [X] T080 [P] [US3] Añadir en la documentación de cada uno de esos tipos, en `hecate/crates/domain/src/` y `hecate/crates/application/src/`, pruebas de documentación `compile_fail` que demuestran que no implementan `Clone` ni `Debug`

### Implementation for User Story 3

- [X] T081 [US3] Corregir en `hecate/crates/` cualquier tipo de secreto de paso que no cumpla T079 o T080, y comprobar que una operación que falla a medias suelta (y borra) los secretos que ya usó
- [X] T082 [US3] Revisar en `hecate/crates/` dónde se crean, se exponen (`expose_secret`) o se copian (`to_vec`, `clone`, `format!`) los secretos de paso, eliminar cualquier copia fuera de los tipos que se borran y anotar el resultado de la revisión para la descripción de la PR

**Checkpoint**: FR-007 se cumple por diseño y lo comprueban las pruebas.

---

## Phase 6: User Story 4 - La versión en Python desaparece y los docs describen la nueva (Priority: P3)

**Goal**: sin Hécate en Python en el repositorio ni en el workspace de uv, con las comprobaciones de calidad en verde y
los docs consolidados.

**Independent Test**: las órdenes del apartado 9 de quickstart.md no dan salida y las de calidad pasan.

- [X] T083 [US4] Borrar con `rm` la Hécate en Python: `hecate/src/`, `hecate/tests/unit/`, `hecate/tests/integration/`, `hecate/tests/contract/`, `hecate/tests/fakes.py` y el resto de ficheros `.py` de `hecate/tests/`, y `hecate/pyproject.toml`; se quedan `hecate/tests/vectors/` y `hecate/README.md`
- [X] T084 [US4] Quitar `"hecate"` de los miembros de `[tool.uv.workspace]` en `pyproject.toml`, regenerar `uv.lock` con `uv lock` y comprobar `uv lock --check`
- [X] T085 [P] [US4] Actualizar `README.md`: en el árbol, `hecate/` como componente en Rust; en el párrafo de los proyectos, que Hécate es un workspace de Cargo con sus crates y sus pruebas, y la referencia de lenguajes al ADR 0005 en lugar del 0002
- [X] T086 [P] [US4] Actualizar `docs/hecate.md`: en el apartado 1, las librerías (aws-lc-rs para AES-256-GCM, HKDF y HMAC; libsodium para Argon2id; vsss-rs para Shamir, sin auditoría pública, con su enlace al riesgo aceptado); en el apartado 4, que el valor de un secreto se pide sin eco o se lee de un fichero, sin nombrar `getpass`; en el apartado 7, la organización por crates y el adapter de software; en el apartado 12, el riesgo de memoria de la versión en Rust (la raíz sigue en memoria mientras está desbloqueada; los secretos de paso se borran al usarse)
- [X] T087 [P] [US4] Actualizar `DECISIONS.md`: el riesgo aceptado de memoria reescrito para la versión en Rust (con la misma mitigación del adapter de hardware) y la fila de Shamir sin PyCryptodome; la fila de la feature 003 en «Cambios pausados» no se toca
- [X] T088 [US4] Pasar en el workspace de `hecate/Cargo.toml` `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo deny check` sin avisos, corrigiendo lo que salga
- [X] T089 [US4] Ejecutar las órdenes del apartado 9 de `specs/005-hecate-rust/quickstart.md` y la revisión a mano que describe (README, doc de Hécate, DECISIONS y fila de la 003)

**Checkpoint**: solo queda la versión en Rust y los docs la describen.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T090 Ejecutar la suite en el contenedor Linux del apartado 4 de `specs/005-hecate-rust/quickstart.md` (FR-017), añadiendo a la orden los paquetes que falten para compilar
- [X] T091 Comprobar el mapeo con el apartado 5 de `specs/005-hecate-rust/quickstart.md`: 346 filas en `specs/005-hecate-rust/test-mapping.md`, 346 comentarios `// py:` distintos en `hecate/crates/`, ninguna fila sin prueba en Rust y la sección «Cambios a propósito» con `HEC2` y FR-018
- [X] T092 Recorrer de nuevo `specs/005-hecate-rust/quickstart.md` de principio a fin (pruebas, calidad, recorridos de 001 y 002 y comprobaciones de seguridad) con la versión final
- [X] T093 Proponer al propietario el mensaje del commit de squash (`refactor:`, en inglés, con una línea por bloque y sin `Co-Authored-By`) y la descripción de la PR en español, que recojan `hecate/`, `pyproject.toml`, `uv.lock`, `README.md`, `docs/hecate.md`, `DECISIONS.md` y `specs/005-hecate-rust/`; el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias. T005 necesita T002 y T004; los vectores se generan mientras la versión en
  Python existe.
- **Foundational (Phase 2)**: depende del Setup y bloquea todas las historias.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2 (Phase 4)**: depende de US1, porque los secretos necesitan la raíz y `verify_proof` del sellado, el canal y la
  CLI.
- **US3 (Phase 5)**: depende de US1 y US2, que crean los tipos de secretos de paso.
- **US4 (Phase 6)**: depende de US1, US2 y US3: Python solo se borra cuando la versión en Rust cumple la paridad.
- **Polish (Phase 7)**: al final.

### Within Each Phase

- Las pruebas (trasladadas, nuevas y de vectores) van primero y deben fallar; después, dominio, aplicación,
  infraestructura y CLI, en ese orden; la tarea de cierre (T050, T078) pasa todo y rellena el mapeo.
- T016 necesita los puertos de T015; los ayudantes de `hecate/crates/infrastructure/src/testing.rs` van en T021, con el
  servidor.
- T023 va antes de las demás pruebas de US1 y T051, antes de las de US2: añaden los ayudantes que usan.
- T067 amplía el fichero de T039 (`contract_status_op.rs`); T048 y T076 amplían el mismo `server.rs`; T049 y T077, la
  misma CLI.

### Parallel Opportunities

- Fundacional: T007 a T012 entre sí; T013, T014 y T018 entre sí.
- US1: T024 a T041 entre sí, tras T023; T042 y T043.
- US2: T052 a T068 entre sí, tras T051; T069 a T071.
- US3: T079 y T080.
- US4: T085, T086 y T087.

---

## Parallel Example: User Story 1

```bash
Task: "Trasladar unit/domain/test_share.py a hecate/crates/domain/src/sealing/share.rs (HEC2)"
Task: "Trasladar unit/application/test_unseal_shares.py a hecate/crates/application/tests/unseal_shares.rs"
Task: "Trasladar contract/test_unseal_ops.py a hecate/crates/infrastructure/tests/contract_unseal_ops.rs"
Task: "Trasladar integration/test_cli.py a hecate/crates/cli/tests/cli.rs (terminal guionizado)"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup, con los vectores generados, y la fase fundacional.
2. US1: el sellado de 001 en Rust.
3. **Revisión del propietario:** pruebas de US1, `root.json` y el recorrido de 001.

### Siguientes incrementos

1. US2 (secretos), con su revisión: pruebas, vectores y el recorrido de 002.
2. US3 (memoria) y US4 (retirada de Python y docs).
3. Polish: Linux, mapeo, quickstart completo y mensaje de commit.

El cambio se entrega entero en una PR con un commit de squash `refactor:`; los incrementos son puntos de revisión.

---

## Phase 8: Convergence

- [X] T094 Registrar en «Cambios a propósito» de `specs/005-hecate-rust/test-mapping.md` los cambios de comportamiento de la CLI en caminos de error que ya se comunicaron al propietario (cancelar con Ctrl-C o fin de entrada fuera de la confirmación de partes imprime `cancelled` y sale con 1; un fallo al arrancar `hecate serve` imprime `error: …` y sale con 1; un `--from-file` que no se puede leer da un motivo neutro en lugar del nombre de la excepción), con las pruebas que los cubren, o igualarlos a Python si el propietario lo prefiere, per Edge Cases / FR-008 (partial)
- [X] T095 Quitar `secrecy` de `[workspace.dependencies]` en `hecate/Cargo.toml` y de la pila de `specs/005-hecate-rust/plan.md` y research R0 y R14, porque ningún crate lo usa y los secretos de paso viven en tipos de zeroize, per plan: pila tecnológica (partial)
- [X] T096 Añadir en `hecate/crates/infrastructure/src/control/protocol.rs` pruebas unitarias de `erase` (pone a cero los textos de un JSON anidado, en objetos y listas, y deja el resto) y de `append_erasing` y `ErasingWriter` (conservan el contenido al crecer), per Constitution IV / FR-007 (partial)
