# Research: Hécate en Rust: migración con paridad de las features 001 y 002

Fase 0 del plan. La base es la Hécate en Python de `main` en `0287c38` (unas 3.160 líneas en `hecate/src` y 346
funciones de prueba en 46 ficheros), los ADRs 0005 a 0008 y lo que dejó el port descartado del 2026-11-08: sus hallazgos
se recogen aquí como decisiones, no como código heredado.

## R0. Decisiones del propietario en el plan

- **Decision:** un crate por capa; tabla propia de caracteres imprimibles de Unicode 17; `time` 0.3 para las fechas; y
  el generador de vectores en `specs/005-hecate-rust/vectors/`, con los vectores en las pruebas de `hecate/`. Al
  preparar las tareas y en el análisis, además: las pruebas de la CLI llaman a las órdenes dentro del proceso con un
  terminal simulado (R15); las entradas siguen su formato estándar (el JSON del canal y del tipo `json`, los PEM y
  `expires_at`), sin las tolerancias de Python (R6 y R8); y las respuestas, los logs y el JSON guardado no imitan los
  separadores ni los escapes de `json.dumps` (R9 y R11). Lo recogen las aclaraciones de la spec y FR-018.
- **Rationale:** son las cuatro bifurcaciones que el plan no podía resolver solo: estructura, una diferencia de Unicode
  que rompe la paridad, una dependencia que no está en el ADR 0007 y dónde queda un script en Python cuando se borra la
  Hécate en Python. Los detalles están en R1, R7, R6 y R5. Las de las tareas y el análisis: cómo trasladar las pruebas
  interactivas de la CLI y qué hacer con las entradas que las librerías de Python aceptan fuera del estándar. Al canal
  llegarán peticiones de otras apps, y otras apps leerán los secretos, así que cada entrada se verifica con su formato
  estándar y se traduce a los casos de Hécate, sin imitar las librerías de Python.
- **Alternatives considered:** un solo crate con módulos; ICU4X o aceptar la diferencia de Unicode; `jiff` o `chrono`;
  guardar solo los vectores; lanzar el binario con tuberías en las pruebas de la CLI; imitar `json.loads`, `json.dumps`,
  `fromisoformat` y los espacios Unicode de los PEM.

## R1. Workspace y crates

- **Decision:** `hecate/` es un workspace de Cargo (resolver 3, edition 2024) con cuatro crates en `hecate/crates/`:
  `hecate-domain`, `hecate-application`, `hecate-infrastructure` y `hecate-cli`, cuyo binario se llama `hecate`. Las
  dependencias van `cli → infrastructure → application → domain`. Las versiones, los lints (`unsafe_code = "forbid"` y
  clippy con todos los avisos) y los metadatos (`license = "AGPL-3.0-or-later"`, `publish = false`) se declaran una vez
  en el workspace.
- **Rationale:** Cargo impide que el dominio dependa de la infraestructura, así que la regla de dependencia del principio
  III se comprueba al compilar. La constitución ya dice que cada componente Rust es «un workspace de Cargo en su carpeta,
  fuera del de uv, con sus crates y sus pruebas». Los crates son las capas del esqueleto más el binario, no piezas
  nuevas.
- **Alternatives considered:** un solo crate con módulos (la regla de dependencia quedaría solo en la revisión).

## R2. Toolchain y versiones

- **Decision:** Rust 1.99.1, la última estable, fijada en `hecate/rust-toolchain.toml` (dentro del componente, no en la
  raíz, para que cada pieza en Rust lleve la suya). Dependencias en su última versión estable el 2026-11-08: aws-lc-rs
  1.19.0, libsodium-rs 0.3.1, vsss-rs 6.0.1, tokio 1.54.1, rusqlite 0.41.1 (`bundled`), clap 4.6.7, tracing 0.1.44,
  tracing-subscriber 0.3.23, serde 1.0.229, serde_json 1.0.154, thiserror 2.0.21, zeroize 1.9.1,
  data-encoding 2.11.1, nix 0.31.3, time 0.3.55 y rand_core 0.10.1. Para pruebas: proptest 1.12.0, assert_cmd 2.2.2,
  predicates 3.1.4 y tempfile 3.27.0. Herramienta: cargo-deny 0.20.2.
