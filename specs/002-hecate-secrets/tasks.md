---

description: "Tareas de implementación de los secretos de Hécate"
---

# Tasks: Secretos de Hécate

**Input**: Design documents from `/specs/002-hecate-secrets/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/control-protocol.md](contracts/control-protocol.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y
deben fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario para implementar y probar cada una por separado.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US5)
- Rutas relativas a la raíz del repositorio; el paquete es `hecate/` (miembro `cerbero-hecate`)

## Convenciones para todas las tareas

- Código, identificadores y mensajes en inglés; Python 3.15 con tipos; mismo estilo que el código de la feature 001.
- Ningún valor de secreto, clave en claro ni huella en logs, excepciones, `repr`, eventos ni respuestas al admin
  (FR-005, FR-019). Las huellas solo aparecen en las respuestas de `secrets.create`, `secrets.add_version`, `secrets.list` y `secrets.get`.
- Tiempos con el port `Clock`; aleatoriedad y criptografía con el port `CryptoProvider`.
- Pruebas unitarias con los dobles de `hecate/tests/fakes.py`; integración con los adapters reales y los límites
  mínimos de Argon2id.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Crear los paquetes vacíos (`__init__.py`) `hecate/src/hecate/domain/secrets/`, `hecate/src/hecate/domain/keys/` y `hecate/src/hecate/application/secrets/`, según la estructura de plan.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: dominio de secretos, cifrado por capas, almacenamiento, registro de accesos ampliado y colaboración con
el sellado

**⚠️ CRITICAL**: ninguna historia puede empezar hasta terminar esta fase

### Pruebas de la fase (primero)

- [X] T002 [P] Pruebas de la ruta en `hecate/tests/unit/domain/test_secret_path.py`: válidas e inválidas según data-model.md (identidad `[a-z][a-z0-9-]{0,62}`; segmentos `[a-z0-9_-]{1,64}`; sin segmentos vacíos; prefijo `secret://`); referencias con `@<n>` (`n ≥ 1`); el dueño es el primer segmento
- [X] T003 [P] Pruebas de los tipos de valor en `hecate/tests/unit/domain/test_value_type.py`: `string` acepta texto UTF-8; `json` rechaza JSON no válido; `pem` exige uno o más bloques `-----BEGIN X-----` / `-----END X-----` con base64 válido; cualquier tipo rechaza más de 64 KiB
- [X] T004 [P] Pruebas del aggregate `Secret` en `hecate/tests/unit/domain/test_secret_aggregate.py`: reconstrucción desde eventos; transiciones permitidas (`active` → `deprecated` | `revoked`; `deprecated` → `active` | `revoked`; `revoked` → `deprecated` | `destroyed`) y rechazo de las demás con `StateConflict`; una sola `active`; estado efectivo `revoked` cuando una `deprecated` supera la gracia (con hypothesis sobre instantes alrededor del límite); `VersionRestored` reinicia la gracia; `stream_version` sube con cada evento
- [X] T005 [P] Pruebas del almacén en `hecate/tests/integration/test_secret_store.py`: los eventos se añaden con versión esperada y una versión desactualizada da conflicto sin escribir nada; eventos y material sobreviven a cerrar y reabrir; borrar el material de una versión deja sus eventos; un fallo dentro de `transaction()` no deja ni eventos ni material; la tabla `events` rechaza `UPDATE` y `DELETE`
- [X] T006 [P] Pruebas del cifrado por capas en `hecate/tests/integration/test_envelope.py`: crear y abrir la clave de ámbito con la raíz; cifrar y descifrar un valor por la clave de versión; con hypothesis, alterar cualquier byte o mover el dato cifrado a otra ruta, versión o identidad impide descifrarlo (datos asociados de research R3); la huella es determinista para la misma identidad y valor, distinta entre identidades y no contiene el valor
- [X] T007 [P] Pruebas del registro de accesos ampliado en `hecate/tests/integration/test_access_log_migration.py`: una base de datos con el esquema de la feature 001 gana las columnas `subject`, `path` y `version` al abrirla, sin perder filas; las filas nuevas guardan esas columnas; las antiguas las leen vacías
- [X] T008 [P] Pruebas de la colaboración con el sellado en `hecate/tests/unit/application/test_sealing_public_ops.py`: `root()` devuelve la raíz desbloqueada y da `StateConflict` sellada o sin inicializar; `verify_proof(proof)` acepta la frase actual o dos partes, rechaza con `AuthenticationFailed` sumando en el contador común y respeta la espera (`RateLimited`)

