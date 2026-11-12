# Research: Access Policy de Hécate y resolución de secretos

Decisiones técnicas de la feature 006. Cada una dice qué se elige, por qué y qué se descartó. Parte de la Hécate que
deja la feature 003: su API mTLS, su autenticación por certificado y token corto, su componente de prueba y la
resolución interna de la feature 002.

## R0. Decisiones del propietario

- **Clarify (2026-11-10):**
  - el estado responde también con Hécate sellada (FR-012);
  - una excepción nombra un secreto concreto que exista al concederla (FR-006);
  - los cambios de política son órdenes sin versión esperada (FR-008);
  - solo dejan fila las consultas del estado rechazadas (FR-015).
- **Plan (2026-11-10):**
  - `rotate_every` pasa a ser una duración ISO 8601 que se valida al guardar (FR-023, R5);
  - los rechazos de la resolución se igualan comprobando todas las referencias antes de descifrar y respondiendo con
    un tiempo mínimo (R2).

## R1. Resolución por la API

**Decisión:** `POST /v1/secrets:resolve`, el nombre que ya da docs/hecate.md, con la autenticación de las consultas de
la feature 003: certificado de cliente válido de la CA vigente y token corto (`Authorization: Bearer`).

- **Cuerpo:** `{ "references": ["secret://hermes/telegram/bot_token", "secret://hermes/smtp/password@3"] }`, de 1 a
  100 referencias, con la regla de rutas de la feature 002 y `@N` opcional (N de 1 a 9 cifras, sin ceros a la
  izquierda).
- **Respuesta:** los secretos en el orden pedido, cada uno con su referencia, su versión, su valor y sus dos tiempos
  de caché (300 s blando y 3600 s duro, los de `SecretsSettings`). Contrato en [api.md](contracts/api.md).
- **Sin `Idempotency-Key`:** resolver no cambia nada salvo el registro de accesos; es una consulta con cuerpo.
- **Sin límite de tamaño aparte:** con 100 referencias de hasta 64 KiB, una respuesta ronda como mucho los 6,5 MiB.
  Bastan esos dos límites (decidido al revisar la checklist).
- **Dónde vive:** las operaciones de la API siguen en `IdentityService`, que ya autentica y registra las llamadas de
  los componentes (el patrón de `queried`). `resolve` autentica, lee la política de la identidad en
  `AccessPolicyService` y delega el descifrado en `SecretsService`, que conserva las reglas de versiones de la 002.
- **Permiso:** una referencia se puede leer si su dueño (el primer segmento de la ruta) es la identidad que llama o si
  su política tiene una excepción para esa ruta exacta (FR-005, FR-006). La política se lee una vez por petición: la
  resolución usa la de antes o la de después de un cambio, nunca una mezcla.
- **Valores:** el caso de uso ya los entrega como `Zeroizing<String>` (texto UTF-8, sea cual sea su tipo); el resto del
  camino, en R6.

**Alternativas:** un servicio nuevo de API que reuniera identidades, secretos y políticas (habría que sacar la
autenticación de `IdentityService` sin ganar nada); poner la comprobación de permisos dentro de `SecretsService` (los
secretos no deben conocer las políticas ni las identidades).

## R2. Tiempos de los rechazos

**Decisión:** comprobar todas las referencias antes de descifrar ninguna y responder todo `not_found` de la resolución
con un tiempo mínimo fijo.

- **Dos fases:** primero se comprueba cada referencia, en el orden pedido: permiso, existencia, versión resoluble y
  material guardado. Solo si todas pasan se descifran. Hoy la resolución falla en la primera referencia mala y ya ha
  descifrado las anteriores, y la de otra identidad se rechaza antes de cargar nada: las dos cosas se ven en el tiempo.
- **Tiempo mínimo:** un rechazo `not_found` se responde no antes de 250 ms desde que la petición llega a su manejador.
  La espera es asíncrona en la API (`tokio::time::sleep_until`) y no ocupa un hilo. Es un ajuste de
  `IdentitySettings` (250 ms por defecto) para que las pruebas usen uno menor.