- **Rationale:** la regla de versiones del ADR 0007. Fuera del ADR 0007 solo entran `time` (R6) y `rand_core`, que es el
  rasgo que vsss-rs pide al generador aleatorio y se cumple con un adaptador sobre el de aws-lc-rs; ninguna escribe C ni
  bindings a mano (constitución, «Herramientas»).
- **Alternatives considered:** `sha2` para la suma de comprobación de las partes y `subtle` para comparaciones (no hacen
  falta: aws-lc-rs da SHA-256, y la suma de las partes no es secreta); el toolchain en la raíz del repositorio (ataría
  a todas las piezas en Rust a la misma versión).

## R3. Correspondencia de primitivas

- **Decision:**

  | Uso | Python | Rust |
  |---|---|---|
  | Aleatorios | `os.urandom` | `aws_lc_rs::rand::SystemRandom` |
  | Clave de la frase | Argon2id de libsodium (PyNaCl), perfiles `sensitive` (4, 1 GiB) y `moderate` (3, 256 MiB) | Argon2id de libsodium (libsodium-rs), mismos perfiles y parámetros guardados |
  | Cifrado | AES-256-GCM (`cryptography`), nonce de 12 bytes, texto cifrado con la etiqueta al final | `aws_lc_rs::aead::AES_256_GCM` con nonce explícito, mismo orden de bytes |
  | Derivación | HKDF-SHA256 con `salt=None` | HKDF de aws-lc-rs con sal vacía (es equivalente) |
  | Huella | HMAC-SHA256 | HMAC de aws-lc-rs |
  | Suma de las partes | SHA-256 (`hashlib`) | `aws_lc_rs::digest` |
  | Shamir 2 de 3 | PyCryptodome, dos mitades de 16 bytes en GF(2^128) | vsss-rs en GF(256), los 32 bytes de una vez |

  Los datos asociados son JSON con las claves ordenadas y sin espacios (`separators=(",", ":")`), byte a byte: raíz
  (`purpose`, `protection`, `config_version`), clave de ámbito (`purpose`, `identity`, `key_version`), clave de versión
  (`purpose`, `path`, `version`, `scope`, `key_version`) y valor (`purpose`, `path`, `version`, `scope`). La huella son
  los 8 primeros bytes de HMAC-SHA256 con una clave derivada por HKDF de la clave de ámbito (`info = "hecate/fingerprint"`),
  en hexadecimal y en grupos de cuatro separados por guiones.
- **Rationale:** el ADR 0007 elige las librerías; esta tabla asegura que dan los mismos bytes (FR-006). El port descartado
  ya comprobó que la HKDF con sal vacía equivale a `salt=None` y que la serialización de los datos asociados es idéntica.
- **Alternatives considered:** ninguna: cualquier diferencia en esta tabla rompería FR-006.

## R4. Formato nuevo de las partes: `HEC2`

- **Decision:** misma estructura que `HEC1` con la versión 2: prefijo `HEC2`, cuerpo de 42 bytes (versión `0x02`, índice
  de 1 a 3, identificador del juego de 4 bytes y los 32 valores de la parte en GF(256)), suma de comprobación de 4 bytes
  (los primeros de SHA-256 del cuerpo), Crockford Base32 en grupos de cuatro separados por guiones, y la misma limpieza al
  leer (sin espacios ni guiones, en mayúsculas, O→0 e I/L→1). Una parte que no empieza por `HEC2` (también una `HEC1`) se
  rechaza con `validation` antes de contar como intento. El contrato está en
  [contracts/recovery-share.md](contracts/recovery-share.md).
- **Rationale:** el único cambio de formato de lo que se cifra o se reparte que permite la spec (FR-005); conservar la
  estructura mantiene las reglas de 001 (detección de errores de copia, índice, juego) y su código de validación.