### Implementación de la fase

- [X] T009 [P] Añadir el error `NotFound` (código `not_found`, no reintentable) en `hecate/src/hecate/domain/sealing/errors.py` y su traducción al sobre en `hecate/src/hecate/infrastructure/control/protocol.py`
- [X] T010 [P] Ampliar `hecate/src/hecate/domain/sealing/access_log.py`: operaciones `secret_create`, `secret_add_version`, `secret_list`, `secret_get`, `secret_revoke`, `secret_restore`, `secret_promote`, `secret_destroy`, `secret_resolve`; campos opcionales `subject`, `path` y `version` en `AccessLogEntry`
- [X] T011 [P] Implementar `SecretPath` y `SecretReference` en `hecate/src/hecate/domain/secrets/path.py` (reglas de T002); hace pasar T002
- [X] T012 [P] Implementar `ValueType` y su validación en `hecate/src/hecate/domain/secrets/value_type.py` (reglas de T003, máximo 64 KiB); hace pasar T003
- [X] T013 Implementar el aggregate `Secret` en `hecate/src/hecate/domain/secrets/secret.py`: eventos `SecretCreated`, `VersionAdded`, `VersionDeprecated`, `VersionRevoked`, `VersionRestored`, `VersionPromoted`, `VersionDestroyed` (motivo `admin` o `max_versions`), sin valores ni claves; métodos que deciden los eventos de cada cambio y aplican las reglas de data-model.md; estado efectivo con la gracia que recibe; hace pasar T004
- [X] T014 [P] Implementar el aggregate `Key` en `hecate/src/hecate/domain/keys/key.py` con el evento `ScopeKeyCreated` (versión 1)
- [X] T015 Ampliar `hecate/src/hecate/application/ports.py`: `CryptoProvider` gana `hmac_sha256(key, data)` y `hkdf_sha256(key, info, length)`; nuevos ports `EventStore` (`load(stream)`, `append(stream, events, expected_version)`, `streams(prefix)`) y `MaterialStore` (guardar, leer y borrar el material de una versión y de una clave de ámbito), ambos con `transaction()` compartida
- [X] T016 [P] Implementar `hmac_sha256` y `hkdf_sha256` en `hecate/src/hecate/infrastructure/crypto/software.py` con `cryptography`
- [X] T017 Implementar en `hecate/src/hecate/infrastructure/store/sqlite.py` un adapter que cumple `EventStore` y `MaterialStore` con una sola conexión y una sola `transaction()`: tabla `events` (stream, versión, tipo, datos JSON, fecha; append-only con triggers), `secret_material` y `key_material` según data-model.md; y la migración del registro de accesos (añadir `subject`, `path`, `version` si faltan); hace pasar T005 y T007
- [X] T018 Implementar el cifrado por capas en `hecate/src/hecate/application/secrets/envelope.py`: crear y abrir la clave de ámbito con la raíz, envolver la clave de versión, cifrar y descifrar el valor y calcular la huella (HMAC-SHA256 con clave HKDF de la clave de ámbito, 8 bytes, `ab12-cd34-ef56-7890`), con los datos asociados de research R3; hace pasar T006
- [X] T019 Exponer en `hecate/src/hecate/application/sealing/service.py` las operaciones públicas `root()` y `verify_proof(proof)` (research R6), reutilizando la verificación y el contador existentes; hace pasar T008
- [X] T020 [P] Añadir a `hecate/tests/fakes.py` un almacén en memoria que cumple `EventStore` y `MaterialStore` con `transaction()` que deshace todo al fallar
- [X] T021 Crear `SecretsService` en `hecate/src/hecate/application/secrets/service.py` con sus dependencias (sellado, almacén, crypto, registro, reloj) y ajustes (gracia 24 h, máximo 10 versiones, caché 300 s y 3600 s), la regla de que toda operación exige Hécate desbloqueada (`StateConflict`, FR-015) y el registro de cada operación con `subject`, `path` y `version` (FR-016)

