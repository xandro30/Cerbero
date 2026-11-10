---

description: "Tareas de la feature 003: identidades de Hécate"
---

# Tasks: Identidades de Hécate

**Input**: Design documents from `/specs/003-hecate-identities/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[contracts/](contracts/README.md), [quickstart.md](quickstart.md)

**Tests**: obligatorios. La constitución (principio IV) exige TDD: en cada fase, las pruebas se escriben primero y deben
fallar antes de implementar.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US7 de la spec), en el orden de sus prioridades.
Las historias dependen unas de otras porque cada una necesita lo que crea la anterior (una CA para dar de alta, un alta
para renovar...); el orden está en «Dependencies & Execution Order».

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US7)
- Rutas relativas a la raíz del repositorio; las órdenes de cargo se ejecutan dentro de `hecate/`

## Convenciones para todas las tareas

- **Pruebas primero:** cada tarea de pruebas se ejecuta y falla antes de la implementación que la hace pasar.
- **Dónde van las pruebas** (como en el cambio 005):
  - las de dominio, en un módulo `#[cfg(test)]` del propio fichero;
  - las de aplicación, en `hecate/crates/application/tests/`, con la infraestructura como dependencia de desarrollo;
  - las de integración, API y contrato, en `hecate/crates/infrastructure/tests/`; las del contrato del canal local, en un
    `contract_*_ops.rs` por grupo de operaciones y en `contract_status_op.rs`, como en el cambio 005;
  - las de la CLI, en `hecate/crates/cli/tests/`, con el terminal guionizado del cambio 005.
- **Ayudantes:** los ayudantes comunes viven en los módulos `testing` de la aplicación (`FakeClock`, dobles en memoria) y
  de la infraestructura (`SqliteStack`, `ServerThread`, `short_dir` y, desde esta feature, el componente de prueba y la
  validación de contratos), tras la feature `test-support`.
- **Contrato:** cada prueba que recibe una respuesta de la API la valida contra
  `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` y `common.v1.json` con el ayudante de T015 (research
  R11).
- **Sin cambios en 001 y 002 (FR-030):** las 423 pruebas del cambio 005 siguen pasando en cada fase. Solo se amplían a
  propósito las que fijan lo que esta feature añade, sin dejar de comprobar lo que ya comprobaban:
  - `hecate/crates/infrastructure/tests/schema.rs`, el esquema exacto de una base nueva (T012);
  - los literales de `AccessLogEntry` de `access_log_migration.rs` y `sqlite_store.rs`, que ganan `identity` y
    `certificate` (T013);
  - `contract_status_op.rs` y la salida exacta de `status` en `hecate/crates/cli/tests/cli_secrets.rs`, que ganan el
    bloque `ca` (T034).

  El único cambio de comportamiento es FR-027.
- **Seguridad:**
  - `unsafe_code = "forbid"`;
  - los secretos de paso, en tipos que se borran al soltarse, sin `Clone` ni `Debug` (research R14);
  - ningún secreto en eventos, logs, errores ni en el registro de accesos;
  - la clave de la CA raíz nunca cruza el canal local (FR-002).