- **Alternatives considered:** el formato nativo de vsss-rs serializado tal cual (cambiaría también la longitud y la
  detección de errores de copia).

## R5. Vectores de prueba y esquema

- **Decision:** un script, `specs/005-hecate-rust/vectors/generate.py`, ejecutado con la Hécate en Python de `main` en
  `0287c38` antes de borrarla, escribe ficheros JSON en `hecate/tests/vectors/`, con la versión de Python y de Unicode en
  la cabecera. Categorías:

  | Fichero | Qué fija |
  |---|---|
  | `kdf.json` | Clave derivada de una frase con sal y límites dados (los mínimos y el perfil `moderate`) |
  | `root.json` | La raíz protegida por la frase y por la clave de recuperación, con su nonce, sus datos asociados y el JSON con que `seal_config` guarda cada protección |
  | `envelope.json` | Clave de ámbito, clave de versión y valor cifrados con nonces fijos, y material generado por Python que Rust debe abrir |
  | `aad.json` | Los bytes exactos de cada dato asociado |
  | `fingerprint.json` | Huella de valores con claves de ámbito dadas |
  | `events.json` | El JSON de cada tipo de evento tal como lo guarda Python; Rust lo lee y guarda el mismo contenido (se compara el JSON leído, no sus bytes) |
  | `instants.json` | Instantes y su `isoformat`, y entradas que `fromisoformat` acepta (con su resultado) o rechaza; las que acepta fuera de ISO 8601 extendido van marcadas como cambio a propósito (R6, FR-018) |
  | `values.json` | Valores `string`, `json` y `pem` con el resultado de Python; los que Python aceptaba fuera del estándar (`json` con `NaN`, `Infinity`, números fuera de rango, sustitutos sueltos o más de 127 niveles; `pem` con espacios que no son ASCII o con un bloque vacío) van marcados como cambio a propósito (FR-018) |
  | `printable.json` | Los rangos de caracteres imprimibles según `str.isprintable` de Python 3.15 |
  | `schema.sql` | El esquema que crea la versión en Python (`sqlite_master`, con tablas, disparadores y pragmas) |

  Los vectores de cifrado se generan con nonces y claves fijos (para comparar bytes) y con material aleatorio que Rust
  solo tiene que abrir (para no depender del orden en que se piden aleatorios).
- **Rationale:** las pruebas de comportamiento pasan aunque cambie el formato, porque cifran y descifran con la misma
  implementación (clarify, pregunta 1). El generador queda en `specs/` como registro del cambio, sin quedarse dentro del
  workspace de Rust (R0).
- **Alternatives considered:** abrir una base de datos completa creada por Python (descartada en el clarify); fijar un
  generador aleatorio común en las dos implementaciones (frágil: depende del orden de las llamadas).

## R6. Fechas e instantes

- **Decision:** `time` 0.3. `OffsetDateTime` guarda el desfase que escribió el admin en `expires_at`; el reloj da UTC
  truncado al microsegundo. Se escribe como `datetime.isoformat()`: sin fracción si los microsegundos son 0 y con seis
  cifras si no, y el desfase como `+HH:MM`. `expires_at` se lee en ISO 8601 extendido (FR-018) con un analizador propio:
  `AAAA-MM-DD`, o `AAAA-MM-DDTHH:MM[:SS[.fracción]]` con `T` o un espacio entre fecha y hora, la fracción truncada al
  microsegundo y un desfase opcional, `Z` o `±HH:MM`; sin desfase, UTC, como en Python. Las rarezas que acepta
  `fromisoformat` (cualquier carácter entre fecha y hora, fechas de semana u ordinales, formato básico, `24:00`,
  desfases con segundos) se rechazan con el mensaje de Python, «expires_at must be an ISO 8601 date and time». El mismo
  analizador lee los instantes guardados. `instants.json` lo comprueba, con esas rarezas marcadas como cambio a
  propósito.
