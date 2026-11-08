# Research: Secretos de Hécate

Fase 0 del plan. Cada decisión resuelve una incógnita del Technical Context de [plan.md](plan.md). La base es lo
que ya existe tras la feature 001: el sellado, SQLite ([ADR 0003](../../docs/adr/0003-persistencia-hecate-sqlite.md)),
el canal local, el registro de accesos y el `CryptoProvider`.

## R1. Event Sourcing de Secret y Key

- **Decision:** los aggregates `Secret` y `Key` (la clave de cada identidad) se guardan como secuencia de eventos en
  SQLite, una tabla `events` append-only con un stream por aggregate (`secret/<ruta>`, `key/<identidad>`) y control
  de concurrencia por versión esperada del stream.
- **Rationale:** la tabla de intensidad de ARCHITECTURE fija Event Sourcing para los cinco aggregates de Hécate, y el
  diseño exige que "la historia sea la auditoría". La versión esperada del stream es justo lo que pide FR-017.
- **Alternatives considered:** tablas de estado actualizadas en sitio (más simple, pero contradice el diseño y pierde
  la historia de cada versión).

## R2. Los eventos no llevan secretos; el material cifrado va aparte

- **Decision:** los eventos solo llevan metadatos (ruta, versión, estado, fechas, huella, quién). El material
  cifrado vive en dos tablas aparte: `secret_material` (por versión: texto cifrado, nonce y la clave de la versión
  envuelta) y `key_material` (por versión de la clave de ámbito: la clave envuelta por la raíz).
- **Rationale:** el diseño dice que "ningún evento lleva un valor", y el borrado criptográfico (FR-009) exige poder
  eliminar la clave de una versión, cosa imposible en un registro append-only. Destruir borra la fila de
  `secret_material` y añade el evento `VersionDestroyed`.
- **Alternatives considered:** guardar el material cifrado en el evento (la destrucción no podría borrarlo).

## R3. Cifrado por capas

- **Decision:** raíz → clave de ámbito (32 bytes, una por identidad, creada al guardar su primer secreto) → clave
  de versión (32 bytes, una por versión) → valor. Las tres capas con AES-256-GCM del `CryptoProvider` existente. Datos
  asociados:
  - clave de ámbito: `{"purpose": "scope_key", "identity": i, "key_version": k}`;
  - clave de versión: `{"purpose": "dek", "path": p, "version": v, "scope": i, "key_version": k}`;
  - valor: `{"purpose": "secret", "path": p, "version": v, "scope": i}`.
- **Rationale:** es la jerarquía de docs/hecate.md; los datos asociados ligan cada cifrado a su ruta, versión e
  identidad (FR-004), así que mover un dato cifrado a otra ruta impide descifrarlo.
- **Alternatives considered:** cifrar el valor directamente con la clave de ámbito (sin clave por versión no hay
  borrado criptográfico por versión).

## R4. La huella

- **Decision:** HMAC-SHA256 del valor con una clave de huella derivada de la clave de ámbito (HKDF-SHA256,
  `info = "hecate/fingerprint"`), truncado a 8 bytes y mostrado como `ab12-cd34-ef56-7890`. Se calcula al escribir y
  se guarda en el evento.
- **Rationale:** una huella sin clave (SHA-256 del valor) permitiría adivinar por fuerza bruta valores débiles a
  quien vea la lista; con clave, solo Hécate puede calcularla (FR-019, SC-008). Como todas las operaciones exigen
  Hécate desbloqueada, la clave está disponible al escribir.
- **Alternatives considered:** SHA-256 truncado sin clave (fuerza bruta offline); no mostrar huella (el admin no
  podría comprobar lo que escribió).

## R5. Periodo de gracia sin procesos en segundo plano

- **Decision:** el paso de `deprecated` a `revoked` al terminar la gracia se calcula al leer: una versión `deprecated`
  cuya fecha de deprecación más la gracia ya pasó se trata como `revoked`. Restaurar una versión `revoked` la deja
  `deprecated` con una gracia nueva desde ese momento.
- **Rationale:** Hécate puede estar sellada o parada cuando vence la gracia; calcularlo al leer cumple SC-005 sin
  temporizadores ni eventos pendientes, y es determinista con el `Clock` de las pruebas.
- **Alternatives considered:** un proceso que añada eventos `VersionRevoked` al vencer (más piezas y huecos mientras
  Hécate está parada).

## R6. Destruir exige prueba: colaboración con el sellado

- **Decision:** `SealingService` expone dos operaciones públicas que hoy son internas: obtener la raíz (solo si está
  desbloqueada; si no, `conflict`) y verificar una prueba (frase actual o dos partes) con el contador común de
  intentos. El nuevo `SecretsService` las usa; no duplica ni la raíz ni el contador.
- **Rationale:** FR-009 y la aclaración de la spec piden el mismo mecanismo de prueba y el mismo contador que la
  feature 001.
- **Alternatives considered:** que `SecretsService` lea la configuración de sellado y verifique por su cuenta
  (duplicaría reglas de seguridad).

## R7. Resolución sin canal

- **Decision:** `resolve(identity, references)` es un caso de uso de la aplicación cuya identidad llega como
  argumento, probado con pruebas unitarias e integración. No se publica en el canal local ni en ningún otro.
- **Rationale:** aclaración Q1 de la spec; el canal local es del admin, que no puede leer valores (Q2), así que
  exponer ahí la resolución rompería FR-019.

## R8. Operaciones del admin en el canal local

- **Decision:** operaciones nuevas `secrets.create` (crea el secreto con su versión 1), `secrets.add_version` (añade una versión, con `expected_version` obligatoria), `secrets.list`, `secrets.get`,
  `secrets.revoke`, `secrets.restore`, `secrets.promote` y `secrets.destroy` (con `proof`). Las que cambian algo
  llevan `expected_version` del secreto (FR-017). La CLI lee el valor con `getpass` o desde un fichero (`--from-file`,
  para PEM), nunca como argumento de la línea de órdenes.
- **Rationale:** sigue el canal local y el sobre común ya aprobados (constitución, principio II); leer de la línea de
  órdenes dejaría el valor en el historial del shell.

## R9. Registro de accesos con quién, ruta y versión

- **Decision:** el registro de accesos gana tres columnas opcionales: `subject` (identidad o `admin`), `path` y
  `version`, y operaciones nuevas (`secret_create`, `secret_add_version`, `secret_list`, `secret_get`, `secret_revoke`, `secret_restore`,
  `secret_promote`, `secret_destroy`, `secret_resolve`). Una base de datos de la feature 001 se migra al arrancar
  añadiendo las columnas que falten.
- **Rationale:** FR-016 exige "quién, qué ruta y qué versión"; columnas opcionales mantienen las filas antiguas.

## R10. Recuentos de caducidad en el estado

- **Decision:** `status` añade `secrets_expired` y `secrets_expiring_soon` (próximos 7 días), calculados con los
  metadatos, que no están cifrados, así que funcionan también con Hécate sellada.
- **Rationale:** FR-022; el cronjob de Hades consultará estos recuentos.

## R11. Validación de valores

- **Decision:** `string` es texto UTF-8; `json` debe analizarse como JSON; `pem` debe contener uno o más bloques
  `-----BEGIN X-----` / `-----END X-----` con base64 válido. Máximo 64 KiB.
- **Rationale:** FR-002 y la suposición de tamaño de la spec; no hacen falta dependencias nuevas.

## R12. Sin tecnología nueva

- **Decision:** HKDF viene de `cryptography` (ya en uso) y HMAC, de la librería estándar; el resto es librería estándar. No hace falta ADR.
