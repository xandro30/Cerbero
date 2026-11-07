# Research: Arranque de Hécate

Fase 0 del plan. Cada decisión resuelve una incógnita del Technical Context de [plan.md](plan.md).

## R1. Persistencia

- **Decision:** SQLite, un fichero en el directorio de datos de Hécate.
- **Rationale:** transacciones atómicas (SC-008), librería estándar, sin servicio ni credenciales; Hécate no
  puede depender de otro servicio para arrancar. Registrado en [ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md).
- **Alternatives considered:** PostgreSQL (dependencia circular de credenciales); ficheros con renombrado
  atómico (más código propio).

## R2. Derivación de la clave a partir de la frase

- **Decision:** Argon2id de libsodium (PyNaCl, `nacl.pwhash.argon2id`) con los límites `SENSITIVE`
  (4 pasadas, 1 GiB), sal aleatoria de 16 bytes por protección. Los parámetros se guardan junto a la protección.
- **Rationale:** PyNaCl ya está previsto en docs/hecate.md; `SENSITIVE` es el perfil de libsodium para claves
  de alto valor que se derivan pocas veces (un desbloqueo por arranque); tarda segundos, dentro de SC-002.
  Guardar los parámetros permite subirlos más adelante (agilidad criptográfica).
- **Alternatives considered:** `argon2-cffi` (otra dependencia para lo mismo); perfil `MODERATE` (256 MiB,
  más rápido, menos coste para el atacante).
- **Medición durante la implementación (2026-11-07, MacBook de desarrollo, 10 núcleos y 24 GB):** `SENSITIVE`
  tardó entre 22 y 40 s por derivación (no "unos segundos") y `MODERATE`, unos 4 s. El perfil se elige con
  `HECATE_KDF_PROFILE` (`sensitive` por defecto, `moderate` admitido) y se guarda en cada protección; para
  cambiarlo basta con cambiar la frase. Con `sensitive`, el recorrido real de la CLI midió 21,9 s para inicializar y 21,8 s para desbloquear
  con la frase (SC-001 y SC-002 se cumplen con margen). La medida en el servidor de casa está pendiente.
  **Decisión del propietario (2026-11-07):** `sensitive` se mantiene como perfil por defecto; `moderate` queda como
  opción explícita. Se revisará con la medida del servidor de casa.

## R3. Cifrado de la raíz

- **Decision:** AES-256-GCM (`cryptography`) con nonce aleatorio de 12 bytes y datos asociados que identifican
  el propósito, la protección (`passphrase` o `recovery`) y la versión de la configuración.
- **Rationale:** es el AEAD que fija docs/hecate.md; los datos asociados impiden intercambiar una protección
  por otra o reutilizar una antigua.
- **Alternatives considered:** XChaCha20-Poly1305 de PyNaCl (válido, pero se mantiene un solo AEAD en Hécate).

## R4. Partes de recuperación (Shamir 2 de 3)

- **Decision:** clave de recuperación aleatoria de 32 bytes que cifra la raíz (R3); se reparte con
  `Crypto.Protocol.SecretSharing.Shamir` de PyCryptodome en dos bloques de 16 bytes con los mismos índices.
  Cada parte lleva los dos fragmentos.
- **Rationale:** ni `cryptography` ni PyNaCl traen Shamir y la constitución prohíbe implementarlo. La función
  de PyCryptodome solo acepta secretos de 16 bytes (comprobado en Python 3.15: 32 bytes da `ValueError`); repartir
  cada bloque por separado mantiene la seguridad de cada uno. Separar la clave de recuperación de la raíz
  permite repartir partes nuevas sin tocar la raíz (FR-012, FR-013). Licencia BSD, compatible con AGPL.
- **Alternatives considered:** repartir la raíz directamente (repartir de nuevo cambiaría lo que protege la
  raíz); implementación propia (prohibida); otras librerías de Shamir (poco mantenidas).

## R5. Formato de una parte

- **Decision:** texto en Base32 de Crockford en grupos de 4 caracteres separados por guiones, con prefijo
  `HEC1-`. Contenido: versión, índice (1-3), identificador del juego de partes (4 bytes), los dos fragmentos
  (32 bytes) y una suma de comprobación (los 4 primeros bytes de SHA-256 del resto).