- **Rationale:** la paridad incluye la validación salvo FR-018, y estos instantes se guardan en el modelo de datos
  (FR-006). `time` es madura y ligera y su tipo con desfase encaja con `expires_at`. Un analizador del formato estándar
  es más pequeño que una copia de `fromisoformat`.
- **Alternatives considered:** `jiff` (0.x), `chrono` (API más antigua); copiar todas las rarezas de `fromisoformat`
  (descartado por el propietario, FR-018); los formatos de la propia crate (RFC 3339 exige segundos y desfase, y su ISO
  8601 acepta formato básico y fechas de semana).

## R7. Texto imprimible: tabla de Unicode 17

- **Decision:** un módulo `printable` en `hecate-domain` con los rangos de caracteres imprimibles de Unicode 17, generados
  por el script de R5 a partir de la base de datos de Unicode 17 que trae Python 3.15 y con la misma regla que
  `str.isprintable`. Una prueba compara la tabla con `printable.json`. La longitud de `rotate_every` se cuenta en
  caracteres, como en Python, no en bytes.
- **Rationale:** unicode-general-category 1.1.0, la última, está en Unicode 16 y difiere en 4.803 caracteres (R0); una
  tabla propia da la paridad exacta sin dependencias.
- **Alternatives considered:** ICU4X (`icu_properties` 2.4, mucho más pesada); unicode-general-category con la diferencia
  documentada.

## R8. Validación de entradas

- **Decision:** se reproduce el comportamiento observable de la versión en Python, salvo las entradas que no siguen su
  formato estándar (FR-018):
  - `pem` sigue la estructura de Python (la misma expresión, con la etiqueta igual en `BEGIN` y en `END`, y base64
    estricto con relleno solo al final; data-encoding acepta `=` entre grupos y hay que comprobarlo aparte), pero con
    los espacios de RFC 7468 (espacio, tabulador, CR, LF, VT y FF) en lugar de los de Unicode, y sin bloques vacíos
    («a PEM block is empty»);
  - `json` acepta solo JSON estándar: el valor se lee con serde_json como `Value`, igual que las peticiones del canal,
    así que se rechazan `NaN`, `Infinity`, los números que no caben en un `f64`, los sustitutos sueltos y más de 127
    niveles de anidamiento, el límite de serde_json;
  - los valores solo se validan al crear y al rotar, nunca al leer;
  - el canal lee la petición como JSON estándar (R9), comprueba él mismo los argumentos de texto y de lista
    obligatorios, y pasa sin tocar a los servicios, como valor JSON, `max_versions`, `rotate_every`, `expires_at`,
    `identity`, `version`, `expected_version` y la prueba; el orden de comprobación de los servicios decide el error y
    la fila del registro de accesos.
- **Rationale:** hallazgos del port descartado, comprobados con miles de entradas contra Python; `values.json` los fija.
  Lo único que no se reproduce son las tolerancias de las librerías de Python con el JSON, los PEM y las fechas (R6): el
  propietario decidió verificar cada entrada con su formato estándar y traducirla a los casos de Hécate.
- **Alternatives considered:** validar «mejor» que Python en lo demás (rompería la paridad; cualquier cambio necesita al
  propietario); imitar `json.loads` y los espacios Unicode de los PEM (descartado por el propietario).

## R9. Canal local

- **Decision:** socket Unix en `HECATE_SOCKET_PATH`, una petición JSON por línea (máximo 1 MiB) y el mismo sobre
  (`status`, `data`, `error` y `meta` con `request_id` y `duration_ms`), con los mismos campos en el mismo orden y los
  mismos códigos (`validation`, `auth`, `rate_limited`, `not_found`, `conflict`, `internal`). El servidor usa tokio y
  comprueba la credencial del par: solo atiende al usuario del propio proceso. Las operaciones se ejecutan de una en una,
  como en Python, y la derivación de la frase va fuera del hilo del reactor. Las peticiones se leen con serde_json como
  JSON estándar en UTF-8 (FR-018) y las respuestas se escriben con serde_json, con los mismos campos en el mismo orden y
  sin imitar los separadores ni los escapes de `json.dumps` (FR-003).