- **Registro de accesos:** cada operación deja las filas que indica [data-model.md](data-model.md#registro-de-accesos),
  con `subject`, `identity` y `certificate`.
- **Atomicidad (FR-026):** cada caso de uso escribe en una sola transacción con la versión esperada de cada stream; las
  pruebas de servicio comprueban que un fallo a mitad no deja nada (SC-007).
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Comprobar el entorno: rama `feature/003-hecate-identities`, `.specify/feature.json` apuntando a `specs/003-hecate-identities`, `rustc 1.99.1`, Docker disponible, cargo-deny 0.20.2 instalado y `cargo test --workspace` en verde antes de empezar (423 pruebas)
- [X] T002 Añadir al `[workspace.dependencies]` de `hecate/Cargo.toml` las crates del ADR 0009 con las versiones de plan.md: axum 0.8.9 (sin features por defecto), hyper 1.12.0 (`http1`, `http2` y `server`), hyper-util 0.1.21 (`server-auto`, `tokio`, `service` y, para las pruebas, `client-legacy`), tower 0.5.3, tokio-rustls 0.26.6 (sin features por defecto, con `aws_lc_rs`), rustls 0.23.46 (sin features por defecto, con `aws_lc_rs`, `std` y `prefer-post-quantum`, sin `tls12`), rcgen 0.14.10 (sin features por defecto, con `aws_lc_rs`, `pem`, `x509-parser` y `zeroize`), x509-parser 0.18.1 (`verify-aws`), jsonwebtoken 11.1.0 (sin features por defecto, con `aws_lc_rs`), hyper-rustls 0.27.10 (sin features por defecto, con `aws-lc-rs`, `http1` y `http2`) y jsonschema 0.61.0 (sin features por defecto, con `resolve-file`); usarlas en `hecate/crates/infrastructure/Cargo.toml` (las dos últimas y `client-legacy`, como dependencias de desarrollo y de `test-support`), y añadir rustls con la feature `tls12` solo como dependencia de desarrollo, para el cliente TLS 1.2 de T016; `cargo build --workspace` compila
- [X] T003 Pasar `cargo deny check`; si aparece un duplicado que ninguna versión publicada quita, justificarlo en `hecate/deny.toml` como las dos excepciones del cambio 005, y añadir MIT-0 a las licencias permitidas, que trae `borrow-or-share` a través de jsonschema (research R20)
- [X] T004 [P] Crear los módulos vacíos de plan.md y registrarlos en cada `lib.rs`: `hecate/crates/domain/src/identity/mod.rs`, `hecate/crates/domain/src/ca/mod.rs`, `hecate/crates/application/src/identity/mod.rs`, `hecate/crates/application/src/ca/mod.rs`, `hecate/crates/infrastructure/src/pki.rs`, `hecate/crates/infrastructure/src/call_token.rs`, `hecate/crates/infrastructure/src/backup.rs`, `hecate/crates/infrastructure/src/tasks.rs`, `hecate/crates/infrastructure/src/api/mod.rs` y `hecate/crates/infrastructure/examples/component.rs` (con un `main` vacío); `cargo build --workspace --examples` compila

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: el contrato publicado, los valores y eventos del dominio, la migración del esquema, los ports, la
configuración y el esqueleto de la API, que usan todas las historias.

**⚠️ CRITICAL**: ninguna historia puede empezar hasta terminar esta fase

### Contrato (primero, principio II)

- [X] T005 [P] Escribir `shared/contracts/src/cerbero_contracts/openapi/common.v1.json` (OpenAPI 3.1 en JSON) con el sobre común de contracts/api.md y research R11: `status` (`completed`, `failed`), `data`, `error` (`code`, `message` y `retryable`, y los opcionales `retry_after_s`, solo con `rate_limited`, y `outcome_unknown`, solo con `timeout`) y `meta` (`request_id`, `duration_ms`); y los nueve códigos cerrados `validation`, `auth`, `not_found`, `conflict`, `rate_limited`, `provider_unavailable`, `timeout`, `unavailable` e `internal`, cada uno con su descripción y sin estado HTTP, que fija cada API
- [X] T006 [P] Escribir `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` con las seis operaciones de contracts/api.md: `POST /v1/identities:enroll`, `POST /v1/certificates:renew`, `GET /v1/revocations`, `GET /v1/keys`, `GET /v1/keys/{kid}` y `GET /v1/capabilities`, con la seguridad (mTLS y `Authorization: Bearer`, salvo el alta), las cabeceras `Idempotency-Key` (de 1 a 128 caracteres ASCII imprimibles, obligatoria en las `POST`) y `X-Request-Id` (opcional, con la misma regla), los cuerpos, los `data` de cada respuesta, los JWK con `cerbero_identity`, `cerbero_status` y `cerbero_not_after`, y las respuestas de error referidas a `common.v1.json`, con el estado HTTP de cada código que usa Hécate (400, 401, 404, 409, 429, 503 y 500)

### Pruebas de la fase (primero)

- [X] T007 [P] Pruebas de los valores en `hecate/crates/domain/src/identity/names.rs`: `TrustDomain` («De 1 a 255 caracteres entre minúsculas, dígitos, `.`, `-` y `_`»), `IdentityName` (`[a-z][a-z0-9-]{0,62}`, la misma regla que el primer segmento de las rutas de secretos, con `hecate` y `admin` reservados), `SpiffeId` (`spiffe://<trust domain>/<identidad>`, lectura y escritura), `Serial` (128 bits, positivo, hexadecimal en minúsculas), `RootFingerprint` (`sha256:` y 64 caracteres hexadecimales) y `TokenId` (16 caracteres hexadecimales en minúsculas); propiedades con proptest de ida y vuelta y de rechazo
- [X] T008 [P] Pruebas del token de alta en `hecate/crates/domain/src/identity/token.rs`: formato `HET1.<identidad>.<id>.<secreto>.<huella>` (id de 16 hexadecimales en minúsculas, secreto de 43 caracteres base64url sin relleno, huella de 64 hexadecimales en minúsculas); un texto mal formado se rechaza; el SHA-256 del secreto y su comparación en tiempo constante; `Debug` no muestra el secreto; propiedad de ida y vuelta
- [X] T009 [P] Pruebas de la clave de firma de llamadas en `hecate/crates/domain/src/identity/call_key.rs`: el `kid` es la huella de RFC 7638 en base64url, comprobada con el ejemplo Ed25519 de RFC 8037 (apéndice A.3: `kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k`); una clave que no es de 32 bytes se rechaza
- [X] T010 [P] Pruebas del lector PEM en `hecate/crates/domain/src/pem.rs`, extraído del tipo de valor `pem` del cambio 005, con dos entradas: varios bloques de cualquier etiqueta, como el tipo `pem`, cuyas pruebas de 002 siguen pasando sin cambios; y un solo bloque con la etiqueta esperada (`CERTIFICATE REQUEST`, `PUBLIC KEY`, `CERTIFICATE`, `X509 CRL` o `HECATE ROOT CA KEY`), que devuelve el DER; en las dos, espacios ASCII de RFC 7468, con contenido y base64 estricto
- [X] T011 [P] Pruebas de los eventos nuevos en `hecate/crates/domain/src/events.rs`: ida y vuelta de `CaCreated`, `IntermediateRenewed`, `IntermediateRetired`, `CaReplaced`, `EnrollmentTokenCreated`, `EnrollmentTokenAnnulled`, `IdentityEnrolled`, `IdentityDisabled`, `IdentityEnabled`, `SuspiciousRenewalDetected`, `CertificateIssued`, `CertificateRenewed` y `CertificateRevoked`, con los campos de data-model.md; ningún evento lleva el secreto de un token ni una clave privada
- [X] T012 Pruebas de la migración en `hecate/crates/infrastructure/tests/store_migration.rs` y `hecate/crates/infrastructure/tests/schema.rs`, ampliando la del cambio 005:
  - una base con el esquema del cambio 005 (`hecate/tests/vectors/schema.sql`) se abre, gana `ca_material`, `hecate_tls`, `enroll_attempts` y las columnas `identity` y `certificate` de `access_log`, que admiten nulos, y conserva sus filas;
  - abrirla otra vez no cambia nada, y dos conexiones que la abren a la vez terminan bien;
  - una base de la feature 001 sigue migrando (`access_log_migration.rs`);
  - `schema.rs` compara una base nueva con `schema.sql` más los objetos de esta feature; `schema.sql` no cambia, porque es la base del cambio 005
- [X] T013 Pruebas del registro de accesos en `hecate/crates/infrastructure/tests/store_access_log.rs`: las filas con `identity` y `certificate` se escriben y se leen, y los disparadores siguen rechazando `UPDATE` y `DELETE`; los literales de `AccessLogEntry` de `access_log_migration.rs` y `sqlite_store.rs` ganan `identity` y `certificate` a `None`
- [X] T014 [P] Pruebas de la configuración en `hecate/crates/infrastructure/tests/config.rs` y `hecate/crates/cli/tests/serve_config.rs` (research R15):
  - `HECATE_API_ADDR`, por defecto `127.0.0.1:8443`;
  - `HECATE_CERT_LIFETIME_HOURS`, por defecto 72, de 1 a 72;
  - `HECATE_ENROLL_TOKEN_MINUTES`, por defecto 15, de 1 a 60;
  - un valor no válido da un error que nombra la variable, y `hecate serve` imprime `error: …` y termina con 2, como todo error de configuración del cambio 005
- [X] T015 Ayudante y pruebas de contrato en `hecate/crates/infrastructure/src/testing.rs` (feature `test-support`) y `hecate/crates/infrastructure/tests/contract_schema.rs`:
  - el ayudante carga los dos OpenAPI de `shared/contracts/src/cerbero_contracts/openapi/` (ruta calculada desde `CARGO_MANIFEST_DIR`), resuelve los `$ref` entre ellos y valida con jsonschema el estado HTTP y el cuerpo de la respuesta de una operación;
  - las pruebas comprueban que los dos ficheros son JSON válido, que todos los `$ref` se resuelven y que un sobre de cada uno de los nueve códigos valida
- [X] T016 Pruebas del esqueleto de la API en `hecate/crates/infrastructure/tests/api_server.rs`, con una ruta de prueba y una CA de usar y tirar generada en la prueba, validadas con el ayudante de T015:
  - solo TLS 1.3: un cliente rustls que solo ofrece TLS 1.2 (con la feature `tls12` de desarrollo) no conecta;
  - el certificado de cliente es opcional, y uno caducado o de otra CA no corta el handshake: llega a la petición;
  - el verificador de la TLS rechaza una firma del handshake que no corresponde al certificado presentado, y cada handshake rechazado deja una línea en los logs estructurados con el motivo;
  - un cuerpo de más de 64 KiB da `validation`;
  - un cuerpo que no es JSON estándar da `validation` sin fila en el registro, con una línea en los logs estructurados que no lleva el cuerpo (FR-029);
  - un `X-Request-Id` fuera de su regla da `validation`;
  - con Hécate sellada, toda ruta responde `unavailable` (503, `retryable`);
  - `meta.request_id` repite `X-Request-Id`;
  - cada código de error que usa Hécate da su estado HTTP y, con `rate_limited`, la cabecera `Retry-After`

### Implementación de la fase

- [X] T017 [P] Implementar los valores en `hecate/crates/domain/src/identity/names.rs`, con la regla de identidad compartida con `hecate/crates/domain/src/secrets/path.rs` (una sola fuente)
- [X] T018 [P] Implementar el token de alta en `hecate/crates/domain/src/identity/token.rs`: secreto en `Zeroizing`, sin `Clone` ni `Debug` que lo muestre; SHA-256 con aws-lc-rs y comparación en tiempo constante
- [X] T019 [P] Implementar `CallKey` y su `kid` en `hecate/crates/domain/src/identity/call_key.rs`
- [X] T020 [P] Extraer el lector PEM a `hecate/crates/domain/src/pem.rs`, con sus dos entradas, y usar la de varios bloques desde `hecate/crates/domain/src/secrets/value_type.rs`
- [X] T021 Implementar los eventos nuevos en `hecate/crates/domain/src/events.rs`
- [X] T022 Implementar la migración en `hecate/crates/infrastructure/src/store/mod.rs`, ampliando la del cambio 005, sin `user_version`:
  - las tablas `ca_material` (`generation`, `intermediate_version` como clave, `root_certificate`, `intermediate_certificate`, `key_nonce`, `key_ciphertext`), `hecate_tls` (`id` 1, `serial`, `private_key`) y `enroll_attempts` (`id` 1, `consecutive_failures`, `wait_until`, `rejected_during_wait`) en `SCHEMA`, con `CREATE TABLE IF NOT EXISTS`;
  - `identity TEXT` y `certificate TEXT` en la sentencia de `access_log` y en `ACCESS_LOG_NEW_COLUMNS`;
  - los campos `identity` y `certificate` y las operaciones nuevas de data-model.md en `hecate/crates/domain/src/sealing/access_log.rs`
- [X] T023 Declarar los ports en `hecate/crates/application/src/ports.rs`, con sus dobles en `hecate/crates/application/src/testing.rs`:
  - `PkiProvider`: generar claves P-256, crear y leer solicitudes, emitir según los perfiles y comprobarlos, leer certificados (SPIFFE ID, serie, validez, emisor y clave pública) y calcular la huella;
  - `CallTokenVerifier`;
  - `IdentityStore`: streams `ca`, `identity/*` y `certificate/*/*`, transacción sobre varios streams, `ca_material` y `hecate_tls`;
  - `EnrollAttemptsRepository`
- [X] T024 Implementar la configuración nueva en `hecate/crates/infrastructure/src/config.rs` y su error de arranque en `hecate/crates/cli/src/lib.rs` (`serve`)
- [X] T025 Implementar el esqueleto de la API en `hecate/crates/infrastructure/src/api/`:
  - `server.rs`:
    - `TcpListener` y `TlsAcceptor` con rustls 0.23 y aws-lc-rs, con `ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])`;
    - un `ClientCertVerifier` propio que pide certificado sin exigirlo, anuncia la CA raíz vigente en `root_hint_subjects` y solo comprueba la firma del handshake con `rustls::crypto::verify_tls13_signature`, sin validar la cadena (research R1);
    - el certificado de la conexión, con su cadena, como extensión de cada petición;
    - `hyper_util::server::conn::auto` con 10 s para las cabeceras y `TowerToHyperService`;
    - la configuración en un `tokio::sync::RwLock<Arc<ServerConfig>>`, para cambiarla sin reiniciar;
    - el listener se puede cerrar y volver a abrir sin reiniciar el proceso;
    - cada handshake rechazado deja una línea en los logs estructurados con el motivo y la dirección de origen, sin fila en el registro;
  - `envelope.rs`: el sobre, los códigos y sus estados HTTP, `Retry-After` y `X-Request-Id` con su regla;
  - `json.rs`: un extractor de JSON estándar de hasta 64 KiB que rechaza sin fila;
  - la guarda de Hécate sellada, que responde `unavailable`

**Checkpoint**: pasan las pruebas de la fase y las 423 del cambio 005, con las ampliaciones a propósito de las
convenciones.

---

## Phase 3: User Story 1 - Crear la CA de Cerbero (Priority: P1) 🎯 MVP

**Goal**: la ceremonia crea la CA raíz en la CLI y la intermedia en Hécate.
- La copia de la clave de la CA raíz se confirma antes de activar la CA y la clave nunca entra en el proceso de Hécate.
- Hécate se emite su certificado y la API escucha, también sellada.
- `status` muestra la CA.

**Independent Test**: `hecate ca create` dentro del proceso deja comprobado todo esto (quickstart §5, pasos 1 a 3):
- la intermedia está firmada por la CA raíz;
- la copia solo se abre con la frase;
- la base no contiene la clave de la CA raíz ni la frase, y la clave de la intermedia solo está cifrada;
- una ceremonia interrumpida no deja nada, y una segunda creación se rechaza;
- la API presenta la cadena hasta la CA raíz.

### Tests for User Story 1 ⚠️

- [X] T026 [P] [US1] Pruebas del aggregate Key de la CA en `hecate/crates/domain/src/ca/authority.rs`: `CaCreated` solo como primer evento; estados de cada intermedia (`active`, `verify_only`, `retired`) y generación según data-model.md; reconstrucción desde los eventos
- [X] T027 [P] [US1] Pruebas del formato de la copia en `hecate/crates/domain/src/ca/backup.rs`:
  - etiqueta PEM `HECATE ROOT CA KEY`;
  - cabecera de 48 bytes (marca `HRK1`, `opslimit` y `memlimit` de 8 bytes big-endian, sal de 16 y nonce de 12) seguida del cifrado con su etiqueta;
  - datos asociados: la cabecera y los 32 bytes de la huella;
  - una marca desconocida o un contenido corto se rechazan;
  - propiedad de ida y vuelta de la cabecera
- [X] T028 [P] [US1] Pruebas de la PKI de la CA en `hecate/crates/infrastructure/tests/pki_ca.rs`, según los perfiles de contracts/formats.md:
  - CA raíz: autofirmada, `CA:true` con `pathLen 1` crítica, `keyCertSign` y `cRLSign`, 10 años y `CN=Cerbero Root CA <td>`;
  - intermedia: firmada por la CA raíz, `pathLen 0`, 1 año sin pasar de la CA raíz y `CN=Cerbero Intermediate CA <td> <generación>.<versión>`;
  - certificado de Hécate: un único SAN `spiffe://<td>/hecate`, `CA:false`, `digitalSignature`, `serverAuth` y `clientAuth`, `notBefore` un minuto antes y sin pasar de la intermedia;
  - en todos, P-256 con SHA-256 y serie de 128 bits positiva;
  - la huella `sha256:<hex>`; la cadena valida con webpki;
  - la solicitud de la intermedia que genera Hécate verifica
- [X] T029 [P] [US1] Pruebas del sobre de la copia en `hecate/crates/infrastructure/tests/backup.rs`:
  - cifra el PKCS#8 con Argon2id (parámetros del perfil) y AES-256-GCM;
  - abre con la frase y la huella correctas;
  - con otra frase o la huella de otra CA raíz falla sin decir cuál de las dos;
  - un byte cambiado se detecta
- [X] T030 [US1] Pruebas del servicio de creación en `hecate/crates/application/tests/ca_create.rs`:
  - **`begin`:**
    - exige Hécate desbloqueada, sin CA y sin otra ceremonia (si no, `conflict`);
    - valida el trust domain;
    - exige una frase de la CA de 12 caracteres o más y distinta de la de desbloqueo, comprobada con los parámetros guardados (si no, `validation`), sin tocar el contador común (research R9);
    - devuelve la solicitud y una caducidad de 15 minutos.
  - **`confirm`:**
    - comprueba la CA raíz autofirmada y la intermedia de la clave pendiente, firmada por la CA raíz y con su perfil;
    - guarda los eventos, `ca_material` con la clave de la intermedia cifrada bajo la raíz de cifrado (datos asociados `{"generation", "purpose": "ca_intermediate", "root", "version"}`), `hecate_tls` y el certificado de Hécate en `certificate/hecate/<serie>` con `origin: self`, todo en una transacción.
  - **`abort`:** registra su motivo.
  - **Ceremonia pendiente:** una desconocida o caducada da `conflict`; sellar o reiniciar la pierde sin guardar nada (FR-004).
  - **Registro y frase:** las filas `ca_create`; la frase de la CA no se guarda ni se registra.
  - **Corte:** con un almacén que falla dentro de la transacción de `confirm`, como `crash_during_change.rs` del cambio 005, no queda nada guardado (SC-007)
- [X] T031 [US1] Pruebas de la renovación propia en `hecate/crates/application/tests/ca_self_certificate.rs`, con `FakeClock` (FR-012):
  - a los dos tercios de su vida, con Hécate desbloqueada, se renueva con una clave nueva y deja una fila con `subject: hecate`;
  - sellada no se renueva;
  - si caducó sellada, se renueva al desbloquear
- [X] T032 [US1] Pruebas de la ceremonia en la CLI en `hecate/crates/cli/tests/ca_create.rs`, según contracts/cli.md:
  - **Camino feliz:** `hecate ca create --trust-domain cerbero.test --out <tmp>/root-ca.pem` pide la frase, escribe la copia con `0600` y pide la ruta de una copia guardada y la frase. Termina imprimiendo `created ca for cerbero.test` y `root fingerprint sha256:<hex>`.
  - **Copia que no se abre:** con otra frase se envía `abort` con `backup_check_failed`, sale `error: the saved copy does not open with that passphrase`, no se crea la CA y la copia escrita se borra.
  - **Ficheros:** un `--out` que ya existe no se sobrescribe.
  - **Cancelar:** Ctrl-C o fin de entrada envían `abort` con `cancelled`, borran la copia si ya estaba escrita, imprimen `cancelled` y terminan con 1.
  - **Canal:** ninguna petición lleva una clave privada ni la copia.
- [X] T033 [US1] Pruebas de la API tras la creación en `hecate/crates/infrastructure/tests/api_identity.rs`:
  - sin CA no hay escucha;
  - tras confirmar, `HECATE_API_ADDR` presenta la cadena (Hécate, intermedia y CA raíz) con la huella de la ceremonia y el SAN `spiffe://<td>/hecate`;
  - sellada, la TLS funciona y todo responde `unavailable`;
  - al reiniciar con CA, escucha sellada;
  - con el certificado propio caducado (Hécate sellada, o la intermedia caducada), no acepta conexiones, y vuelve a escuchar en cuanto hay un certificado nuevo
- [X] T034 [US1] Pruebas de `status` en `hecate/crates/infrastructure/tests/contract_status_op.rs` y `hecate/crates/cli/tests/status_ca.rs`:
  - `contract_status_op.rs` amplía los campos que fija el cambio 005 con el bloque `ca` (`exists`, `trust_domain`, `root_fingerprint`, y `hecate_certificate` con `serial` y `not_after`), que responde también sellada; sin CA, solo `exists: false`;
  - las líneas `ca: <td> sha256:<hex>`, `hecate certificate <serie> until <instante>` y `ca: none` de `hecate status`, después de las que ya imprime;
  - la salida exacta de `status` en `hecate/crates/cli/tests/cli_secrets.rs` gana la línea `ca: none`
- [X] T035 [P] [US1] Pruebas de contrato de `ca.create.begin`, `ca.create.confirm` y `ca.create.abort` en `hecate/crates/infrastructure/tests/contract_ca_create_ops.rs`, como los `contract_*_ops.rs` del cambio 005:
  - los campos exactos de `data` de contracts/control-protocol.md;
  - `validation`: trust domain mal formado, frase de la CA corta o igual a la de desbloqueo y certificados que no cumplen el perfil;
  - `conflict`: Hécate sellada, ya hay CA, otra ceremonia en curso, ceremonia desconocida o caducada;
  - las filas `ca_create`, también la de `abort` con su motivo
- [X] T036 [US1] Pruebas de memoria de la ceremonia en `hecate/crates/application/tests/memory.rs` y `hecate/crates/infrastructure/tests/memory_ca.rs`:
  - la frase de la CA se suelta tras `begin`;
  - el guardián de la clave de la CA raíz (`RootCaKey`) la borra al soltarse;
  - la clave de una intermedia se borra al terminar la operación que firma, también si falla;
  - doctests `compile_fail` de que los tipos nuevos no se clonan ni se muestran (FR-028)

### Implementation for User Story 1

- [X] T037 [P] [US1] Implementar el aggregate Key de la CA en `hecate/crates/domain/src/ca/authority.rs`
- [X] T038 [P] [US1] Implementar el formato de la copia en `hecate/crates/domain/src/ca/backup.rs`
- [X] T039 [US1] Implementar la PKI de la CA en `hecate/crates/infrastructure/src/pki.rs`, con rcgen y x509-parser:
  - perfiles de la CA raíz, la intermedia y el certificado de Hécate;
  - solicitud de la intermedia;
  - huella;
  - `RootCaKey`, que guarda el PKCS#8 de la clave de la CA raíz en memoria que se borra al soltarse; cada operación que
    firma carga el `KeyPair` de rcgen en un guardián que lo borra con `zeroize` al terminar, también si falla
- [X] T040 [US1] Implementar el sobre de la copia en `hecate/crates/infrastructure/src/backup.rs` con `derive_key` del `CryptoProvider` (perfil de `HECATE_KDF_PROFILE`) y AES-256-GCM de aws-lc-rs con el nonce de la cabecera, que forma parte de los datos asociados
- [X] T041 [US1] Implementar el almacén de la CA en `hecate/crates/infrastructure/src/store/ca.rs`: streams `ca` y `certificate/hecate/*`, escritura, lectura y borrado de `ca_material`, sustitución de `hecate_tls`, y transacciones sobre varios streams
- [X] T042 [US1] Implementar el servicio de creación y la renovación propia en `hecate/crates/application/src/ca/service.rs`:
  - la clave de la intermedia se descifra de `ca_material` con la raíz de cifrado en cada operación que firma y se borra al terminar (research R14);
  - la renovación propia, a los dos tercios de la vida del certificado
- [X] T043 [US1] Añadir `ca.create.begin`, `ca.create.confirm` y `ca.create.abort` en `hecate/crates/infrastructure/src/control/operations.rs`, según contracts/control-protocol.md:
  - la ceremonia pendiente vive en la memoria del servidor, se borra al soltarse y caduca a los 15 minutos;
  - una sola a la vez;
  - `status` gana el bloque `ca`, con el certificado vigente de Hécate
- [X] T044 [US1] Arrancar la API en `hecate/crates/infrastructure/src/api/server.rs` y `hecate/crates/cli/src/lib.rs` (`serve`):
  - al confirmar la ceremonia y al arrancar con CA, solo mientras el certificado propio está vigente;
  - un supervisor de la API en `hecate/crates/infrastructure/src/api/supervisor.rs` lee la CA y el certificado propio y arranca, para o sustituye la configuración del servidor; lo llaman `serve` al arrancar, `ca.create.confirm`, el desbloqueo y la tarea de cada minuto;
  - la clave de `hecate_tls` se lee en `Zeroizing`;
  - la cadena llega hasta la CA raíz
- [X] T045 [US1] Implementar `hecate ca create` y las líneas `ca:` y `hecate certificate` de `hecate status` en `hecate/crates/cli/src/lib.rs`, según contracts/cli.md
- [X] T046 [US1] Programar la renovación propia del certificado de Hécate en la tarea de cada minuto de `hecate/crates/infrastructure/src/tasks.rs`, arrancada por `serve`: si caduca sin poder renovarse, la tarea cierra el listener de la API y lo vuelve a abrir en cuanto hay un certificado nuevo (research R8)

**Checkpoint**: la CA se crea con la ceremonia de la CLI, Hécate se identifica y la API escucha.

---

## Phase 4: User Story 2 - Dar de alta un componente (Priority: P1)

**Goal**: el admin genera tokens de un solo uso y un componente obtiene su primer certificado con uno.
- Hécate emite certificados de 3 días con la identidad del token y registra su clave de firma de llamadas.
- Rechaza lo reservado, lo mal formado y los tokens que no valen.
- Frena las altas fallidas.

**Independent Test**: el componente de prueba se da de alta con un token (quickstart §5, pasos 4 a 6):
- recibe un certificado de 3 días con solo su identidad y su clave queda registrada;
- el token no sirve otra vez, salvo la repetición exacta;
- ninguna clave privada llega a Hécate;
- 10 fallos seguidos frenan las altas.

### Tests for User Story 2 ⚠️

- [X] T047 [P] [US2] Pruebas del aggregate Identity en `hecate/crates/domain/src/identity/identity.rs`:
  - estados sin alta, `enrolled` y `disabled`;
  - la época sube con cada alta y con cada vez que se deshabilita;
  - un token está pendiente si no se ha usado ni anulado, no ha caducado, es de la generación de CA vigente y se creó después del último `IdentityDisabled`;
  - no se crean tokens para identidades reservadas, mal formadas o deshabilitadas
- [X] T048 [P] [US2] Pruebas del aggregate Certificate en `hecate/crates/domain/src/identity/certificate.rs`:
  - estado `valid`, `revoked` o `expired`; uno de otra generación no vale;
  - se puede renovar si es `valid`, de la generación vigente, sin `CertificateRenewed` ni `CertificateRevoked`, con la época de la identidad y la identidad habilitada;
  - su clave vale para llamadas si el certificado es `valid` y la identidad está habilitada;
  - los de Hécate solo admiten la revocación urgente
- [X] T049 [P] [US2] Pruebas del freno en `hecate/crates/domain/src/identity/throttle.rs`:
  - el décimo fallo seguido abre una espera de 1 minuto, y los siguientes de 2, 4 y 8, y después de 15;
  - durante la espera, `rate_limited` con `retry_after_s`;
  - un alta emitida pone el contador a cero;
  - `unavailable` y `rate_limited` no cuentan como fallo;
  - el resumen de cada tanda: fallos que la abrieron y peticiones rechazadas durante la espera
- [X] T050 [P] [US2] Pruebas de la PKI de componentes en `hecate/crates/infrastructure/tests/pki_leaf.rs`:
  - **Solicitud:** PEM estricto y firma válida. Solo se admite P-256; otra clave da `validation`. El SAN tiene que faltar o ser exactamente el SPIFFE ID esperado; si no, `validation`.
  - **Certificado:** el perfil de componente de contracts/formats.md, con la vida configurada y sin pasar de la intermedia.
  - **Clave de firma de llamadas:** PEM `PUBLIC KEY` Ed25519 (RFC 8410), distinta de la clave del certificado.
- [X] T051 [US2] Pruebas del servicio de tokens y altas en `hecate/crates/application/tests/identity_enroll.rs`:
  - **`tokens.create`:**
    - solo con CA y desbloqueada;
    - una identidad reservada o mal formada da `validation`, y una deshabilitada o la intermedia caducada, `conflict`;
    - el token se devuelve una vez, el evento guarda solo el SHA-256 y la caducidad sale de `HECATE_ENROLL_TOKEN_MINUTES`.
  - **`tokens.list` y `tokens.annul`:** la lista trae los pendientes sin su valor; anular uno que no existe da `not_found`.
  - **Alta válida:** certificado de 3 días con solo la identidad, cadena hasta la CA raíz, clave registrada y token usado en la misma transacción.
  - **Repetición:** el mismo token, la misma solicitud y la misma clave dan la misma serie sin eventos nuevos; el mismo token con otra solicitud da `auth`.
  - **Tokens que no valen:**
    - un token usado, caducado, anulado o de otra generación da `auth` y queda registrado;
    - otra identidad en la solicitud da `validation`;
    - un alta rechazada deja el token sin usar (FR-011).
  - **Concurrencia y nueva alta:** una nueva alta deja sin renovación los certificados anteriores (FR-013); dos altas a la vez con el mismo token dan un solo certificado.
  - **`Idempotency-Key`:** reutilizada con otro contenido en la misma identidad da `conflict`; la misma clave en otra identidad no choca.
  - **Freno:** cuenta los fallos, rechaza con `rate_limited` durante la espera también con un token válido, deja una fila por tanda, no toca el contador común y sobrevive a reabrir la base con `SqliteStack` durante una espera (FR-034).
  - **Corte:** con un almacén que falla dentro de la transacción del alta no queda nada a medias, y repetir la petición devuelve el certificado (SC-007).
- [X] T052 [P] [US2] Pruebas de las identidades reservadas en `hecate/crates/application/tests/secrets_reserved.rs` (FR-027):
  - `secrets.create` y `secrets.add_version` sobre `secret://hecate/...` o `secret://admin/...` dan `validation`;
  - revocar y destruir lo que ya existía sigue igual;
  - otras identidades preparan secretos antes del alta
- [X] T053 [US2] Pruebas de la API de alta en `hecate/crates/infrastructure/tests/api_enroll.rs`, con el componente de prueba:
  - antes de enviar el token, comprueba la cadena de Hécate con la huella del token y el SAN `spiffe://<td>/hecate`;
  - `POST /v1/identities:enroll` sin certificado de cliente devuelve los campos del contrato y valida contra el OpenAPI;
  - un certificado de cliente que llegue con el alta no se usa ni la impide;
  - **errores:**
    - `validation`: cuerpo que no es JSON (sin fila), PEM mal formado, otro SAN o sin `Idempotency-Key`;
    - `auth`;
    - `conflict`;
    - `rate_limited`, con `Retry-After`;
    - `unavailable`, con Hécate sellada;
  - un alta tarda menos de 1 segundo (SC-001)
- [X] T054 [US2] Pruebas de las órdenes de tokens en `hecate/crates/cli/tests/tokens.rs`:
  - `hecate tokens create hermes` imprime `token <token>`, `id <id> identity hermes expires <instante>` y `this token is shown only once`;
  - `hecate tokens list` imprime `<token-id> <identidad> expires <instante>`;
  - `hecate tokens annul` imprime `annulled <token-id>`;
  - los errores `error (validation)`, `error (conflict)` y `error (not_found)`
- [X] T055 [US2] Ampliar `hecate/crates/infrastructure/tests/contract_status_op.rs` y `hecate/crates/cli/tests/status_ca.rs` con `enrollment_throttled` y la línea `enrollment throttled: <yes/no>`
- [X] T056 [P] [US2] Pruebas de contrato de `tokens.create`, `tokens.list` y `tokens.annul` en `hecate/crates/infrastructure/tests/contract_token_ops.rs`:
  - los campos exactos de `data`; el token solo aparece en la respuesta de `tokens.create`;
  - `validation`: identidad reservada o mal formada;
  - `conflict`: Hécate sellada, sin CA, identidad deshabilitada o intermedia caducada;
  - `not_found`: token que no existe;
  - las filas con `subject: admin` y la identidad

### Implementation for User Story 2

- [X] T057 [P] [US2] Implementar el aggregate Identity en `hecate/crates/domain/src/identity/identity.rs`
- [X] T058 [P] [US2] Implementar el aggregate Certificate en `hecate/crates/domain/src/identity/certificate.rs`
- [X] T059 [P] [US2] Implementar el freno en `hecate/crates/domain/src/identity/throttle.rs`
- [X] T060 [US2] Implementar en `hecate/crates/infrastructure/src/pki.rs` la lectura de solicitudes, la emisión de certificados de componente y la lectura de la clave de firma de llamadas
- [X] T061 [US2] Implementar el freno en `hecate/crates/infrastructure/src/store/identity.rs` (tabla `enroll_attempts`). Los streams `identity/*` y `certificate/<identidad>/<serie>`, con su listado por prefijo, los sirve el `SqliteIdentityStore` de T041, cuya transacción cubre todos los streams de un alta
- [X] T062 [US2] Rechazar las rutas de `hecate` y `admin` en `secrets.create` y `secrets.add_version` de `hecate/crates/application/src/secrets/service.rs` (FR-027)
- [X] T063 [US2] Implementar tokens, altas, repetición e `Idempotency-Key`, y el freno, en `hecate/crates/application/src/identity/service.rs`
- [X] T064 [US2] Añadir `tokens.create`, `tokens.list` y `tokens.annul`, y `enrollment_throttled` en `status`, en `hecate/crates/infrastructure/src/control/operations.rs`
- [X] T065 [US2] Implementar `POST /v1/identities:enroll` en `hecate/crates/infrastructure/src/api/routes.rs`, con `Idempotency-Key` obligatoria y las filas del registro de data-model.md
- [X] T066 [US2] Implementar el componente de prueba en `hecate/crates/infrastructure/src/testing.rs` y el ejemplo `hecate/crates/infrastructure/examples/component.rs` (orden `enroll --token --state`, con `HECATE_API_ADDR`):
  - `TestComponent` genera una clave P-256 para la TLS y una Ed25519 para firmar, y crea la solicitud con rcgen;
  - usa un cliente hyper-rustls con un verificador propio: cadena hasta la CA raíz fijada por la huella y SAN `spiffe://<td>/hecate`;
  - ese verificador usa el reloj que recibe (`FakeClock` en las pruebas), y el componente presenta en mTLS su certificado y la intermedia que lo firmó;
  - guarda su estado en un directorio con permisos `0600`
- [X] T067 [US2] Implementar `hecate tokens create`, `list` y `annul` y la línea `enrollment throttled` en `hecate/crates/cli/src/lib.rs`
- [X] T068 [US2] Cerrar las tandas del freno en la tarea de cada minuto de `hecate/crates/infrastructure/src/tasks.rs`, con una fila `identity_enroll_throttled` por tanda

**Checkpoint**: un componente se da de alta con un token y las altas fallidas se frenan.

---

## Phase 5: User Story 3 - Renovar el certificado (Priority: P1)

**Goal**: un componente renueva su certificado una sola vez, con el actual y un token corto ligado al canal. Un segundo
intento revoca con urgencia la identidad y lo cuenta el estado.

**Independent Test**: el componente de prueba renueva y obtiene la misma identidad con otra clave, y cada caso no
válido se rechaza. Un segundo intento revoca todos los certificados de la identidad (quickstart §6, pasos 1 y 2).

### Tests for User Story 3 ⚠️

- [X] T069 [P] [US3] Pruebas del verificador del token corto en `hecate/crates/infrastructure/tests/call_token.rs`:
  - solo EdDSA: `none` y HS256 se rechazan;
  - `iss` igual al SPIFFE ID de la conexión y `aud` igual al de Hécate;
  - `exp - iat` de 300 s como mucho, con 30 s de margen;
  - `jti` de 1 a 128 caracteres ASCII imprimibles;
  - firmado con la clave registrada: otra clave se rechaza
- [X] T070 [P] [US3] Pruebas de la caché de `jti` en `hecate/crates/application/src/identity/jti.rs`:
  - la misma pareja `iss` y `jti` se rechaza hasta que caduca, y la entrada caduca;
  - un token con `iat` anterior al arranque menos el margen se rechaza
- [X] T071 [US3] Pruebas del servicio de renovación en `hecate/crates/application/tests/identity_renew.rs`:
  - **Renovación válida:** un certificado válido con un token corto y claves nuevas da un certificado de 3 días con la misma identidad. Queda registrada la clave nueva, y el anterior queda renovado y sin renovación.
  - **Repetición:** con un token nuevo devuelve la misma serie sin efectos.
  - **Certificado que no vale:** caducado, revocado, de una identidad deshabilitada, de otro trust domain o de otra CA raíz da `auth` y queda registrado.
  - **Token que no vale:** sin token, caducado, ya usado, de otra clave o con otro emisor da `auth` y queda registrado.
  - **Petición que no vale:** la clave actual repetida o la petición de otra identidad dan `validation`.
  - **Segundo intento** con un certificado renovado:
    - da `auth` y escribe `SuspiciousRenewalDetected`;
    - revoca con urgencia todos los certificados válidos de la identidad, también el recién emitido, con `subject: hecate`;
    - lo cuenta en `status`.
  - **A la vez:** de dos renovaciones distintas del mismo certificado, una obtiene certificado y la otra, tras el conflicto de versión, se reevalúa y se trata como segundo intento.
  - **Corte:** con un almacén que falla dentro de la transacción no queda nada a medias, y repetir la petición devuelve el certificado (SC-007).
  - **Sellada:** responde `unavailable`.
  - **`status`:** `identities_expiring_24h` y `suspicious_renewals_24h`, también sellada.
- [X] T072 [US3] Pruebas de la API de renovación en `hecate/crates/infrastructure/tests/api_renew.rs`, con el componente de prueba:
  - `POST /v1/certificates:renew` con certificado de cliente y `Authorization: Bearer`, validada contra el OpenAPI;
  - un certificado caducado, de otra CA o la falta de certificado dan `auth` y dejan fila (SC-006);
  - una renovación tarda menos de 1 segundo (SC-001);
  - con una vida de 1 hora y `FakeClock`, el componente que renueva a los dos tercios conserva su identidad (SC-008)
- [X] T073 [US3] Ampliar `hecate/crates/infrastructure/tests/contract_status_op.rs` con `identities_expiring_24h` y `suspicious_renewals_24h`, y `hecate/crates/cli/tests/status_ca.rs` con las líneas `identities expiring within 24h: <n>` y `suspicious renewals within 24h: <n>`

### Implementation for User Story 3

- [X] T074 [US3] Implementar el verificador del token corto en `hecate/crates/infrastructure/src/call_token.rs`, con jsonwebtoken
- [X] T075 [US3] Implementar la caché de `jti` y el instante de arranque en `hecate/crates/application/src/identity/jti.rs`, y su limpieza en `hecate/crates/infrastructure/src/tasks.rs`
- [X] T076 [US3] Implementar el extractor de credenciales en `hecate/crates/infrastructure/src/api/auth.rs` (los certificados de la conexión y el token de `Authorization: Bearer`) y su validación en el caso de uso:
  - exige el certificado de cliente y valida su cadena hasta la CA raíz vigente y su caducidad con el `WebPkiClientVerifier` de rustls, a través del port `PkiProvider` (`verify_client_cert` con la hora del port `Clock`); comprueba que lo emitió Hécate y es conocido, y lee su SPIFFE ID;
  - verifica el token corto con la clave registrada para ese certificado;
  - un fallo da `auth` y deja fila
- [X] T077 [US3] Implementar en `hecate/crates/application/src/identity/service.rs`:
  - la renovación única;
  - la detección del segundo intento, con la revocación urgente de toda la identidad en la misma transacción;
  - la repetición;
  - tras un conflicto de versión, relee los streams y reevalúa la petición, así que una segunda renovación simultánea es un segundo intento
- [X] T078 [US3] Implementar `POST /v1/certificates:renew` en `hecate/crates/infrastructure/src/api/routes.rs`
- [X] T079 [US3] Añadir `identities_expiring_24h` y `suspicious_renewals_24h` al bloque `ca` en `hecate/crates/infrastructure/src/control/operations.rs`, y sus líneas en `hecate/crates/cli/src/lib.rs`
- [X] T080 [US3] Añadir la renovación al componente de prueba y al ejemplo (`renew --state [--from <serie>]`) en `hecate/crates/infrastructure/src/testing.rs` y `hecate/crates/infrastructure/examples/component.rs`

**Checkpoint**: los componentes renuevan una sola vez y un segundo intento revoca la identidad.

---

## Phase 6: User Story 4 - Publicar las claves de firma de llamadas (Priority: P2)

**Goal**: los componentes autenticados obtienen la lista de claves que valen para llamadas, válida 5 minutos, y
cualquier clave por su `kid` para verificar firmas pasadas.

**Independent Test**: tras altas y renovaciones de prueba, `component keys` (quickstart §6, paso 3) comprueba:
- valen la clave nueva y la anterior hasta que caduca su certificado;
- lo deshabilitado desaparece al momento;
- una clave antigua sale por `kid` como `verify_only`.

### Tests for User Story 4 ⚠️

- [X] T081 [P] [US4] Pruebas de las claves en `hecate/crates/application/tests/identity_keys.rs`:
  - la lista solo tiene las claves de certificados válidos con la identidad habilitada;
  - tras renovar, valen la nueva y la anterior hasta que caduca su certificado;
  - deshabilitar o revocar con urgencia la quita al momento;
  - `expires_at` es el momento de la consulta más 300 s;
  - por `kid`, cualquier clave registrada con `active` o `verify_only`, y `not_found` si no existe
- [X] T082 [US4] Pruebas de la API en `hecate/crates/infrastructure/tests/api_keys.rs`:
  - `GET /v1/keys`, `GET /v1/keys/{kid}` y `GET /v1/capabilities`, con certificado y token corto, validadas contra el OpenAPI;
  - los miembros del JWK (`kty`, `crv`, `x`, `kid`, `alg`, `use`, `cerbero_identity`, `cerbero_status` y `cerbero_not_after`);
  - una consulta atendida no deja fila y un rechazo sí (FR-024);
  - sin certificado o sin token, o con un certificado caducado o de otra CA, `auth` con fila;
  - con un año de datos (unos 5.400 certificados), `GET /v1/keys` tarda menos de 1 segundo (plan, Performance Goals)

### Implementation for User Story 4

- [X] T083 [US4] Implementar las consultas de claves en `hecate/crates/application/src/identity/service.rs`
- [X] T084 [US4] Implementar `GET /v1/keys`, `GET /v1/keys/{kid}` y `GET /v1/capabilities` en `hecate/crates/infrastructure/src/api/routes.rs`, sin fila cuando se atienden
- [X] T085 [US4] Añadir `keys` y `key <kid>` al componente de prueba y al ejemplo, en `hecate/crates/infrastructure/src/testing.rs` y `hecate/crates/infrastructure/examples/component.rs`

**Checkpoint**: las claves publicadas permiten verificar llamadas y firmas pasadas.

---

## Phase 7: User Story 5 - Revocar un certificado o deshabilitar una identidad (Priority: P2)

**Goal**: el admin revoca de forma pasiva o urgente, deshabilita y habilita identidades, y las lista.
- Los componentes obtienen la CRL, válida 5 minutos.
- El certificado de Hécate se revoca con urgencia y se reemite solo.

**Independent Test**: las revocaciones y la deshabilitación se reflejan en la renovación, la CRL y las claves, y una
identidad deshabilitada no se da de alta (quickstart §6, pasos 4 a 6).

### Tests for User Story 5 ⚠️

- [X] T086 [P] [US5] Pruebas de la CRL en `hecate/crates/infrastructure/tests/pki_crl.rs`:
  - X.509 v2 firmada por la intermedia emisora, con `thisUpdate` al generarla, `nextUpdate` 300 s después y `cRLNumber` en milisegundos creciente;
  - entradas: los revocados con urgencia sin caducar, con el motivo `keyCompromise` (intento sospechoso) o `unspecified` (admin);
  - webpki la acepta y marca las series revocadas
- [X] T087 [US5] Pruebas del servicio en `hecate/crates/application/tests/identity_revoke.rs`:
  - **Revocación pasiva:** sin renovación, fuera de la CRL y con la clave válida hasta caducar.
  - **Revocación urgente:** el certificado deja de valer, entra en la CRL hasta caducar y su clave deja de valer al momento.
  - **Deshabilitar:**
    - sin renovación, con las claves fuera y los tokens pendientes anulados;
    - no se crean tokens;
    - con `urgent`, los certificados entran en la CRL.
  - **Habilitar:** exige un token nuevo y los certificados anteriores no se renuevan.
  - **Certificado de Hécate:**
    - la revocación urgente lo reemite al momento con otra clave y deja el anterior en la CRL;
    - la pasiva y deshabilitar `hecate` dan `validation`.
  - **Recién renovado:** revocar un certificado que se acaba de renovar revoca solo ese.
  - **Listados:** los campos de `identities.list` e `identities.get`; las filas del registro; `not_found`.
  - **Corte:** una revocación o una deshabilitación con un fallo dentro de la transacción no deja nada a medias (SC-007).
  - **Órdenes repetidas:** deshabilitar una identidad deshabilitada, habilitar una habilitada o revocar un certificado revocado o caducado se acepta sin eventos nuevos; con `urgent`, una revocación o una deshabilitación pasivas pasan a urgentes; una identidad sin alta da `not_found`.
- [X] T088 [US5] Pruebas de la API de revocaciones en `hecate/crates/infrastructure/tests/api_revocations.rs`:
  - `GET /v1/revocations` con certificado y token corto devuelve las CRL en PEM y `expires_at`, validadas contra el OpenAPI;
  - una consulta atendida no deja fila y un rechazo sí;
  - tras una revocación urgente, el componente de prueba deja de aceptar ese certificado en cuanto refresca la CRL (SC-005);
  - tras reemitir el de Hécate, acepta la cadena nueva y ve la serie antigua en la CRL (SC-015);
  - con un año de datos, `GET /v1/revocations` tarda menos de 1 segundo (plan, Performance Goals)
- [X] T089 [US5] Pruebas de las órdenes en `hecate/crates/cli/tests/identities.rs`:
  - `hecate identities list`, `get`, `disable [--urgent]` y `enable`, y `hecate certificates revoke <serie> [--urgent]`;
  - las salidas de contracts/cli.md: `revoked <serie>`, `urgent`, `reissued <serie nueva>`, `disabled <identidad>`, `revoked <n> certificates` y `enabled <identidad>`;
  - los errores
- [X] T090 [P] [US5] Pruebas de contrato de `identities.list`, `identities.get`, `identities.disable`, `identities.enable` y `certificates.revoke` en `hecate/crates/infrastructure/tests/contract_identity_ops.rs`:
  - los campos exactos de `data`, con `revoked` en `identities.disable` y `reissued` al revocar el certificado de Hécate;
  - `validation`: revocación pasiva del certificado de Hécate y deshabilitar `hecate`;
  - `conflict`: Hécate sellada;
  - `not_found`: identidad o serie que no existen, también una identidad que solo tiene tokens;
  - una orden repetida responde con los mismos campos, sin series nuevas en `revoked`;
  - las filas con `subject: admin`, la identidad y, en `certificates.revoke`, la serie

### Implementation for User Story 5

- [X] T091 [US5] Añadir la generación de CRL al port `PkiProvider` de `hecate/crates/application/src/ports.rs` e implementarla en `hecate/crates/infrastructure/src/pki.rs`
- [X] T092 [US5] Implementar revocaciones, deshabilitar, habilitar y los listados en `hecate/crates/application/src/identity/service.rs`, y la reemisión del certificado de Hécate tras su revocación urgente en `hecate/crates/application/src/ca/service.rs`; una orden que no cambia nada no escribe eventos (research R10)
- [X] T093 [US5] Añadir `identities.list`, `identities.get`, `identities.disable`, `identities.enable` y `certificates.revoke` en `hecate/crates/infrastructure/src/control/operations.rs`
- [X] T094 [US5] Implementar `GET /v1/revocations` en `hecate/crates/infrastructure/src/api/routes.rs`
- [X] T095 [US5] Implementar `hecate identities list`, `get`, `disable` y `enable` y `hecate certificates revoke` en `hecate/crates/cli/src/lib.rs`
- [X] T096 [US5] Añadir `revocations` al componente de prueba y al ejemplo, con la comprobación de la CRL con webpki, en `hecate/crates/infrastructure/src/testing.rs` y `hecate/crates/infrastructure/examples/component.rs`

**Checkpoint**: las revocaciones y la deshabilitación se propagan en 5 minutos como mucho.

---

## Phase 8: User Story 6 - Renovar la CA intermedia (Priority: P3)

**Goal**: el admin renueva la intermedia con una copia de la clave de la CA raíz, que solo usa la CLI.
- La anterior deja de emitir, firma la CRL de sus certificados y se destruye cuando caduca el último.

**Independent Test**: tras renovar, las altas y renovaciones usan la intermedia nueva y los certificados anteriores
siguen valiendo hasta caducar. El estado avisaba de la caducidad y la clave anterior desaparece al caducar su último
certificado (quickstart §7, paso 1).

### Tests for User Story 6 ⚠️

- [X] T097 [US6] Pruebas del servicio en `hecate/crates/application/tests/ca_intermediate.rs`, con `FakeClock`:
  - **`begin`:** exige Hécate desbloqueada, CA y ninguna otra ceremonia, también con la intermedia caducada, que es la salida, y devuelve la solicitud y el certificado de la CA raíz.
  - **`confirm`:** activa la versión nueva y deja la anterior en `verify_only`.
  - **Tras renovar:** las emisiones usan la nueva; los certificados anteriores siguen valiendo y se renuevan con la nueva; la CRL de la anterior la firma su clave.
  - **`abort`:** con `wrong_root_key` o `wrong_passphrase` no cambia nada ni cuenta en el contador común.
  - **`status`:** `intermediate_days_left` cuando quedan menos de 30 días, y 0 si ya caducó, también sellada.
  - **Retiro:** al caducar el último certificado de la anterior, `IntermediateRetired` y la fila de `ca_material` borrada (SC-018).
- [X] T098 [US6] Pruebas de la CLI en `hecate/crates/cli/tests/ca_renew_intermediate.rs`:
  - `hecate ca renew-intermediate --root-key <ruta>` pide la frase e imprime `renewed intermediate version <n> until <instante>`;
  - otra frase o la copia de otra CA envían `abort` con `wrong_passphrase`, sin decir cuál de las dos, y una copia que se abre con otra clave, `wrong_root_key`;
  - cancelar imprime `cancelled`;
  - la clave no cruza el canal y la copia abierta se borra de la memoria
- [X] T099 [P] [US6] Pruebas de contrato de `ca.intermediate.begin`, `ca.intermediate.confirm` y `ca.intermediate.abort` en `hecate/crates/infrastructure/tests/contract_ca_intermediate_ops.rs`, y de `intermediate_days_left` en `hecate/crates/infrastructure/tests/contract_status_op.rs`:
  - los campos exactos de `data`;
  - `validation`: una intermedia que no cumple el perfil;
  - `conflict`: Hécate sellada, sin CA, otra ceremonia en curso, ceremonia desconocida o caducada;
  - las filas `ca_intermediate_renew`, también la de `abort` con `wrong_passphrase` o `wrong_root_key`
- [X] T100 [P] [US6] Pruebas de la API tras renovar la intermedia en `hecate/crates/infrastructure/tests/api_identity.rs`:
  - la TLS presenta la intermedia que firmó el certificado vigente de Hécate;
  - un componente con un certificado de la intermedia anterior renueva y obtiene uno de la nueva

### Implementation for User Story 6

- [X] T101 [US6] Implementar la renovación de la intermedia en `hecate/crates/application/src/ca/service.rs`
- [X] T102 [US6] Añadir `ca.intermediate.begin`, `ca.intermediate.confirm` y `ca.intermediate.abort`, y `intermediate_days_left`, en `hecate/crates/infrastructure/src/control/operations.rs`
- [X] T103 [US6] Implementar el retiro de intermedias en la tarea de cada minuto de `hecate/crates/infrastructure/src/tasks.rs` y el borrado en `hecate/crates/infrastructure/src/store/ca.rs`
- [X] T104 [US6] Implementar `hecate ca renew-intermediate` y la línea `intermediate days left` de `status` en `hecate/crates/cli/src/lib.rs`

**Checkpoint**: la intermedia se renueva sin tocar los componentes.

---

## Phase 9: User Story 7 - Sustituir la CA (Priority: P3)

**Goal**: el admin sustituye la CA entera con una prueba y una frase de la CA nueva.
- La copia nueva se confirma antes de activarla.
- Hécate solo acepta la CA nueva, anula lo anterior y destruye al momento la clave de la intermedia anterior.

**Independent Test**: tras sustituir la CA, los certificados y tokens anteriores ya no sirven, un alta nueva obtiene un
certificado de la CA nueva y sin una prueba válida no cambia nada (quickstart §7, paso 2).

### Tests for User Story 7 ⚠️

- [X] T105 [US7] Pruebas del servicio en `hecate/crates/application/tests/ca_replace.rs`:
  - **`begin`:**
    - la prueba es la frase o dos partes; una que no vale da `auth` y cuenta en el contador común, y con espera, `rate_limited`;
    - la frase nueva de la CA sigue las reglas de FR-002.
  - **`confirm`**, en una transacción:
    - generación nueva y certificado de Hécate de la CA nueva;
    - los certificados, las claves y los tokens anteriores dejan de valer;
    - `IntermediateRetired` con `ca_replaced` y las filas de `ca_material` borradas.
  - **Alta nueva:** un token nuevo lleva la huella nueva y su alta tarda menos de 1 segundo (SC-010).
  - **Interrumpida:** no deja nada.
  - **Corte:** un fallo dentro de la transacción de `confirm` no cambia de generación ni borra nada (SC-007).
- [X] T106 [US7] Pruebas de la API tras la sustitución en `hecate/crates/infrastructure/tests/api_replace.rs`:
  - la TLS presenta la cadena nueva;
  - un certificado de la generación anterior recibe `auth` y deja fila (SC-006);
  - el componente de prueba se da de alta con un token nuevo
- [X] T107 [US7] Pruebas de la CLI en `hecate/crates/cli/tests/ca_replace.rs`:
  - `hecate ca replace --out <ruta> [--with-shares]` confirma la copia e imprime `replaced ca for <td> generation <n>` y la huella;
  - los errores;
  - cancelar
- [X] T108 [P] [US7] Pruebas de contrato de `ca.replace.begin`, `ca.replace.confirm` y `ca.replace.abort` en `hecate/crates/infrastructure/tests/contract_ca_replace_ops.rs`:
  - los campos exactos de `data`;
  - `auth`: una prueba que no vale, que cuenta en el contador común; `rate_limited` durante su espera;
  - `validation`: una frase nueva de la CA corta o igual a la de desbloqueo, y certificados que no cumplen el perfil;
  - `conflict`: Hécate sellada, sin CA, otra ceremonia en curso, ceremonia desconocida o caducada;
  - las filas `ca_replace`, también la de `abort` con su motivo

### Implementation for User Story 7

- [X] T109 [US7] Implementar la sustitución en `hecate/crates/application/src/ca/service.rs`
- [X] T110 [US7] Añadir `ca.replace.begin`, `ca.replace.confirm` y `ca.replace.abort` en `hecate/crates/infrastructure/src/control/operations.rs`, y cambiar la configuración TLS (CA raíz nueva) en `hecate/crates/infrastructure/src/api/server.rs`
- [X] T111 [US7] Implementar `hecate ca replace` en `hecate/crates/cli/src/lib.rs`

**Checkpoint**: la CA se sustituye y los componentes se dan de alta de nuevo.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [X] T112 Revisar la memoria (FR-028, research R14) en `hecate/crates/`, como en el cambio 005, y ampliar `hecate/crates/application/tests/memory.rs` y `hecate/crates/infrastructure/tests/memory_ca.rs` con lo que aparezca:
  - cada secreto de paso de esta feature vive en un tipo que se borra al soltarse: la frase de la CA, la clave de la CA raíz y la copia abierta en la CLI, las claves de las intermedias y de la ceremonia pendiente, la clave propia de Hécate y el secreto del token;
  - los caminos de error también los borran;
  - corregir lo que aparezca y apuntar el resultado para la descripción de la PR
- [X] T113 Prueba de fugas en `hecate/crates/infrastructure/tests/identity_no_leak.rs`, como `no_secret_leak.rs` del cambio 005:
  - tras una ceremonia, altas, renovaciones, revocaciones y una sustitución de la CA, ni la base, ni el registro de accesos, ni los logs, ni las respuestas contienen, en ninguna de sus codificaciones, el token de alta o su secreto, la frase de la CA, la clave de la CA raíz o su copia, el PKCS#8 de una intermedia ni la clave privada del componente de prueba;
  - la clave propia de Hécate solo aparece en `hecate_tls` (riesgo aceptado);
  - la prueba detecta un secreto plantado (SC-003, FR-024)
- [X] T114 Ejecutar `cargo test --workspace` en `hecate/` y comprobar que pasan las 423 pruebas del cambio 005, con las ampliaciones a propósito de las convenciones, y todas las nuevas, y que el único cambio de comportamiento de 001 y 002 es FR-027 (FR-030)
- [X] T115 Pasar las comprobaciones de calidad del apartado 2 de `specs/003-hecate-identities/quickstart.md` (FR-031), incluida la de que el árbol normal no activa `tls12`
- [X] T116 Ejecutar la suite en el contenedor Linux del apartado 3 de `specs/003-hecate-identities/quickstart.md`, con `shared/` montado (FR-031)
- [X] T117 Comprobar con el apartado 4 de `specs/003-hecate-identities/quickstart.md` que todas las respuestas de la API, también las de error, validan contra los OpenAPI
- [X] T118 Recorrer los apartados 5 a 9 de `specs/003-hecate-identities/quickstart.md` con el binario release y el ejemplo `component`
- [X] T119 Consolidar los docs según research R19 (FR-032):
  - `docs/hecate.md`: apartados 1, 2, 3, 4, 5, 7, 8, 9, 10 y 12, con el almacenamiento nuevo (streams `ca`, `identity/<nombre>` y `certificate/<identidad>/<serie>`, y sus tablas);
  - `ARCHITECTURE.md`: Identidad y seguridad;
  - `GLOSSARY.md`: Enrollment Token, los términos nuevos y el código `unavailable` en Envelope;
  - `docs/hermes.md`: el código común `unavailable` en su lista de códigos;
  - `DECISIONS.md`: las desviaciones frente a step-ca y SPIFFE y el riesgo aceptado de la clave del certificado de Hécate;
  - `shared/contracts/README.md`: dónde están los OpenAPI, dentro del módulo `cerbero_contracts`, y el sobre común;
  - la orden del apartado 10 de quickstart.md no imprime nada
- [X] T120 Retirar la fila de `003-hecate-identities` de «Cambios pausados» en `DECISIONS.md` (FR-033)
- [X] T121 Proponer al propietario el mensaje del commit de squash y la descripción de la PR:
  - **commit:** `feat:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, con el resultado de T112;
  - deben recoger `hecate/`, `shared/contracts/`, `docs/`, `ARCHITECTURE.md`, `GLOSSARY.md`, `DECISIONS.md` y `specs/003-hecate-identities/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias.
- **Foundational (Phase 2)**: depende del Setup y bloquea todas las historias. T005 y T006 van antes que el resto: son el
  contrato.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2 (Phase 4)**: depende de US1, porque un alta necesita la CA, el certificado de Hécate y la API escuchando.
- **US3 (Phase 5)**: depende de US2, porque se renueva un certificado obtenido en un alta.
- **US4 (Phase 6)**: depende de US3, que crea la autenticación con certificado y token corto.
- **US5 (Phase 7)**: depende de US3, por la autenticación de `revocations` y por los certificados renovados.
- **US6 (Phase 8)**: depende de US1, y de US2 para probar los certificados anteriores; la CRL de la intermedia anterior
  necesita US5.
- **US7 (Phase 9)**: depende de US1 y US2.
- **Polish (Phase 10)**: al final; T120 va en la PR que fusiona.

### Within Each Phase

- Las pruebas van primero y deben fallar; después dominio, aplicación, infraestructura y CLI, en ese orden.
- **Ficheros que se amplían en varias tareas:**
  - `hecate/crates/infrastructure/src/pki.rs`: T039, T060 y T091;
  - `hecate/crates/application/src/identity/service.rs`: T063, T077, T083 y T092;
  - `hecate/crates/application/src/ca/service.rs`: T042, T092, T101 y T109;
  - `hecate/crates/infrastructure/src/control/operations.rs`: T043, T064, T079, T093, T102 y T110;
  - `hecate/crates/infrastructure/src/api/routes.rs`: T065, T078, T084 y T094;
  - `hecate/crates/infrastructure/src/tasks.rs`: T046, T068, T075 y T103;
  - `hecate/crates/cli/src/lib.rs`: T045, T067, T079, T095, T104 y T111;
  - `hecate/crates/infrastructure/tests/contract_status_op.rs`: T034, T055, T073 y T099;
  - `hecate/crates/cli/tests/status_ca.rs`: T034, T055 y T073;
  - `hecate/crates/infrastructure/tests/api_identity.rs`: T033 y T100.

  Esas tareas van en orden.
- **Componente de prueba:** T066 lo crea, y T080, T085 y T096 lo amplían.
- **Contrato:** T015 (el ayudante) va antes que cualquier prueba de la API.

### Parallel Opportunities

- Fundacional: T005 y T006; T007 a T011 y T014; T017 a T020.
- US1: T026 a T029 y T035; T037 y T038.
- US2: T047 a T050, T052 y T056; T057 a T059.
- US3: T069 y T070.
- US4: T081 con T082 una vez escrito el ayudante.
- US5: T086, T087 y T090.
- US6: T099 y T100.
- US7: T105 y T108.

---

## Parallel Example: User Story 2

```bash
Task: "Pruebas del aggregate Identity en hecate/crates/domain/src/identity/identity.rs"
Task: "Pruebas del aggregate Certificate en hecate/crates/domain/src/identity/certificate.rs"
Task: "Pruebas del freno en hecate/crates/domain/src/identity/throttle.rs"
Task: "Pruebas de la PKI de componentes en hecate/crates/infrastructure/tests/pki_leaf.rs"
Task: "Pruebas de las identidades reservadas en hecate/crates/application/tests/secrets_reserved.rs"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: el contrato publicado, la migración y el esqueleto de la API.
2. US1: la ceremonia, el certificado de Hécate y la API escuchando.
3. **Revisión del propietario:** las pruebas de US1 y el recorrido del apartado 5 (pasos 1 a 3) de quickstart.md.

### Siguientes incrementos

1. US2 y US3, las otras dos historias P1: alta y renovación, con su revisión (apartados 5 y 6 de quickstart.md).
2. US4 y US5 (P2): claves publicadas, revocación, CRL y certificado de Hécate.
3. US6 y US7 (P3): intermedia y sustitución de la CA.
4. Polish: memoria, Linux, contrato, quickstart completo, consolidación y mensaje de commit.

La feature se entrega entera en una PR con un commit de squash `feat:`; los incrementos son puntos de revisión.

---

## Phase 11: Convergence

- [X] T122 Añadir pruebas de corte de luz (un almacén que falla dentro de la transacción) para `ca.intermediate.confirm`, la renovación programada del certificado de Hécate y el retiro de una intermedia en `hecate/crates/application/tests/ca_intermediate.rs` y `hecate/crates/application/tests/ca_self_certificate.rs`: cada una deja el estado anterior y funciona al repetirla, per SC-007 / R16 (partial)
- [X] T123 Añadir en `hecate/crates/application/tests/identity_renew.rs` una prueba de que, tras un segundo intento de renovación, la CRL lista todos los certificados de la identidad emitidos antes del intento y las claves publicadas ya no tienen las suyas, per SC-011 (partial)
- [X] T124 Destruir la clave de una intermedia anterior en el instante en que caduca su último certificado, y no en la siguiente pasada de la tarea de cada minuto (hasta 60 s después), en `hecate/crates/application/src/ca/service.rs` y `hecate/crates/infrastructure/src/tasks.rs`, con su prueba, per FR-005 / SC-018 (contradicts)
- [X] T125 Añadir a la tabla de errores de `specs/003-hecate-identities/contracts/control-protocol.md` el `validation` de una CA raíz de sustitución igual a la vigente, el único rechazo nuevo que puede provocar un cliente. Los `conflict` de una intermedia caducada al reemitir el certificado de Hécate y de una CA que cambia durante una ceremonia son salvaguardas que una sola instancia no alcanza: ese certificado nunca dura más que su intermedia, y solo hay una ceremonia a la vez, per contracts/control-protocol.md (partial)

## Phase 12: Convergence

- [X] T126 No volver a armar el temporizador del retiro tras un retiro que falla, en `hecate/crates/infrastructure/src/tasks.rs`: con un fallo persistente del almacén, la tarea repite el retiro sin pausa (cada vuelta carga todos los certificados) en lugar de dejarlo para su siguiente pasada; con una prueba en `hecate/crates/infrastructure/tests/api_identity.rs` que cuente las transacciones que intenta la tarea con un almacén que falla, per Constitution VI / T124 (contradicts)

## Phase 13: Convergence

- [X] T127 Dejar fila en el registro de accesos, con la operación de la ruta y sin `subject` ni identidad (no se autentica a nadie), cuando la API rechaza una petición antes de su caso de uso: con Hécate sellada (`unavailable`, con el resultado que da el canal local a Hécate sellada, `rejected_conflict`, y sin contar en el freno de altas) y con un `X-Request-Id` que no sigue su regla (`validation`, como un `Idempotency-Key` mal formado). Hoy la guarda de `hecate/crates/infrastructure/src/api/server.rs`, `request_context` en `hecate/crates/infrastructure/src/api/envelope.rs` y la comprobación de sellado de `enroll`, `renew` y `queried` en `hecate/crates/application/src/identity/service.rs` responden sin fila ni línea en los logs; con sus pruebas en `hecate/crates/infrastructure/tests/` y la nota en el registro de accesos de `specs/003-hecate-identities/data-model.md`, per FR-024 / SC-006 / contracts/api.md (partial)
- [X] T128 Volver a ejecutar la suite completa en el contenedor Linux del apartado 3 de `specs/003-hecate-identities/quickstart.md`, tras T127: la pasada de T116 es anterior al código de las fases 11 a 13 (`hecate/crates/application/src/ca/service.rs`, `hecate/crates/infrastructure/src/tasks.rs` y pruebas con temporizadores reales), per FR-031 / SC-014 (partial)

## Phase 14: Convergence

- [X] T129 Hacer que el rechazo de un alta por un `X-Request-Id` mal formado cumpla el freno de altas, como el de un `Idempotency-Key` mal formado al que T127 lo equipara: cuenta como fallo y, durante una espera, responde `rate_limited` y cuenta en la tanda en lugar de dejar su propia fila; con Hécate sellada no cuenta, como en `enroll`. Hoy `request_context` lo rechaza antes de `enroll`, así que no cuenta y, con las altas frenadas, cada petición deja una fila. En `hecate/crates/application/src/identity/service.rs` y `hecate/crates/infrastructure/src/api/` (el middleware responde lo que diga el freno), con su prueba en `hecate/crates/infrastructure/tests/api_enroll.rs`, per FR-034 / SC-017 / US2-AC10 (contradicts)
- [X] T130 Reconocer en `operation_of` (`hecate/crates/infrastructure/src/api/routes.rs`) las peticiones `HEAD` a las rutas `GET`, que axum atiende con la misma operación, para que su rechazo antes del caso de uso (Hécate sellada, `X-Request-Id` mal formado) deje su fila como el de `GET`; con su caso en la prueba unitaria de `routes.rs`, per FR-024 (partial)

## Phase 15: Convergence

- [X] T131 CRITICAL: responder con el sobre común también a las peticiones que no son operaciones de la API. Hoy, con Hécate desbloqueada, una ruta que no existe recibe el `404` vacío de axum y un método que la ruta no admite, su `405` vacío, sin `status`, `error` ni `meta`; con Hécate sellada ya responde la guarda. Un `fallback` y un `method_not_allowed_fallback` del router en `hecate/crates/infrastructure/src/api/server.rs` responden `not_found`, como una operación que no existe, sin fila (T127); con su prueba en `hecate/crates/infrastructure/tests/api_server.rs` validada contra el sobre común y la fila `not_found` de la tabla de errores de `specs/003-hecate-identities/contracts/api.md`, per Constitution II / contracts/api.md (partial)

## Phase 16: Convergence

- [X] T132 CRITICAL: hacer que `GET /v1/keys/{kid}` con un `kid` que no es UTF-8 válido una vez descodificado (p. ej. `%FF`) llegue al caso de uso. Hoy lo rechaza el extractor `Path<String>` de axum con un `400` de texto plano, sin el sobre común ni fila. Con `Result<Path<String>, PathRejection>` en `hecate/crates/infrastructure/src/api/routes.rs`, el caso de uso autentica primero y responde `validation` con su fila, como a cualquier `kid` mal formado; con su prueba en `hecate/crates/infrastructure/tests/api_keys.rs` validada contra el OpenAPI, per Constitution II / FR-024 (partial)