**Checkpoint**: las 160 pruebas de la feature 001 siguen pasando y las de esta fase pasan

---

## Phase 3: User Story 1 - Guardar un secreto (Priority: P1) 🎯 MVP

**Goal**: el admin guarda secretos (versión 1) y los consulta sin ver nunca un valor

**Independent Test**: guardar un secreto y comprobar que existe con su versión 1, metadatos y huella, que se lista y
se consulta sin valor, y que el valor no aparece en claro en ningún sitio

### Tests for User Story 1 ⚠️

- [X] T022 [P] [US1] Pruebas de crear en `hecate/tests/unit/application/test_secret_create.py`: `create` da la versión 1 `active` con huella; la primera vez para una identidad crea su clave (`ScopeKeyCreated` una sola vez); valor no válido para su tipo, ruta mal formada o más de 64 KiB → `InvalidInput` sin cambios; `create` sobre una ruta existente → `StateConflict`; sellada → `StateConflict`; el registro guarda `secret_create` con `subject: admin`, ruta y versión, sin valor
- [X] T023 [P] [US1] Pruebas de consultar en `hecate/tests/unit/application/test_secret_list_get.py`: `list` devuelve rutas y metadatos, con filtro por identidad; `get` devuelve metadatos, `stream_version` y por versión estado, fecha y huella; ninguna respuesta contiene el valor; `rotate_every` y `expires_at` guardados al crear aparecen en `get` y `list`; `list` da `version_count`; ruta inexistente → `NotFound`; cada consulta queda registrada
- [X] T024 [P] [US1] Pruebas de contrato de `secrets.create`, `secrets.list` y `secrets.get` en `hecate/tests/contract/test_secret_ops.py`, incluidos `validation`, `conflict` (sellada y ruta existente) y `not_found`

### Implementation for User Story 1

- [X] T025 [US1] Implementar `create`, `list` y `get` en `hecate/src/hecate/application/secrets/service.py` (FR-001, FR-002, FR-003, FR-005, FR-018, FR-019, FR-020, FR-021), guardando eventos y material en una sola transacción
- [X] T026 [US1] Añadir las operaciones `secrets.create`, `secrets.list` y `secrets.get` en `hecate/src/hecate/infrastructure/control/server.py` (con su registro de argumentos mal formados) y construir `SecretsService` en `hecate serve` en `hecate/src/hecate/infrastructure/cli/main.py`
- [X] T027 [US1] Implementar `hecate secrets create <ruta> --type <tipo> [--from-file <fichero>]`, `hecate secrets list [--identity <id>]` y `hecate secrets get <ruta>` en `hecate/src/hecate/infrastructure/cli/main.py`; el valor se lee con `getpass` o del fichero, nunca de la línea de órdenes

**Checkpoint**: US1 funciona y se prueba sola

---

## Phase 4: User Story 2 - Una identidad resuelve sus secretos (Priority: P1)

**Goal**: resolver uno o varios secretos propios; denegar los ajenos sin revelar si existen

**Independent Test**: con la identidad como dato de entrada, cada identidad resuelve los suyos y se le deniegan los de
otra, quedando registrado

### Tests for User Story 2 ⚠️

- [X] T028 [P] [US2] Pruebas de resolver en `hecate/tests/unit/application/test_secret_resolve.py`: la versión `active` o una concreta (`@n`) resoluble; varias referencias en una petición; todo o nada; un secreto de otra identidad y uno inexistente dan el mismo `NotFound` con el mismo mensaje (FR-013, SC-007); sin versión `active` → el mismo `NotFound` (FR-014); tiempos de caché 300 y 3600; cada resolución y cada rechazo quedan registrados con `subject` = identidad, sin valor; sellada → `StateConflict`
- [X] T029 [P] [US2] Prueba de integración en `hecate/tests/integration/test_secret_resolve_sqlite.py`: con SQLite y criptografía reales, guardar, reiniciar el proceso, desbloquear y resolver; resolver 10 referencias tarda menos de 1 s (SC-001)