- **Rationale:** FR-003 y los contratos de 001 y 002; ejecutar de una en una conserva el comportamiento que comprueban las
  pruebas de concurrencia.
- **Alternatives considered:** atender operaciones en paralelo (cambiaría el orden observable de los efectos).

## R10. Registro de accesos y errores

- **Decision:** en el sellado solo se registra un rechazo cuando el error es de Hécate (un fallo interno no deja fila); en
  los secretos se registra como fallo cualquier error. Los mensajes de error son los de Python donde las pruebas los
  comprueban.
- **Rationale:** hallazgo del port descartado; es comportamiento observable del registro de accesos (FR-002).
- **Alternatives considered:** unificar los dos criterios (cambio de comportamiento sin decisión).

## R11. Logs

- **Decision:** una línea JSON por evento a stdout con las mismas claves que en Python (`ts`, `level`, `logger`, `event`,
  los campos explícitos y, si hay error, solo su tipo en `exception`), con un formateador propio sobre tracing que
  escribe con serde_json, sin imitar los separadores ni los escapes de `json.dumps`.
- **Rationale:** el formato de los logs no es contrato de la CLI, pero conservarlo mantiene válido el doc de Hécate y lo
  que comprueban las pruebas de fugas (ningún secreto en los logs).
- **Alternatives considered:** el formato JSON por defecto de tracing-subscriber (otras claves).

## R12. Proceso

- **Decision:** al arrancar, sin volcados de memoria (`RLIMIT_CORE` a 0) y, en Linux, `mlockall` para que la raíz no vaya
  a swap; si falla, o en macOS, un aviso como en Python. Se usan las funciones seguras de nix.
- **Rationale:** paridad con 001; `unsafe_code = "forbid"` se mantiene.
- **Alternatives considered:** llamar a libc directamente (exigiría código inseguro).

## R13. Capa de aplicación

- **Decision:** servicios síncronos; los puertos son traits que se pasan como `Arc<dyn …>`, y las transacciones son
  cierres que reciben un manejador de escritura. tokio solo vive en la infraestructura (canal y señales, también el
  Ctrl-C de la CLI y la parada de `serve`).
- **Rationale:** es el mismo diseño de la versión en Python y el que el propietario aprobó para el port descartado; deja el
  dominio y la aplicación sin runtime y fáciles de probar con dobles en memoria.
- **Alternatives considered:** puertos asíncronos en toda la aplicación (más complejidad sin beneficio en un servicio que
  atiende de uno en uno).

## R14. Secretos de paso en memoria

- **Decision:** la frase, las partes, la clave derivada de la frase, la clave de recuperación, las claves de ámbito y de
  versión y los valores en claro (la lista de data-model) viven en tipos de zeroize que se borran al soltarse,
  sin `Clone` ni `Debug` que los muestre; la raíz vive en el guardián de la raíz y se borra al sellar. La revisión sigue
  esa lista, y las pruebas comprueban al compilar que cada tipo se borra al soltarse y, al ejecutar, que `zeroize` deja
  los bytes a cero.
- **Rationale:** FR-007 y la respuesta 4 del clarify; sin código inseguro no se puede inspeccionar la memoria liberada,
  así que el diseño de tipos es la garantía.
- **Alternatives considered:** volcar la memoria del proceso en una prueba (descartado en el clarify).

## R15. Traslado de pruebas y mapeo