- **Por qué 250 ms:** la primera fase con 100 referencias (cargar 100 streams y la política) tiene que quedar por
  debajo de una quinta parte (50 ms); una prueba sobre SQLite lo comprueba en macOS y en el contenedor Linux con la
  mediana de 20 mediciones. Si alguna vez la fase tarda más, la respuesta llega más tarde: el tiempo es un mínimo.
- **Solo los tres casos:** los rechazos `auth`, `validation` o por estar sellada no llevan espera, porque no dicen nada
  de un secreto. Las resoluciones atendidas tampoco.
- **Nada en la respuesta lo delata:** la duración que indica el sobre (`meta.duration_ms`) se calcula después de la
  espera, así que refleja el tiempo mínimo y no el de proceso (FR-004).
- **Cómo se prueba (SC-003):** 1.000 peticiones de cada grupo por la API con el componente de prueba, en su propio
  fichero de pruebas para que ninguna otra corra a la vez. Los grupos son los tres tipos con una referencia y las
  peticiones de 10 referencias en las que falla la primera o la última, siempre por un secreto sin versión resoluble;
  se intercalan en orden aleatorio con una semilla fija, con 8 peticiones a la vez y un tiempo mínimo de prueba de
  50 ms, por encima de lo que tardan antes de la espera. La diferencia entre las medianas de dos tipos, o entre falla
  la primera y falla la última, no puede superar la mayor de las diferencias entre las dos mitades, en orden de envío,
  de los dos grupos comparados, más un margen de 1 ms. El caso de 100 referencias lo cubre la prueba de la primera
  fase.

**Alternativas:** hacer el mismo trabajo en cada caso, con descifrados falsos (cualquier rama distinta vuelve a filtrar
tiempo y es frágil de mantener); un retraso aleatorio (se promedia con muchas muestras y no oculta la diferencia).

## R3. Access Policy

**Decisión:** un aggregate event-sourced por identidad, en el stream `policy/<identidad>`, como fija ARCHITECTURE para
los aggregates de Hécate.

| Evento | Datos |
|---|---|
| `ReadGranted` | `secret` (la ruta), `by` (`admin`) |
| `ReadRevoked` | `secret`, `by` |
| `StatusGranted` | `by` |
| `StatusRevoked` | `by` |

- El estado son las rutas que puede leer además de las suyas y si tiene el permiso de estado. La versión es la del
  stream; el historial, sus eventos con su instante. Reconstruir qué podía leer en un momento pasado es aplicar los
  eventos hasta ese instante (SC-006).
- **Reglas al conceder una lectura:** la identidad sigue la regla de nombres y no es reservada; la ruta sigue la regla
  de la 002; el secreto existe; su dueño no es la propia identidad (FR-006). El permiso de estado tampoco se concede a
  las reservadas (FR-007). La identidad no tiene que estar dada de alta: su política se prepara antes, como sus
  secretos.
- **Órdenes sin versión esperada (FR-008):** cada cambio escribe en una transacción con la versión que lee, y si otro
  se adelantó vuelve a leer, como las órdenes de la feature 003. Conceder lo que ya existe o retirar lo que no existe no
  escribe eventos.
- **Sin regla por defecto guardada:** una identidad sin stream solo lee lo suyo.
- **Nombre del stream:** `policy/` no choca con `secret/`, `key/`, `ca`, `identity/` ni `certificate/`.

**Alternativas:** una tabla de permisos fuera del event store (perdería el historial con quién, que pide FR-008); una
política global para todas las identidades (un cambio en una identidad versionaría todas).

## R4. Ámbito `status`

**Decisión:** `GET /v1/status`, con certificado y token corto, para las identidades con el permiso de estado.

- **Campos:** `sealed`, `secrets_expired`, `secrets_expiring_soon` (7 días), `secrets_due_rotation`,
  `identities_expiring_24h`, `suspicious_renewals_24h`, `intermediate_days_left` (solo si quedan menos de 30) y
  `enrollment_throttled`, el mismo nombre que en el `status` del canal local. Ninguna ruta, nombre de identidad ni
  metadato de un secreto concreto (FR-011).
- **También sellada (FR-012):** la guarda de la API pregunta por cada petición si hay que rechazarla por estar sellada,
  y la ruta del estado no se rechaza. La autenticación funciona sellada: los certificados de la CA, las claves de firma
  de llamadas y las políticas no están cifrados. Los recuentos tampoco tocan material cifrado.