### Implementation for User Story 2

- [X] T030 [US2] Implementar `resolve(identity, references)` en `hecate/src/hecate/application/secrets/service.py` (FR-011 a FR-014), sin operación en el canal local (research R7)

**Checkpoint**: US1 y US2 funcionan

---

## Phase 5: User Story 3 - Rotar un secreto con una versión nueva (Priority: P2)

**Goal**: añadir versiones, con gracia para la anterior y límite de versiones

**Independent Test**: añadir una versión y comprobar que la nueva es la activa, que la anterior se resuelve
explícitamente durante la gracia y que deja de hacerlo al terminar

### Tests for User Story 3 ⚠️

- [X] T031 [P] [US3] Pruebas de `add_version` en `hecate/tests/unit/application/test_secret_rotate.py`: con `expected_version` correcta, la nueva queda `active` y la anterior `deprecated`; `@anterior` se resuelve durante la gracia y no después (SC-005); `expected_version` desactualizada → `StateConflict` con la versión actual, y ausente → `InvalidInput`, ambos sin cambios; ruta inexistente → `NotFound`; superar el máximo destruye la más antigua no destruida en el mismo cambio y borra su material; el registro guarda `secret_add_version` con `subject: admin`, ruta y versión
- [X] T032 [P] [US3] Pruebas de contrato de `secrets.add_version` en `hecate/tests/contract/test_secret_ops.py` (`validation` sin `expected_version`, `conflict` con `data.current_version`, `not_found`)

### Implementation for User Story 3

- [X] T033 [US3] Implementar `add_version` en `hecate/src/hecate/application/secrets/service.py` (FR-006, FR-007, FR-010, FR-017) y la operación `secrets.add_version` en `hecate/src/hecate/infrastructure/control/server.py`
- [X] T034 [US3] Implementar `hecate secrets put <ruta> [--from-file <fichero>]`, que añade una versión: pide primero `secrets.get` y envía su `stream_version` como `expected_version`, en `hecate/src/hecate/infrastructure/cli/main.py`

**Checkpoint**: US1-US3 funcionan

---

## Phase 6: User Story 4 - Revocar, restaurar o volver a la anterior (Priority: P2)

**Goal**: revocar al instante, restaurar una revocada y promover una anterior

**Independent Test**: revocar la activa y ver que deja de resolverse al momento; restaurar y promover, comprobando qué
se resuelve en cada paso

### Tests for User Story 4 ⚠️

- [X] T035 [P] [US4] Pruebas en `hecate/tests/unit/application/test_secret_lifecycle.py`: revocar `active` o `deprecated` deja de resolverse al instante; restaurar una `revoked` la deja `deprecated` con gracia nueva; promover una `deprecated` la deja `active` y la activa pasa a `deprecated`; sin versión `active` la resolución sin versión se rechaza; transiciones no permitidas → `StateConflict`; versión inexistente → `NotFound`; ninguna de estas operaciones pide prueba
- [X] T036 [P] [US4] Pruebas de contrato de `secrets.revoke`, `secrets.restore` y `secrets.promote` en `hecate/tests/contract/test_secret_lifecycle_ops.py`

### Implementation for User Story 4

- [X] T037 [US4] Implementar `revoke`, `restore` y `promote` en `hecate/src/hecate/application/secrets/service.py` (FR-008)
- [X] T038 [US4] Añadir `secrets.revoke`, `secrets.restore` y `secrets.promote` en `hecate/src/hecate/infrastructure/control/server.py` y `hecate secrets revoke|restore|promote <ruta> --version <n>` en `hecate/src/hecate/infrastructure/cli/main.py`

**Checkpoint**: US1-US4 funcionan

---

## Phase 7: User Story 5 - Destruir una versión (Priority: P3)

**Goal**: borrado criptográfico de una versión revocada, demostrándolo de nuevo

**Independent Test**: destruir una versión revocada y comprobar que no se puede resolver ni restaurar y que su
historial sigue

### Tests for User Story 5 ⚠️