- **Rationale:** Base32 de Crockford evita confundir caracteres (0/O, 1/I/L) al copiar a mano; la suma detecta
  una parte mal copiada sin gastar un intento (FR-009, SC-005); el identificador del juego distingue una parte
  de un juego antiguo y lo dice con claridad.
- **Alternatives considered:** palabras tipo BIP39 (necesitan lista de palabras y más longitud); hexadecimal
  (más largo y fácil de equivocar).

## R6. Canal local entre la CLI y el proceso

- **Decision:** socket Unix en el directorio de datos, con permisos `0600` y directorio `0700` del usuario de
  Hécate; mensajes JSON de una línea; las respuestas usan el sobre común (`status`, `data`, `error`, `meta`).
  Sin framework: `asyncio` de la librería estándar. En Linux, además, se comprueba que quien conecta tiene el
  mismo uid (`SO_PEERCRED`).
- **Rationale:** el acceso local identifica al admin (spec, Assumptions); no hay superficie de red. El framework
  HTTP se decidirá con la API mTLS interna, que es otra feature.
- **Alternatives considered:** FastAPI sobre socket Unix (adelanta el ADR del framework); TCP en localhost
  (otros usuarios del equipo podrían conectar).

## R7. Códigos de error

- **Decision:** se usan los códigos comunes `validation` (entrada mal formada o parte con suma incorrecta; no
  cuenta como intento), `auth` (frase o partes incorrectas; cuenta), `rate_limited` (espera pendiente, con
  `retry_after_s`) e `internal`. Para "la operación no está permitida en el estado actual" (sellada, ya
  inicializada, sin inicializar) se usa **`conflict`**.
- **Rationale:** `conflict` no está en la lista común que describe Hermes. La constitución (principio II) pide
  revisar el contrato común en lugar de hacer excepciones: al consolidar esta feature, `conflict` se añade a
  la lista común.
- **Alternatives considered:** reutilizar `validation` para los conflictos de estado (mezcla dos causas);
  `provider_unavailable` (nombre propio de Hermes).

## R8. Intentos fallidos

- **Decision:** contador de fallos consecutivos y hora del siguiente intento permitido, persistidos en SQLite.
  A partir del 3.er fallo: espera de 1, 2, 4, 8 minutos y después 15 minutos fijos. Durante la espera se
  rechaza sin evaluar (`rate_limited`) y se registra. Un desbloqueo correcto pone el contador a cero. Frase,
  partes y pruebas para cambiar la protección comparten el contador.
- **Rationale:** FR-016 y las aclaraciones de la spec; persistir evita saltarse la espera reiniciando. Con esta
  serie, en la primera hora caben unos 10 intentos y después 4 por hora (SC-007).

## R9. La raíz en memoria

- **Decision:** la raíz se guarda en un `bytearray` que se sobrescribe con ceros al sellar (mejor esfuerzo); al
  arrancar se desactivan los volcados de memoria (`RLIMIT_CORE = 0`) y en Linux se intenta `mlockall` si hay
  permisos; si no los hay, se registra un aviso y se sigue.
- **Rationale:** FR-014. Python no garantiza borrar la memoria: es un riesgo ya aceptado en DECISIONS que
  cubrirá el adapter de hardware.

## R10. Herramientas

- **Decision:** CLI con `argparse` y `getpass` (librería estándar); pruebas con pytest e hypothesis
  ([ADR 0004](../../docs/adr/0004-pruebas-pytest-hypothesis.md)).
- **Rationale:** la CLI tiene pocos comandos y debe leer la frase sin mostrarla; no justifica otra dependencia.

## R11. Compatibilidad con Python 3.15

- **Decision:** dependencias de ejecución `cryptography`, `pynacl` y `pycryptodome`.
- **Rationale:** comprobado el 2026-11-07: instalan y funcionan en Python 3.15.0 (cryptography 50.0.2, pynacl
  1.6.2, pycryptodome 4.3.0). Licencias Apache-2.0/BSD y BSD, compatibles con AGPL-3.0-or-later.
