---

description: "Tareas de implementación del arranque de Hécate"
---

# Tasks: Arranque de Hécate

**Input**: Design documents from `/specs/001-hecate-bootstrap/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/control-protocol.md](contracts/control-protocol.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero
y deben fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario para implementar y probar cada una por separado.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US5)
- Rutas relativas a la raíz del repositorio; el paquete es `hecate/` (miembro `cerbero-hecate` del workspace)

## Convenciones para todas las tareas

- Código, identificadores y mensajes en inglés; Python 3.15 con tipos.
- Ningún secreto (raíz, frase, partes, clave de recuperación) en logs, excepciones, `repr` ni en disco (FR-014).
- Los tiempos se piden siempre al port `Clock`; la aleatoriedad, al port `CryptoProvider`.
- Las pruebas unitarias usan los dobles de `hecate/tests/fakes.py`; las de integración, los adapters reales con
  los límites de Argon2id mínimos (configurados en `SealingService`) para que sean rápidas.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: dependencias, herramientas y esqueleto del paquete

- [X] T001 Añadir a `hecate/pyproject.toml` las dependencias `cryptography`, `pynacl` y `pycryptodome`, y el script `hecate = "hecate.infrastructure.cli.main:main"` en `[project.scripts]`; ejecutar `uv lock`
- [X] T002 Añadir al `pyproject.toml` de la raíz `[dependency-groups] dev = ["pytest", "hypothesis"]` (ADR 0004) y la configuración `[tool.pytest.ini_options]` en `hecate/pyproject.toml` con `testpaths = ["tests"]`; ejecutar `uv sync --all-packages`
- [X] T003 [P] Crear los paquetes vacíos (`__init__.py`) de `hecate/src/hecate/domain/sealing/`, `hecate/src/hecate/application/sealing/`, `hecate/src/hecate/infrastructure/{crypto,store,control,cli}/` y la carpeta `hecate/tests/contract/` según la estructura de plan.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: piezas que necesitan todas las historias: estado, errores, ports, adapters, canal local y `status`

**⚠️ CRITICAL**: ninguna historia puede empezar hasta terminar esta fase

### Pruebas de la fase (primero)

- [X] T004 [P] Pruebas del adapter de criptografía en `hecate/tests/integration/test_crypto_software.py`: AES-256-GCM cifra y descifra con datos asociados; con hypothesis, alterar cualquier byte del nonce, del texto cifrado o de los datos asociados impide descifrar; Argon2id es determinista para la misma frase, sal y límites; Shamir 2 de 3 sobre 32 bytes recupera con cualquier par de partes y no con una sola
- [X] T005 [P] Pruebas del almacén SQLite en `hecate/tests/integration/test_sqlite_store.py`: guardar y leer la configuración de sellado y el contador de intentos sobreviven a cerrar y reabrir; un fallo dentro de la transacción no deja cambios; el registro de accesos rechaza `UPDATE` y `DELETE`
- [X] T006 [P] Pruebas del canal local en `hecate/tests/integration/test_control_channel.py`: el socket se crea con permisos `0600` en un directorio `0700`; una petición mal formada recibe `validation` dentro del sobre común; `status` sin inicializar devuelve `state: uninitialized`
- [X] T007 [P] Pruebas de la política de intentos en `hecate/tests/unit/domain/test_attempts.py`: sin espera hasta el 2.º fallo; tras el 3.º, 1 minuto; después 2, 4, 8 y 15 minutos fijos; un éxito pone el contador a cero

### Implementación de la fase

- [X] T008 [P] Definir `SealState` (`uninitialized`, `sealed`, `unsealed`) y `UnsealMethod` (`passphrase`, `recovery`) en `hecate/src/hecate/domain/sealing/state.py`
- [X] T009 [P] Definir los errores de dominio `InvalidInput`, `AuthenticationFailed`, `RateLimited(retry_after_s)`, `StateConflict(state)` en `hecate/src/hecate/domain/sealing/errors.py`, cada uno con su código del contrato (`validation`, `auth`, `rate_limited`, `conflict`) y sin secretos en el mensaje
- [X] T010 [P] Definir el registro de accesos en `hecate/src/hecate/domain/sealing/access_log.py`: `AccessLogEntry` con `at`, `operation` (`init`, `unseal`, `seal`, `passphrase_change`, `shares_rekey`), `method` (`passphrase`, `recovery`, `local`), `result` (`success`, `failure`, `rejected_rate_limited`, `rejected_conflict`, `invalid_input`) y `detail` sin secretos
- [X] T011 [P] Implementar `AttemptPolicy` en `hecate/src/hecate/domain/sealing/attempts.py`: `register_failure(now)`, `register_success()` y `retry_after(now)` con la serie de research R8
- [X] T012 Definir los ports en `hecate/src/hecate/application/ports.py`: `CryptoProvider` (`random_bytes(n)`, `derive_key(passphrase, salt, opslimit, memlimit)` (siempre recibe los límites; el port no tiene valores por defecto), `aead_encrypt(key, plaintext, aad) -> (nonce, ciphertext)`, `aead_decrypt(key, nonce, ciphertext, aad)`, `shamir_split(secret32) -> list[(index, fragments32)]`, `shamir_combine(list) -> secret32`), `SealRepository` (configuración y contador, con escritura transaccional), `AccessLog` (`append`) y `Clock` (`now`)
- [X] T013 [P] Implementar `SoftwareCryptoProvider` en `hecate/src/hecate/infrastructure/crypto/software.py`: AES-256-GCM de `cryptography` con nonce de 12 bytes; Argon2id de `nacl.pwhash.argon2id` con los límites que recibe, sin valores por defecto; Shamir de `Crypto.Protocol.SecretSharing.Shamir` repartiendo los 32 bytes en dos bloques de 16 con los mismos índices (research R4); hace pasar T004
- [X] T014 [P] Implementar `SqliteSealRepository` y `SqliteAccessLog` en `hecate/src/hecate/infrastructure/store/sqlite.py`: fichero `hecate.db` en el directorio de datos; tablas `seal_config` (una fila: `config_version`, `initialized_at`, `updated_at`, protección por frase y por partes según data-model.md), `attempts` (una fila: `consecutive_failures`, `next_attempt_at`) y `access_log` (append-only con triggers que abortan `UPDATE`/`DELETE`); todas las escrituras en transacción; hace pasar T005
- [X] T015 [P] Crear los dobles en `hecate/tests/fakes.py`: `FakeClock` controlable, `InMemorySealRepository`, `InMemoryAccessLog`; las pruebas usan el `SoftwareCryptoProvider` real y configuran `SealingService` con los límites mínimos de Argon2id de libsodium
- [X] T016 [P] Implementar la configuración en `hecate/src/hecate/infrastructure/config.py`: `HECATE_DATA_DIR` (obligatorio) y `HECATE_SOCKET_PATH` (por defecto `${HECATE_DATA_DIR}/control.sock`); crea el directorio con permisos `0700` si no existe
- [X] T017 Implementar el portador de la raíz en `hecate/src/hecate/application/sealing/root_holder.py`: guarda la raíz en un `bytearray`, la sobrescribe con ceros al soltarla y nunca la expone en `repr`
- [X] T018 Implementar `SealingService` en `hecate/src/hecate/application/sealing/service.py` con el estado en memoria (raíz, `unsealed_via`, operaciones pendientes) y una configuración que fija los límites de Argon2id (`SENSITIVE` en producción) que se guardan en cada protección por frase; y la operación `status()` que devuelve `state`, `initialized_at`, `unsealed_via`, `recommend_passphrase_change`, `consecutive_failures` y `retry_after_s` (contrato, `status`)
- [X] T019 Implementar el protocolo en `hecate/src/hecate/infrastructure/control/protocol.py`: lectura de una petición JSON por línea (`request_id`, `op`, `args`), respuesta con el sobre común (`status`, `data`, `error` con `code`, `message`, `retryable` y `retry_after_s` solo con `rate_limited`, `meta` con `request_id` y `duration_ms`) y traducción de los errores de dominio a códigos
- [X] T020 Implementar el servidor en `hecate/src/hecate/infrastructure/control/server.py`: `asyncio` sobre socket Unix con permisos `0600`, un cerrojo para atender las operaciones de una en una, rechazo en Linux de conexiones con otro uid (`SO_PEERCRED`) y despacho de `status`; hace pasar T006
- [X] T021 [P] Implementar el cliente en `hecate/src/hecate/infrastructure/control/client.py`: envía una petición por el socket y devuelve el sobre
- [X] T022 Implementar el endurecimiento del proceso en `hecate/src/hecate/infrastructure/process.py`: `RLIMIT_CORE = 0` y, en Linux, `mlockall` si hay permisos (si no, aviso en el log y sigue) (research R9)
- [X] T023 Implementar el esqueleto de la CLI en `hecate/src/hecate/infrastructure/cli/main.py` con `argparse`: `hecate serve` (endurece el proceso, abre el almacén y arranca el servidor siempre sin raíz en memoria) y `hecate status`; logs estructurados JSON a stdout sin secretos

**Checkpoint**: `hecate serve` arranca y `hecate status` responde `uninitialized`

---

## Phase 3: User Story 1 - Inicializar Hécate (Priority: P1) 🎯 MVP

**Goal**: inicializar una sola vez: raíz, frase, tres partes mostradas una vez, confirmación de dos y Hécate sellada

**Independent Test**: sobre una instalación nueva, inicializar y comprobar que queda `sealed`, que las partes se mostraron una vez y que ni la frase, ni las partes, ni la raíz quedan en disco

### Tests for User Story 1 ⚠️

> Escribir primero y comprobar que fallan

- [X] T024 [P] [US1] Pruebas de la parte de recuperación en `hecate/tests/unit/domain/test_share.py`: ida y vuelta del texto `HEC1-` (Base32 de Crockford en grupos de 4); con hypothesis, cambiar cualquier carácter se detecta por la suma de comprobación; se aceptan minúsculas y las confusiones `O→0` e `I/L→1`
- [X] T025 [P] [US1] Pruebas de la frase en `hecate/tests/unit/domain/test_passphrase.py`: menos de 12 caracteres da `InvalidInput`; `repr` y `str` no muestran el valor
- [X] T026 [P] [US1] Pruebas de la inicialización en `hecate/tests/unit/application/test_initialize.py`: `init_begin` devuelve 3 partes distintas, `init_id`, caducidad de 15 minutos y el aviso de FR-018; `init_confirm` con 2 partes mostradas deja `sealed` y una fila `init/success` en el registro; confirmar con una parte no mostrada da `InvalidInput`; abortar o caducar no persiste nada; inicializar estando inicializada da `StateConflict` sin cambios
- [X] T027 [P] [US1] Pruebas de contrato de `init.begin`, `init.confirm` e `init.abort` en `hecate/tests/contract/test_init_ops.py`, incluidos `validation` (frase corta) y `conflict` (ya inicializada)
- [X] T028 [P] [US1] Prueba de integración en `hecate/tests/integration/test_init_crash.py`: parar el proceso tras `init.begin` y antes de `init.confirm`; al arrancar, `status` es `uninitialized` (SC-008)

### Implementation for User Story 1

- [X] T029 [P] [US1] Implementar `Passphrase` en `hecate/src/hecate/domain/sealing/passphrase.py`: mínimo 12 caracteres (FR-005), sin exponer el valor
- [X] T030 [P] [US1] Implementar `RecoveryShare` en `hecate/src/hecate/domain/sealing/share.py`: campos `version` (1), `index` (1-3), `share_set_id` (4 bytes), `fragments` (32 bytes) y `checksum` (4 primeros bytes de SHA-256 de los anteriores); codificar y decodificar `HEC1-` en Base32 de Crockford en grupos de 4; suma incorrecta → `InvalidInput`
- [X] T031 [P] [US1] Implementar `SealConfig`, `PassphraseProtection` (`kdf`, `kdf_opslimit`, `kdf_memlimit`, `salt` de 16 bytes, `aead`, `nonce` de 12 bytes, `ciphertext`) y `RecoveryProtection` (`share_set_id`, `threshold` 2, `share_count` 3, `aead`, `nonce`, `ciphertext`) en `hecate/src/hecate/domain/sealing/protection.py`, con el constructor de datos asociados `{"purpose": "root", "protection": "passphrase"|"recovery", "config_version": n}`
- [X] T032 [US1] Implementar `init_begin`, `init_confirm` e `init_abort` en `hecate/src/hecate/application/sealing/service.py`: raíz y clave de recuperación de 32 bytes, partes con un `share_set_id` nuevo, todo en memoria hasta confirmar; al confirmar, guarda la configuración (`config_version` 1) en una transacción y registra el intento (FR-002, FR-003, FR-004, FR-018, FR-019: sin CA raíz)
- [X] T033 [US1] Añadir las operaciones `init.begin`, `init.confirm` e `init.abort` al protocolo y al servidor en `hecate/src/hecate/infrastructure/control/server.py`
- [X] T034 [US1] Implementar `hecate init` en `hecate/src/hecate/infrastructure/cli/main.py`: muestra el aviso de FR-018 y pide conformidad, lee la frase dos veces con `getpass`, muestra las 3 partes una vez y pide dos para confirmar; si se interrumpe, envía `init.abort`

**Checkpoint**: US1 funciona y se prueba sola

---

## Phase 4: User Story 2 - Desbloquear con la frase tras un reinicio (Priority: P1)

**Goal**: arrancar siempre sellada y desbloquear con la frase, con espera creciente ante fallos

**Independent Test**: con Hécate inicializada y sellada, desbloquear con la frase correcta; reiniciar y comprobar que vuelve a `sealed`

### Tests for User Story 2 ⚠️

- [X] T035 [P] [US2] Pruebas del desbloqueo con frase en `hecate/tests/unit/application/test_unseal_passphrase.py`: frase correcta → `unsealed` con `unsealed_via: passphrase`; incorrecta → `AuthenticationFailed`, sigue `sealed` y suma un fallo; durante la espera → `RateLimited` sin evaluar la frase y fila `rejected_rate_limited`; en `unsealed` → `StateConflict`
- [X] T036 [P] [US2] Pruebas de contrato en `hecate/tests/contract/test_unseal_ops.py`: `unseal` con frase (`completed`, `auth`, `rate_limited` con `retry_after_s`, `conflict`) y cualquier operación distinta de `status` y `unseal` estando sellada → `conflict` (FR-007)
- [X] T037 [P] [US2] Prueba de integración en `hecate/tests/integration/test_restart.py`: tras desbloquear y reiniciar el proceso, `status` es `sealed`; el contador de fallos y la espera pendiente se mantienen tras reiniciar (FR-006, FR-016)

### Implementation for User Story 2

- [X] T038 [US2] Implementar `unseal_with_passphrase` en `hecate/src/hecate/application/sealing/service.py`: comprueba la espera, deriva la clave con los parámetros guardados, descifra la raíz con sus datos asociados, actualiza el contador en una transacción y registra el intento
- [X] T039 [US2] Añadir al servidor en `hecate/src/hecate/infrastructure/control/server.py` la operación `unseal` (frase) y la regla de estado sellado: solo `status` y `unseal` (FR-007)
- [X] T040 [US2] Implementar `hecate unseal` en `hecate/src/hecate/infrastructure/cli/main.py`: lee la frase con `getpass` e informa de la espera pendiente si la hay

**Checkpoint**: US1 y US2 funcionan; Hécate es usable en el día a día

---

## Phase 5: User Story 3 - Sellar a mano (Priority: P2)

**Goal**: sellar con una sola acción y borrar la raíz de memoria

**Independent Test**: con Hécate desbloqueada, sellarla y comprobar que vuelve a `sealed` y rechaza todo salvo `unseal` y `status`

### Tests for User Story 3 ⚠️

- [X] T041 [P] [US3] Pruebas del sellado en `hecate/tests/unit/application/test_seal.py`: en `unsealed` pasa a `sealed`, el portador de la raíz queda a ceros y se registra `seal/success`; en `sealed` → `StateConflict` con el estado, sin cambiar nada
- [X] T042 [P] [US3] Prueba de contrato de `seal` en `hecate/tests/contract/test_seal_op.py`

### Implementation for User Story 3

- [X] T043 [US3] Implementar `seal` en `hecate/src/hecate/application/sealing/service.py` (FR-010) y la operación `seal` en `hecate/src/hecate/infrastructure/control/server.py`
- [X] T044 [US3] Implementar `hecate seal` en `hecate/src/hecate/infrastructure/cli/main.py`

**Checkpoint**: US1-US3 funcionan

---

## Phase 6: User Story 4 - Recuperar con 2 de 3 partes (Priority: P2)

**Goal**: desbloquear con dos partes y fijar una frase nueva

**Independent Test**: con Hécate sellada, desbloquear con dos partes cualesquiera, fijar una frase nueva y comprobar que la anterior ya no sirve y la nueva sí

### Tests for User Story 4 ⚠️

- [X] T045 [P] [US4] Pruebas del desbloqueo con partes en `hecate/tests/unit/application/test_unseal_shares.py`: con hypothesis, cualquier par de partes desbloquea y recomienda cambiar la frase; una sola parte, o dos con el mismo índice, → `InvalidInput`; una parte con suma incorrecta → `InvalidInput` sin sumar fallo; una parte de otro juego → `AuthenticationFailed` que suma fallo; los fallos con partes y con frase comparten contador
- [X] T046 [P] [US4] Pruebas del cambio de frase en `hecate/tests/unit/application/test_passphrase_change.py`: con prueba por partes o por la frase actual, la frase nueva desbloquea tras reiniciar y la anterior no; sube `config_version`; la protección por partes y la raíz no cambian (FR-013); sin prueba válida → `AuthenticationFailed` que suma fallo
- [X] T047 [P] [US4] Pruebas de contrato de `unseal` con `shares` y de `passphrase.change` en `hecate/tests/contract/test_recovery_ops.py`

### Implementation for User Story 4

- [X] T048 [US4] Implementar `unseal_with_shares` en `hecate/src/hecate/application/sealing/service.py`: valida las partes (formato, suma, índices distintos, `share_set_id` actual), combina la clave de recuperación, descifra la raíz y marca `unsealed_via: recovery` (FR-008, FR-009, FR-011)
- [X] T049 [US4] Implementar `change_passphrase(proof, new_passphrase)` en `hecate/src/hecate/application/sealing/service.py`: verifica la prueba (frase actual o dos partes) con el contador de intentos, crea una protección por frase nueva (sal y nonce nuevos), la guarda subiendo `config_version` en una transacción y registra `passphrase_change` (FR-012, FR-013)
- [X] T050 [US4] Añadir al servidor `unseal` con `shares` y `passphrase.change` en `hecate/src/hecate/infrastructure/control/server.py`
- [X] T051 [US4] Implementar `hecate unseal --shares` y `hecate passphrase change [--with-shares]` en `hecate/src/hecate/infrastructure/cli/main.py`, leyendo frases y partes con `getpass`

**Checkpoint**: US1-US4 funcionan; olvidar la frase ya no es un desastre

---

## Phase 7: User Story 5 - Repartir de nuevo las partes (Priority: P3)

**Goal**: generar un juego nuevo de partes demostrándolo de nuevo, sin tocar la raíz

**Independent Test**: repartir de nuevo y comprobar que las partes antiguas ya no recuperan y las nuevas sí

### Tests for User Story 5 ⚠️

- [X] T052 [P] [US5] Pruebas del reparto en `hecate/tests/unit/application/test_shares_rekey.py`: `rekey_begin` exige prueba válida (si no, `AuthenticationFailed` que suma fallo); `rekey_confirm` con dos partes nuevas sustituye la protección por partes con un `share_set_id` nuevo y sube `config_version`; las partes antiguas dan `AuthenticationFailed` como de otro juego; abortar o caducar deja todo como estaba; la protección por frase y la raíz no cambian
- [X] T053 [P] [US5] Pruebas de contrato de `shares.rekey.begin`, `shares.rekey.confirm` y `shares.rekey.abort` en `hecate/tests/contract/test_rekey_ops.py`

### Implementation for User Story 5

- [X] T054 [US5] Implementar `rekey_begin`, `rekey_confirm` y `rekey_abort` en `hecate/src/hecate/application/sealing/service.py`: clave de recuperación nueva en memoria hasta confirmar; al confirmar, sustituye la protección por partes en una transacción y registra `shares_rekey` (FR-012, FR-013)
- [X] T055 [US5] Añadir las operaciones `shares.rekey.*` al servidor en `hecate/src/hecate/infrastructure/control/server.py`
- [X] T056 [US5] Implementar `hecate shares rekey` en `hecate/src/hecate/infrastructure/cli/main.py`: pide la prueba, muestra las partes nuevas una vez y pide dos para confirmar

**Checkpoint**: todas las historias funcionan

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T057 [P] Prueba de no fuga en `hecate/tests/integration/test_no_secret_leak.py`: tras recorrer init, unseal, cambio de frase y reparto, ni la frase ni ninguna parte aparecen en `hecate.db`, en el registro de accesos ni en los logs capturados (SC-006)
- [X] T058 [P] Prueba de concurrencia en `hecate/tests/integration/test_concurrent_unseal.py`: dos `unseal` simultáneos; uno desbloquea y el otro recibe `conflict`
- [X] T059 [P] Prueba en `hecate/tests/integration/test_crash_during_change.py`: forzar un fallo dentro de la transacción de `passphrase.change` y de `shares.rekey.confirm`; tras reiniciar, la frase y las partes anteriores siguen desbloqueando y `config_version` no cambia (SC-008)
- [X] T060 Recorrer [quickstart.md](quickstart.md) completo, corregir lo que no coincida y medir en el servidor de casa la inicialización (menos de 10 minutos, SC-001) y el desbloqueo con frase (menos de 1 minuto, SC-002)
- [X] T061 [P] Consolidar en `docs/hecate.md`: la inicialización sin CA raíz, el canal local de administración, PyCryptodome para Shamir y SQLite (ADR 0003)
- [X] T062 [P] Añadir el código `conflict` a la lista común de errores en `docs/hermes.md` (sobre común) y en la entrada **Envelope** de `GLOSSARY.md` (research R7)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (fase 1)**: sin dependencias.
- **Foundational (fase 2)**: depende de Setup; bloquea todas las historias.
- **US1 (fase 3)**: depende de Foundational.
- **US2 (fase 4)**: depende de US1 (necesita una Hécate inicializada para desbloquear).
- **US3 (fase 5)**: depende de US2 (sellar requiere poder desbloquear).
- **US4 (fase 6)**: depende de US1 (partes) y de US2 (contador de intentos).
- **US5 (fase 7)**: depende de US4 (prueba con partes y cambio de protección).
- **Polish (fase 8)**: depende de todas las historias.

### Within Each User Story

- Las pruebas se escriben y fallan antes de implementar.
- Dominio → servicio → servidor → CLI.

### Parallel Opportunities

- Fase 2: las pruebas T004-T007 entre sí; el dominio T008-T011 entre sí; T013, T014, T015 y T016 entre sí.
- Dentro de cada historia, todas las pruebas marcadas [P] entre sí, y los ficheros de dominio [P] entre sí.
- US3 y US4 se pueden hacer en cualquier orden una vez terminada US2.

---

## Parallel Example: User Story 1

```text
T024 test_share.py · T025 test_passphrase.py · T026 test_initialize.py · T027 test_init_ops.py · T028 test_init_crash.py
T029 passphrase.py · T030 share.py · T031 protection.py
```

---

## Implementation Strategy

### Primer incremento

1. Fases 1 y 2.
2. US1 (inicializar) y US2 (desbloquear): con las dos, Hécate ya se usa en el día a día.
3. Validar con los pasos 1-6 de quickstart.md.

### Siguientes incrementos

- US3 (sellar) → US4 (recuperar) → US5 (repartir), validando cada una con su prueba independiente.
- Se trabaja una historia cada vez, en la rama `feature/001-hecate-bootstrap`; la PR se abre al terminar todas.

---

## Phase 9: Convergence

- [X] T063 Registrar en el registro de accesos (`result: invalid_input`) las operaciones reconocidas cuyos argumentos están mal formados: hoy `_text`/`_texts` y la comprobación de `proof` en `hecate/src/hecate/infrastructure/control/server.py` fallan antes de llegar al servicio y no dejan fila (comprobado: `unseal {}`, `unseal {"passphrase": 5}`, `passphrase.change` con `proof` mal formado y `shares.rekey.confirm` con `rekey_id` numérico añaden 0 filas); añadir una prueba de contrato en `hecate/tests/contract/test_unseal_ops.py` per contracts/control-protocol.md ("toda operación reconocida, salvo `status`, deja una fila") (partial)
- [X] T064 [P] Añadir a `hecate/tests/integration/test_restart.py` un caso combinado que reinicie el proceso y alterne frase y partes en la misma serie de fallos, y compruebe que no se superan 10 intentos por hora tras los primeros fallos per SC-007 (partial)
- [X] T065 [P] Probar la comprobación de uid del par en `hecate/src/hecate/infrastructure/control/server.py` (`_peer_is_same_user`, con `SO_PEERCRED` simulado, o una prueba solo para Linux) y `harden_process` en `hecate/src/hecate/infrastructure/process.py` (`RLIMIT_CORE` a 0; `mlockall` simulado) per T020 y T022 (partial)
- [X] T066 [P] Recoger `HECATE_KDF_PROFILE` (`sensitive` por defecto, `moderate`) en el Technical Context de `plan.md` y en `hecate/src/hecate/infrastructure/config.py` junto a `HECATE_DATA_DIR` y `HECATE_SOCKET_PATH`, o retirarla: hoy solo consta en `research.md` (R2) y `quickstart.md`, no en el plan ni en las tareas per plan: Technical Context (unrequested)

---

## Phase 10: Convergence

- [X] T067 [P] Añadir en `hecate/tests/contract/test_status_op.py` las pruebas de contrato que faltan: `status` en cada estado con todos sus campos (`state`, `initialized_at`, `unsealed_via`, `recommend_passphrase_change`, `consecutive_failures`, `retry_after_s`) y el código `internal` (un fallo inesperado del servicio responde `failed`, `retryable: true` y un mensaje genérico que no contiene el texto de la excepción), porque el contrato exige al menos una prueba por cada operación y cada código y hoy `status` solo se prueba de pasada y `internal` no tiene ninguna per contracts/control-protocol.md, "Pruebas de contrato" (partial)