- [X] T039 [P] [US5] Pruebas en `hecate/tests/unit/application/test_secret_destroy.py`: con la frase actual o dos partes, una `revoked` pasa a `destroyed`, se borra su material y quedan sus eventos; sin prueba válida → `AuthenticationFailed`, sigue `revoked` y suma en el contador común; durante una espera → `RateLimited`; una versión no `revoked` → `StateConflict`; una `destroyed` no se resuelve ni se restaura
- [X] T040 [P] [US5] Pruebas de contrato de `secrets.destroy` en `hecate/tests/contract/test_secret_lifecycle_ops.py` (`auth`, `rate_limited`, `conflict`)

### Implementation for User Story 5

- [X] T041 [US5] Implementar `destroy` en `hecate/src/hecate/application/secrets/service.py` usando `verify_proof` del sellado (FR-009)
- [X] T042 [US5] Añadir `secrets.destroy` en `hecate/src/hecate/infrastructure/control/server.py` y `hecate secrets destroy <ruta> --version <n> [--with-shares]` en `hecate/src/hecate/infrastructure/cli/main.py`, pidiendo la prueba con `getpass`

**Checkpoint**: todas las historias funcionan

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T043 [P] Pruebas y cambio de `status` en `hecate/tests/contract/test_status_op.py`: `SecretsService.expiry_counts(now)` en `hecate/src/hecate/application/secrets/service.py` calcula `secrets_expired` y `secrets_expiring_soon` (próximos 7 días) a partir de los metadatos, sin rutas y también con Hécate sellada (FR-022), y el handler de `status` en `hecate/src/hecate/infrastructure/control/server.py` los añade a la respuesta del sellado, sin que `sealing/service.py` dependa de los secretos; `hecate status` los muestra
- [X] T044 [P] Prueba de no fuga en `hecate/tests/integration/test_secret_no_leak.py`: tras guardar, rotar, revocar, destruir y resolver, ningún valor aparece en `hecate.db`, en los eventos, en el registro de accesos, en los logs ni en las respuestas al admin (SC-002, SC-008)
- [X] T045 [P] Prueba de borrado criptográfico en `hecate/tests/integration/test_secret_crypto_shred.py`: tras destruir una versión, ni con la raíz ni restaurando una copia de `hecate.db` hecha después se recupera su valor (SC-004)
- [X] T046 [P] Prueba de cortes en `hecate/tests/integration/test_secret_crash.py`: un fallo dentro de la transacción de `put` (creación y versión nueva) y de `destroy` deja el secreto en su estado anterior (SC-006)
- [X] T047 [P] Prueba de concurrencia en `hecate/tests/integration/test_secret_concurrency.py`: dos versiones nuevas con la misma `expected_version` por el socket; una se aplica y la otra recibe `conflict`
- [X] T048 Recorrer [quickstart.md](quickstart.md) con la CLI real y corregir lo que no coincida
- [X] T049 [P] Consolidar en `docs/hecate.md`: el event store y las tablas de material cifrado, la huella con clave, que el admin no lee valores, las operaciones `secrets.*` del canal local y que la resolución no tiene canal hasta la feature de identidades
- [X] T050 [P] Añadir **Fingerprint** (huella con clave de una versión de secreto) a `GLOSSARY.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (fase 1)**: sin dependencias.
- **Foundational (fase 2)**: depende de Setup; bloquea todas las historias.
- **US1 (fase 3)**: depende de Foundational.
- **US2 (fase 4)**: depende de US1 (necesita secretos guardados).
- **US3 (fase 5)**: depende de US1; sus pruebas de gracia usan la resolución de US2.
- **US4 (fase 6)**: depende de US3 (necesita varias versiones).
- **US5 (fase 7)**: depende de US4 (solo se destruyen versiones revocadas).
- **Polish (fase 8)**: depende de todas las historias.

### Within Each User Story

- Las pruebas se escriben y fallan antes de implementar.
- Dominio → servicio → servidor → CLI.

### Parallel Opportunities

- Fase 2: las pruebas T002-T008 entre sí; T009-T012 y T014 entre sí; T016 y T020 entre sí.
- Dentro de cada historia, las pruebas marcadas [P] entre sí.
- Fase 8: T043-T047 y T049-T050 entre sí.

---

## Parallel Example: User Story 1

```text
T022 test_secret_create.py · T023 test_secret_list_get.py · T024 test_secret_ops.py
```

---

## Implementation Strategy

### Primer incremento

1. Fases 1 y 2.
2. US1 (guardar y consultar) y US2 (resolver): con las dos, los secretos se guardan y la lógica de resolución queda
   lista para conectar el canal en la feature de identidades.
3. Validar con los pasos 1-3 del quickstart.

### Siguientes incrementos

- US3 (rotar) → US4 (revocar, restaurar, promover) → US5 (destruir), validando cada una con su prueba independiente.
- Una historia cada vez, en la rama `feature/002-hecate-secrets`; la PR se abre al terminar todas.

---

## Phase 9: Convergence

- [X] T051 Exigir un máximo de versiones de 2 o más (decidido tras el converge), para que tras una rotación la versión anterior siga resolviéndose durante su gracia: hoy, con `max_versions=1`, `add_version` destruye en el mismo cambio la versión que acaba de pasar a `deprecated` (comprobado: tras rotar, `@1` da `NotFound` y queda `destroyed`). En `hecate/src/hecate/domain/secrets/secret.py`, `Secret.create` valida `max_versions` de 2 a 100 (constante `MIN_VERSIONS = 2` junto a `MAX_VERSIONS_LIMIT`) y el mensaje de `InvalidInput` dice "from 2 to 100"; la ayuda de `--max-versions` en `hecate/src/hecate/infrastructure/cli/main.py` indica el rango. Pruebas primero: en `hecate/tests/unit/domain/test_secret_aggregate.py`, `max_versions=1` se rechaza con `InvalidInput` y, con `max_versions=2`, tras rotar la versión 1 sigue `deprecated` dentro de la gracia; en `hecate/tests/contract/test_secret_ops.py`, `secrets.create` con `max_versions: 1` responde `validation`. Adaptar las tres pruebas que hoy usan `max_versions=1` para que sigan probando la destrucción automática con `max_versions=2` y una tercera versión: `test_a_maximum_of_one_keeps_only_the_new_version` (`hecate/tests/unit/domain/test_secret_aggregate.py`), `test_a_failure_while_destroying_the_oldest_leaves_everything_as_it_was` (`hecate/tests/unit/application/test_secret_rotate.py`) y la de borrado criptográfico de `hecate/tests/integration/test_secret_crypto_shred.py`. En `specs/002-hecate-secrets/data-model.md`, `max_versions` queda "10 por defecto, de 2 a 100" con su razón, y la regla del máximo aclara que solo la versión anterior inmediata tiene garantizada su gracia (SC-005): con N rotaciones dentro de la gracia y máximo N, la más antigua se destruye aunque siga en gracia (FR-010) per SC-005 y FR-010 (partial)
- [X] T052 [P] Recoger en `specs/002-hecate-secrets/data-model.md` y `plan.md` lo implementado que no figura en el diseño, o retirarlo: el módulo `hecate/src/hecate/domain/events.py` (codificación común de eventos, fuera de la estructura del plan); los valores nuevos del registro de accesos `Result.NOT_FOUND` y `Method.IDENTITY`; los límites añadidos (como mucho 100 referencias por resolución, el tope de 100 en `max_versions` (el mínimo de 2 lo recoge T051), valor no vacío, `rotate_every` de 1 a 64 caracteres imprimibles); el campo `rotate_every` en la respuesta de `secrets.list`; `PRAGMA secure_delete` para el borrado criptográfico; y el HMAC de la librería estándar en lugar del de `cryptography` (research R12) per plan: Project Structure y data-model (unrequested)

---

## Phase 10: Convergence

- [X] T053 Completar la tabla de errores de `specs/002-hecate-secrets/contracts/control-protocol.md` con los dos rechazos de `secrets.create` que ya implementa `hecate/src/hecate/application/secrets/service.py` y no figuran en el contrato: `rotate_every` que no sea un texto imprimible de 1 a 64 caracteres y `expires_at` que no sea una fecha y hora ISO 8601, ambos `validation`; y cubrirlos en `hecate/tests/contract/test_secret_ops.py` si aún no hay prueba de contrato para ellos per plan: contracts/control-protocol.md, Errores (partial)