- **Filas (FR-015):** una consulta atendida no deja fila; un rechazo (sin permiso, `auth`) sí, con la operación
  `status_get`.
- El `status` del canal local gana `secrets_due_rotation` (FR-013).

**Alternativas:** que el componente vea el mismo `status` que el admin (lleva la huella de la CA raíz y la serie del
certificado de Hécate, que no necesita); responder `unavailable` sellada (descartado en clarify).

## R5. `rotate_every` como duración

**Decisión:** una duración ISO 8601 en días o en semanas, `P<n>D` o `P<n>W`, con n de 1 a 3650 días (520 semanas). La
lee un tipo propio del dominio (`RotationInterval`), sin crates nuevos.

- `secrets.create` la valida al guardar y rechaza otro texto con `validation`: «rotate_every must be an ISO 8601
  duration in days or weeks, such as P90D or P12W» (FR-023).
- Se guarda como texto, igual que ahora, y los eventos no cambian. Un valor antiguo que no es una duración se sigue
  leyendo y mostrando, y no cuenta.
- **Debe rotarse:** un secreto con una versión `active` cuya fecha de creación más el intervalo ya ha llegado; en el
  instante exacto ya cuenta (decidido al revisar la checklist).
- Las pruebas de la 002 que usan `"90d"` pasan a `"P90D"`: es el cambio a propósito de FR-020.

**Alternativas:** todas las duraciones ISO 8601 con años y meses (necesitan aritmética de calendario, y un mes no tiene
una duración fija); seguir con `90d` (no es un formato estándar, en contra del FR-018 del 005); un crate de duraciones
para una gramática de dos formas.

## R6. Memoria de los valores resueltos

**Decisión:** el valor no sale de tipos que se borran hasta que se envía.

- El caso de uso entrega `Zeroizing<String>` (ya lo hace) y no se copia a un `serde_json::Value`.
- La respuesta se serializa desde una estructura propia a un `Zeroizing<Vec<u8>>` de tamaño exacto: una pasada cuenta
  los bytes y otra escribe, así no hay reubicaciones que dejen copias.
- El cuerpo va a hyper como `Bytes::from_owner(buffer)` (`axum::body::Bytes`, bytes 1.12): cuando hyper suelta la
  última referencia, después de escribirla, el búfer se borra.
- Las copias que hacen rustls al cifrar el registro TLS y el núcleo en el socket quedan en el riesgo residual ya
  aceptado (lo interno de las librerías y los búferes del sistema operativo).
- Lo comprueban una prueba de memoria, como en el cambio 005, y la prueba de fugas de la 003 ampliada con la resolución.

**Alternativas:** el sobre actual con `serde_json::to_vec` (deja el valor en un `Vec` que no se borra y en el árbol del
`Value`).

## R7. Canal local y CLI

**Decisión:** seis operaciones `policies.*` del admin, con su orden en la CLI. Todas exigen Hécate desbloqueada
(FR-014) y dejan fila con `subject: admin`.

| `op` | Orden |
|---|---|
| `policies.grant_read` | `hecate policies grant-read <identidad> <secreto>` |
| `policies.revoke_read` | `hecate policies revoke-read <identidad> <secreto>` |
| `policies.grant_status` | `hecate policies grant-status <identidad>` |
| `policies.revoke_status` | `hecate policies revoke-status <identidad>` |
| `policies.list` | `hecate policies list` |
| `policies.get` | `hecate policies get <identidad>` |

- `policies.get` de una identidad sin política devuelve la política vacía (versión 0), porque vale la regla por
  defecto; no es un error.
- `hecate status` añade la línea `secrets due rotation: <n>`.
- Contratos en [control-protocol.md](contracts/control-protocol.md) y [cli.md](contracts/cli.md).

**Alternativas:** una sola operación `policies.change` con el tipo de cambio en los argumentos (peor para la CLI y las
filas del registro).

## R8. Registro de accesos

**Decisión:**

