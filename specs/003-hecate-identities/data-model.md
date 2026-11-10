# Data Model: Identidades de Hécate

Modelo de la feature 003 sobre el de las features 001 y 002 (cambio 005). Event Sourcing sin CQRS, como fija
ARCHITECTURE para Hécate: el estado se reconstruye leyendo cada stream ([research R10](research.md#r10-almacenamiento-y-event-sourcing)).

## Valores

| Valor | Regla | Origen |
|---|---|---|
| `TrustDomain` | De 1 a 255 caracteres entre minúsculas, dígitos, `.`, `-` y `_` (SPIFFE) | Suposiciones de la spec |
| `IdentityName` | `[a-z][a-z0-9-]{0,62}`, la regla del primer segmento de las rutas de secretos. `hecate` y `admin` están reservados | Suposiciones, FR-007 |
| `SpiffeId` | `spiffe://<trust domain>/<identidad>` | FR-009 |
| `Serial` | 128 bits aleatorios, positivo, en hexadecimal en minúsculas | R2 |
| `RootFingerprint` | `sha256:` y 64 caracteres hexadecimales del SHA-256 del DER de la CA raíz | R2 |
| `TokenId` | 8 bytes aleatorios en hexadecimal | R6 |
| `EnrollmentToken` | `HET1.<identidad>.<id>.<secreto>.<huella>`; el secreto, 32 bytes en base64url sin relleno | R6, [contracts/formats.md](contracts/formats.md) |
| `CallKey` | Clave pública Ed25519; su `kid` es la huella de RFC 7638 en base64url | R5 |
| Frase de la CA | `Passphrase` de 12 caracteres o más, distinta de la de desbloqueo | FR-002 |

## Aggregates

### Key de la CA (`ca`)

| Evento | Datos | Efecto |
|---|---|---|
| `CaCreated` | `generation` (1), `trust_domain`, `root_fingerprint`, `root_not_after`, `intermediate_version` (1), `intermediate_not_after` | Nace la CA con su primera intermedia `active` |
| `IntermediateRenewed` | `generation`, `intermediate_version`, `intermediate_not_after` | La nueva pasa a `active` y la anterior a `verify_only` |
| `IntermediateRetired` | `generation`, `intermediate_version`, `reason` (`expired_certificates`, `ca_replaced`) | La intermedia pasa a `retired` y su clave se ha destruido |
| `CaReplaced` | `generation` (siguiente), `root_fingerprint`, `root_not_after`, `intermediate_version` (1), `intermediate_not_after` | Nueva generación; la anterior entera deja de valer |

- Stream `ca`, fuera del espacio de nombres de las identidades: `key/<identidad>` es el de la clave de ámbito de cada
  identidad (feature 002) y `ca` es un nombre de identidad válido.
- Una sola CA: `CaCreated` solo puede ser el primer evento (FR-001).
- Estados de cada intermedia, con los nombres del aggregate Key del doc de Hécate:
  - `active` emite;
  - `verify_only` ya no emite y solo firma la CRL de sus certificados;
  - `retired` ya no tiene clave.
- Una intermedia `verify_only` pasa a `retired` cuando caduca el último certificado que firmó (como mucho, 3 días).
  Al sustituir la CA, todas pasan a `retired` en la misma transacción (FR-005, FR-006).

### Identity (`identity/<nombre>`)

| Evento | Datos |
|---|---|
| `EnrollmentTokenCreated` | `token_id`, `secret_sha256`, `generation`, `expires_at` |
| `EnrollmentTokenAnnulled` | `token_id` |
| `IdentityEnrolled` | `token_id`, `serial` |
| `IdentityDisabled` | `urgent` |
| `IdentityEnabled` | — |
| `SuspiciousRenewalDetected` | `serial` |

- **Estado:**
  - sin alta: solo hay tokens; no aparece en `identities.list`, y `identities.get`, `identities.disable` e
    `identities.enable` dan `not_found`;
  - `enrolled`: dada de alta y habilitada;
  - `disabled`.

  Nace con su primera alta (Key Entities de la spec).
- **Época:** sube con cada `IdentityEnrolled` y con cada `IdentityDisabled`. Un certificado solo se renueva si su época
  es la de la identidad y la identidad está habilitada. Así una nueva alta (FR-013) o deshabilitar y volver a habilitar
  (FR-019) dejan sin renovación los certificados anteriores, sin escribir en cada uno.
- **Token de alta:** está pendiente si cumple todo esto:
  - no se ha usado (ningún `IdentityEnrolled` con su `token_id`) ni anulado;
  - no ha caducado;
  - es de la generación de CA vigente;
  - se creó después del último `IdentityDisabled`.

  Si no, está usado, anulado o caducado. Solo la emisión de un certificado lo gasta (FR-011).
- **Reglas:**
  - No se crea un token para una identidad reservada, mal formada o deshabilitada (FR-007, FR-019).
  - `SuspiciousRenewalDetected` se escribe con la revocación urgente de todos los certificados válidos de la
    identidad, en la misma transacción (FR-015).

### Certificate (`certificate/<identidad>/<serie>`)

| Evento | Datos |
|---|---|
| `CertificateIssued` | `identity`, `serial`, `generation`, `intermediate_version`, `epoch`, `not_before`, `not_after`, `call_key` (`x`, `kid`; `null` en los de Hécate), `origin` (`enrollment`, `renewal`, `self`), `renewed_from`, `request_sha256`, `idempotency_key`, `certificate` (DER en base64) |
| `CertificateRenewed` | `new_serial` |
| `CertificateRevoked` | `urgent`, `by` (`admin`, `hecate`), `reason` (`admin`, `suspicious_renewal`) |

- **Estado:**
  - `revoked` si hay una revocación urgente;
  - `expired` si pasó `not_after`;
  - si no, `valid`.

  Un certificado de otra generación de CA no vale en ninguna operación (FR-021).
- **Se puede renovar** si cumple todo esto:
  - es `valid` y de la generación vigente;
  - no tiene `CertificateRenewed` ni `CertificateRevoked`;
  - su época es la de la identidad, y la identidad está habilitada.

  La revocación pasiva solo quita la renovación (FR-018).
- **Su clave de firma de llamadas vale para llamadas** si el certificado es `valid` y la identidad está habilitada.
  Después, solo para verificar firmas pasadas (FR-017).
- **Certificados de Hécate:** son de la identidad `hecate`, `origin: self` y sin clave de firma de llamadas. Solo
  admiten la revocación urgente, que Hécate sigue al momento con un certificado nuevo (FR-012).
- **Repetición:** `request_sha256` e `idempotency_key` reconocen una repetición y el uso indebido de la
  `Idempotency-Key`, dentro de la identidad (R12).

## Tablas fuera de los eventos

| Tabla | Columnas | Ciclo de vida |
|---|---|---|
| `ca_material` | `generation`, `intermediate_version` (clave), `root_certificate`, `intermediate_certificate`, `key_nonce`, `key_ciphertext` | Se escribe al crear, renovar o sustituir. La fila se borra al retirar la intermedia (borrado criptográfico, con `secure_delete`) |
| `hecate_tls` | `id` (1), `serial`, `private_key` (PKCS#8 en claro) | Se sustituye en cada certificado nuevo de Hécate |
| `enroll_attempts` | `id` (1), `consecutive_failures`, `wait_until`, `rejected_during_wait` | Freno de altas (FR-034); sobrevive a los reinicios |
| `access_log` | Las de 001 y 002 más `identity` y `certificate`, que admiten nulos | Append-only, como siempre |

- La clave de cada intermedia se cifra bajo la raíz de cifrado con AES-256-GCM. Sus datos asociados, en JSON con las
  claves ordenadas como los de 002, son `{"generation": g, "purpose": "ca_intermediate", "root": "<huella>",
  "version": v}`.
- La base se migra como en el cambio 005: al abrirla, las tablas nuevas se crean con `CREATE TABLE IF NOT EXISTS` y
  las dos columnas entran en la lista de columnas que le faltan al registro de accesos. Es idempotente y tolera dos
  conexiones a la vez.

## Estado en memoria

| Qué | Cuándo existe |
|---|---|
| Clave de una intermedia, descifrada con la raíz de cifrado | Solo durante la operación que firma, con Hécate desbloqueada (FR-028) |
| Ceremonia pendiente: tipo, `ceremony_id`, clave de la intermedia nueva, solicitud, caducidad (15 minutos) | Desde `*.begin` hasta `*.confirm`, `*.abort`, la caducidad o el reinicio |
| Parejas `iss` y `jti` vistas | Hasta que caduca cada token corto |
| Instante de arranque | Desde que arranca el proceso: rechaza tokens cortos anteriores |
| Configuración TLS: certificado propio y la CA raíz vigente como pista para el cliente | Mientras el certificado propio está vigente, también sellada; sin él, la API no escucha (R8) |
| Verificador de certificados de cliente de la aplicación, con la CA raíz vigente | Desde que existe la CA; se sustituye al sustituirla (R1) |

## Registro de accesos

| Operación | Método | Quién (`subject`) | `identity` | `certificate` |
|---|---|---|---|---|
| `ca_create`, `ca_intermediate_renew`, `ca_replace` (con su `abort` y su motivo) | `local` | `admin` | — | — |
| `token_create`, `token_annul`, `token_list` | `local` | `admin` | La del token | — |
| `identity_list`, `identity_get`, `identity_disable`, `identity_enable` | `local` | `admin` | La consultada o cambiada | — |
| `certificate_revoke` | `local` | `admin` | La del certificado | La serie |
| `identity_enroll` | `enrollment_token` | El SPIFFE ID de la identidad del token, o nadie si el token no se reconoce | Sí | La serie emitida |
| `certificate_renew` | `identity` | El SPIFFE ID de la conexión | Sí | La presentada y la emitida (en `detail`) |
| `certificate_revoke` automática (FR-015) y `certificate_renew` del certificado propio de Hécate | `internal` | `hecate` | Sí | La serie |
| `key_list`, `key_get`, `revocation_list`, `capabilities_get` | `identity` | El SPIFFE ID, o nadie | Si se conoce | — |
| `identity_enroll_throttled`, una por tanda (FR-034) | `internal` | — | — | — |

- Las operaciones siguen el estilo del cambio 005: snake_case y en singular. Los métodos `enrollment_token` (el alta) e
  `internal` (lo que Hécate hace por su cuenta) son nuevos.

- La repetición exacta de un alta deja una fila con `detail: repeat`, aunque no tenga efecto.
- Una deshabilitación urgente deja además una fila `certificate_revoke` del admin por cada certificado que revoca, con
  su serie.
- Las consultas atendidas de `keys` y `revocations` no dejan fila; sus rechazos sí (FR-024).
- Una petición de un componente que la API rechaza antes de su caso de uso deja la fila de su operación, sin `subject`,
  identidad ni certificado, porque no se ha autenticado a nadie (FR-024):
  - con Hécate sellada, `rejected_conflict` con `detail: sealed`, como en el canal local, sin contar en el freno de
    altas;
  - con un `X-Request-Id` que no sigue su regla, `invalid_input` con `detail: request_id`. En un alta pasa además por
    el freno, como un `Idempotency-Key` mal formado: cuenta como fallo y, con las altas frenadas, responde
    `rate_limited` y cuenta en la tanda, sin fila propia (FR-034).

  Un `HEAD` a una ruta `GET` es esa operación, porque axum lo atiende con ella. Una petición que no es una operación
  (otra ruta u otro método) no deja fila.
- Una petición que no es JSON estándar no deja fila (FR-029).
- Nunca un token, una clave privada ni la frase de la CA.

## Transiciones

```text
Intermedia:   active ──renovar──▶ verify_only ──caduca su último certificado──▶ retired
              active, verify_only ──sustituir la CA──▶ retired
Identidad:    sin alta ──alta──▶ enrolled ──deshabilitar──▶ disabled ──habilitar──▶ enrolled (necesita un token nuevo)
Token:        pending ──emisión──▶ used        pending ──admin, deshabilitar o sustituir la CA──▶ anulado
              pending ──15 minutos──▶ caducado
Certificado:  valid (renovable) ──renovar, revocación pasiva, alta nueva, deshabilitar──▶ valid (sin renovación)
              valid ──revocación urgente──▶ revoked        valid ──not_after──▶ expired
```

Repetir una orden sobre algo que ya está en ese estado no escribe eventos (FR-026). La urgencia sí cambia: con
`urgent`, una identidad deshabilitada sin urgencia revoca con urgencia sus certificados válidos, y un certificado con
revocación pasiva recibe otro `CertificateRevoked` con `urgent`.

## Validaciones y requisitos

| Regla | Requisito |
|---|---|
| Trust domain y nombre de identidad; identidades reservadas | FR-007, FR-029, Suposiciones |
| Frase de la CA de 12 caracteres o más y distinta de la de desbloqueo | FR-002 |
| Solicitud en PEM estricto, P-256, firma válida y sin otra identidad | FR-008, FR-010, FR-029 |
| Clave de firma de llamadas Ed25519 en PEM, distinta de la del certificado y de la actual al renovar | FR-008, FR-014 |
| Token corto ligado al certificado, de 300 s como mucho y una sola vez | FR-016 |
| Una sola renovación por certificado; la segunda revoca la identidad | FR-015 |
| Certificado de 3 días como mucho y nunca después de su intermedia | FR-009 |
| Rutas de secretos de `hecate` y `admin` rechazadas | FR-027 |

## Estado para `status`

Se calcula con los eventos, también sellada (FR-025): si existe la CA (con trust domain y huella), la serie y la
caducidad del certificado vigente de Hécate, los días que le quedan a la intermedia cuando son menos de 30 (0 si ya
caducó), cuántas identidades habilitadas tienen su certificado más reciente a menos de 24 horas de caducar, cuántos
`SuspiciousRenewalDetected` hubo en 24 horas y si las altas están frenadas.