- **Decision:** cada prueba de Python tiene una en Rust en el mismo nivel (unitarias junto al código; integración y
  contrato en `tests/` de cada crate), y las propiedades de hypothesis pasan a proptest. Las de la CLI, que en Python
  llaman a las órdenes con un terminal simulado que responde a la petición de partes con las que la CLI acaba de
  imprimir, hacen lo mismo: `hecate-cli` es biblioteca y binario, las órdenes reciben un trait de terminal y las pruebas
  las llaman dentro del proceso con un terminal guionizado y el servidor en un hilo; assert_cmd cubre lo que solo da un
  proceso (la salida 2 sin configuración, los errores de uso y un recorrido de humo). Cada prueba en Rust lleva un
  comentario con el identificador de su origen en Python, y `specs/005-hecate-rust/test-mapping.md` relaciona las 346
  funciones con sus equivalentes y marca los cambios a propósito. Los dobles en memoria de `fakes.py` pasan a un módulo
  de soporte de pruebas de la aplicación, y sus ayudantes de integración (`short_dir`, `ServerThread` y `SqliteStack`), a
  uno de la infraestructura que usan también las pruebas de la CLI. Se escribe primero la prueba trasladada y se ve
  fallar, como exige el principio IV.
- **Rationale:** FR-008 y el ADR 0006; un terminal simulado traslada las pruebas interactivas tal cual (assert_cmd solo
  admite una entrada fija, y las partes se conocen cuando la CLI las imprime).
- **Alternatives considered:** agrupar por comportamiento en lugar de caso a caso (pierde la trazabilidad que pide la
  spec); lanzar el binario con tuberías en las pruebas interactivas (más fontanería y riesgo de bloqueos).

## R16. Contenedor Linux

- **Decision:** la suite completa se ejecuta también en un contenedor con la imagen oficial `rust:1.99.1-bookworm`,
  montando `hecate/` y con `--ulimit memlock=-1:-1` para que `mlockall` funcione. La orden está en
  [quickstart.md](quickstart.md); si la imagen necesita paquetes para compilar AWS-LC o libsodium, se añaden ahí mismo.
- **Rationale:** FR-017: cubre la credencial del par, el bloqueo de memoria y los volcados en Linux sin un fichero de
  configuración más.
- **Alternatives considered:** un Containerfile propio (solo si la orden se complica).

## R17. Calidad

- **Decision:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo deny check`.
  `hecate/deny.toml` permite las licencias permisivas habituales y la de OpenSSL solo en `aws-lc-sys` (y en `ring`, si
  llegara a entrar) por el ADR 0008, rechaza avisos de RUSTSEC, avisa de versiones duplicadas y solo admite crates.io.
- **Rationale:** FR-010 y los ADRs 0006 y 0008.
- **Alternatives considered:** ninguna.

## R18. Retirada de Python y consolidación

- **Decision:** cuando la versión en Rust cumple la paridad (vectores ya generados), se borran `hecate/src`,
  `hecate/tests` y `hecate/pyproject.toml`, se quita `hecate` de los miembros de `pyproject.toml` y se regenera `uv.lock`;
  `hecate/README.md` se queda. Después se consolidan:
  - el README: el árbol del repositorio y el párrafo de los proyectos (Hécate como workspace de Cargo; lenguajes en el
    ADR 0005);
  - el doc de Hécate: las librerías del `CryptoProvider` en los apartados 1 y 7, cómo se pide el valor de un secreto en
    el apartado 4 y el riesgo de memoria del apartado 12;
  - DECISIONS: el riesgo de memoria reescrito para la versión en Rust y la fila de Shamir sin PyCryptodome.
- **Rationale:** FR-011 a FR-014 y la consolidación de la constitución (los docs se actualizan cuando lo implementado está
  consolidado).
- **Alternatives considered:** borrar Python en otro cambio (dos implementaciones a la vez se desincronizan).

## R19. Lo que no cambia

| Qué | Por qué |
|---|---|
| Los contratos del canal de 001 y 002 | Se conservan tal cual (FR-003); las specs fusionadas no se editan |
| Orthros, su README y la feature 003 con su fila en DECISIONS | Fuera de alcance (FR-015) |
| Los ADRs | Este cambio los implementa; no los reabre |
| El esqueleto hexagonal y Event Sourcing en `Secret` y `Key` | Lo fija la constitución y la tabla de intensidad de ARCHITECTURE |