| Operación | Método | `subject` | `identity` | `certificate` | Ruta y versión |
|---|---|---|---|---|---|
| `secret_resolve` | `identity` | El SPIFFE ID de la conexión | La que llama | La serie presentada | Atendida: una fila por referencia, con su ruta y su versión. Rechazada: una fila con la primera referencia que falla |
| `status_get` | `identity` | El SPIFFE ID, o nadie | Si se conoce | La serie, si se conoce | — (solo rechazos) |
| `policy_grant`, `policy_revoke` | `local` | `admin` | La de la política | — | La ruta en las lecturas; `detail` `read` o `status` |
| `policy_list`, `policy_get` | `local` | `admin` | La consultada | — | — |

- Una resolución rechazada antes del caso de uso (sellada, `X-Request-Id` mal formado) deja su fila con el registrador
  de la feature 003: `operation_of` reconoce `POST /v1/secrets:resolve` y `GET /v1/status`. El estado sellado no se
  rechaza, así que no pasa por ahí.
- Nunca un valor, una huella, una clave privada ni un token.

## R9. Contrato

**Decisión:**

- `hecate.v1.json` gana `resolveSecrets` (`POST /v1/secrets:resolve`: 200, 400, 401, 404, 500 y 503) y `getStatus`
  (`GET /v1/status`: 200, 400, 401 y 500), con sus esquemas. `GET /v1/capabilities` añade `secrets:resolve` y `status`.
- El `not_found` de la resolución usa el `NotFoundFailure` que ya existe, con el mismo mensaje en los tres casos.
- Pruebas de contrato de la API, que validan cada respuesta, también las de error; y de las operaciones nuevas del
  canal (`contract_policy_ops.rs`), con el contrato de `status` ampliado.

## R10. Pruebas

**Decisión:** TDD en cada nivel, como en la feature 003.

- **Dominio:** la Access Policy (conceder, retirar, repetir, permiso de lectura y de estado) y `RotationInterval` (las
  dos formas, los límites y lo que se rechaza).
- **Aplicación:** resolver secretos propios, compartidos y denegados con sus versiones; que un rechazo no descifra
  nada; las filas; las órdenes de política con su historial; el estado y sus recuentos, también sellada;
  `rotate_every` en `secrets.create`; un corte de luz durante un cambio de política; la memoria.
- **API:** el componente de prueba gana `resolve` y `status`; la resolución, el estado, Hécate sellada, `auth`,
  `validation` y las filas, validados contra el OpenAPI; el rendimiento con 10 secretos (SC-001); la prueba de fugas
  ampliada.
- **Tiempos (R2):** la prueba de SC-003 por la API, en su propio fichero, y la de la primera fase con 100 referencias
  sobre SQLite.
- **CLI:** las órdenes `policies` y la línea nueva de `status`.
- La suite completa, también en el contenedor Linux (FR-021).

## R11. Consolidación al cerrar

**Decisión:** lo que lista FR-022, revisado contra los docs al cerrar:

- `docs/hecate.md`: la resolución por el canal (apartado 4 y la ruta `/v1/secrets:resolve` de su diagrama), la Access
  Policy (apartados 3 y 9), las operaciones nuevas (apartados 4, 5 y 9), el estado para los componentes (apartados 5 y
  10) y `rotate_every` como duración (apartado 6);
- GLOSSARY: la entrada Access Policy;
- `shared/contracts`: el OpenAPI;
- ARCHITECTURE, si algo lo contradice;
- DECISIONS: la decisión abierta sale de «Abiertas» y pasa al ADR 0010, que recoge el modelo de la política, la regla
  por defecto, las excepciones por secreto, el permiso de estado y la igualdad de los rechazos;
- DECISIONS, desviaciones de Hécate frente a OpenBao / Vault (principio VII): excepciones por secreto concreto, sin
  comodines ni políticas ligadas a tokens.

## R12. OpenTelemetry

**Decisión:** sigue la excepción de las features anteriores hasta que DECISIONS cierre el colector y el backend. Lo que
pasa queda en el registro de accesos y en los logs estructurados.

## R13. Dependencias

**Decisión:** ninguna nueva. `axum::body::Bytes` es `bytes` 1.12, que ya tiene `from_owner`, y la duración la lee
código propio. `deny.toml` no cambia.
