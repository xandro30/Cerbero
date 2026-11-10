# Research: Identidades de Hécate

Decisiones técnicas del plan de la feature 003, sobre la Hécate en Rust del cambio 005. Cada una con su decisión, su
porqué y las alternativas. Las versiones son las últimas estables a 2026-11-09 (regla del ADR 0007).

## R0. Decisiones del propietario

Durante el plan (2026-11-09), con la opción recomendada en las seis:

| Tema | Decisión | Dónde |
|---|---|---|
| Stack HTTP de la API | axum sobre hyper, con tokio-rustls | R1, ADR 0009 |
| Token corto | jsonwebtoken 11 con aws-lc-rs | R3, ADR 0009 |
| Lista de revocación | CRL X.509 firmada por la intermedia | R4, ADR 0009 |
| Salud | Solo por el canal local (`hecate status`) | R17 |
| Copia de la clave de la CA raíz | Sobre propio (Argon2id + AES-256-GCM, PEM) | R7, ADR 0009 |
| Clave del certificado propio de Hécate | En SQLite, en una tabla aparte, en claro | R8 |

Las del clarify (spec, sesión 2026-11-09) que fijan el diseño: la clave de la CA raíz solo en la CLI (R9), sin
OpenTelemetry (R18), sin fila para lo que no es JSON estándar (R11), el token de alta solo se gasta al emitir (R6),
pruebas en Linux (R16), solo revocación urgente del certificado de Hécate (R8), confirmación de la copia en la ceremonia
(R7, R9), refrescos sin fila (R11), freno de altas (R13) y destrucción de la clave de la intermedia anterior (R2, R10).

## R1. Stack HTTP y TLS de la API

**Decisión:** axum 0.8.9 sobre hyper 1.12.0 y hyper-util 0.1.21 (`server-auto`: HTTP/1.1 y HTTP/2 por ALPN), con
tokio-rustls 0.26.6 y rustls 0.23.46 con el proveedor aws-lc-rs: solo TLS 1.3. La versión se fija en la configuración
(`builder_with_protocol_versions`) y el binario no compila la feature `tls12`. Hay que fijarla aunque falte la feature:
las pruebas activan `tls12` para su cliente TLS 1.2 y Cargo unifica las features.

- **Post-cuántico:** rustls con `prefer-post-quantum`. El intercambio de claves prefiere el híbrido X25519MLKEM768
  (ML-KEM de aws-lc-rs) frente a X25519, así que el tráfico grabado hoy queda protegido frente a un descifrado cuántico
  futuro, a cambio de unos bytes más en cada handshake.
- **axum sin features por defecto:** basta con el router y los extractores; el JSON lo lee un extractor propio (R11).
- **Bucle de aceptación propio:** `TcpListener` de tokio → `TlsAcceptor` → certificado del cliente de la conexión
  (`peer_certificates`) → extensión de la petición → router de axum servido con `hyper_util::server::conn::auto` y
  `TowerToHyperService`. Así cada manejador recibe el certificado con el que se abrió la conexión (principio V: ligar el
  token corto al canal).
- **Certificado del cliente, validado en la aplicación:**
  - La TLS usa un verificador propio que pide certificado sin exigirlo, anuncia la CA raíz vigente
    (`root_hint_subjects`) y solo comprueba la firma del handshake (`rustls::crypto::verify_tls13_signature`): que el
    cliente tiene la clave del certificado que presenta. No valida la cadena.
  - La aplicación exige el certificado en todo salvo en el alta. Valida la cadena hasta la CA raíz vigente y la
    caducidad con el `WebPkiClientVerifier` de rustls, llamando a `verify_client_cert` con la hora del port `Clock`, y
    después comprueba que lo emitió Hécate, la generación, el SPIFFE ID, la revocación y la identidad (FR-014, FR-016,
    FR-021).
  - Así cada certificado que no vale (caducado, de otra CA o de la generación anterior) recibe `auth` y deja fila
    (SC-006, FR-024), y las pruebas controlan la hora con `FakeClock`.
  - Un handshake que falla (firma que no corresponde, cliente sin TLS 1.3 o saludo mal formado) no llega a la
    aplicación: no tiene respuesta ni fila, y deja una línea en los logs estructurados con el motivo y la dirección de
    origen.
- **Rotación sin reiniciar:** un supervisor de la API, en infraestructura, lee la CA y el certificado propio y arranca,
  para o sustituye la configuración entera del servidor, guardada en un `RwLock` de tokio que se lee al aceptar cada
  conexión. Lo llaman `serve` al arrancar, la confirmación de la ceremonia, el desbloqueo y la tarea de cada minuto
  (R8). La aplicación no sabe nada de la API, y no hace falta un `ResolvesServerCert` aparte ni otra crate.
- **Límites:** cuerpo de hasta 64 KiB (`DefaultBodyLimit`) y 10 s para leer las cabeceras.
- **Proceso:** la API corre en el runtime de `hecate serve`, junto al canal local. Escucha en cuanto existe la CA, al
  arrancar o al confirmar la ceremonia, también sellada. Sin CA no escucha (caso límite de la spec), y deja de escuchar
  si su certificado propio caduca sin poder renovarse, hasta tener uno nuevo (R8).

**Alternativas:**
- Para el servidor: hyper sin framework (enrutado, errores y extracción a mano); actix-web (su propio runtime y su
  propia integración TLS, menos alineada con tokio); poem o warp (menos adoptadas).
- Para el certificado del cliente: `WebPkiClientVerifier` en la TLS, con un envoltorio que registre los rechazos. El
  rechazo llegaría como fallo de conexión y no como `auth`, y la fila no llevaría la operación.

## R2. PKI: emisión, lectura y perfiles

**Decisión:** rcgen 0.14.10 (`aws_lc_rs`, `pem`, `x509-parser` y `zeroize`) para emitir certificados y CRL, y
x509-parser 0.18.1 (`verify-aws`) para leer certificados y solicitudes y verificar sus firmas.

| Certificado | Perfil |
|---|---|
| CA raíz | Autofirmada, `CA:true` con `pathLen 1`, `keyCertSign` y `cRLSign`, 10 años, CN `Cerbero Root CA` y el trust domain |
| Intermedia | Firmada por la CA raíz, `CA:true` con `pathLen 0`, `keyCertSign` y `cRLSign`, 1 año sin pasar de la CA raíz |
| Componente y Hécate | `CA:false`, un único SAN URI (`spiffe://<trust domain>/<identidad>`), `digitalSignature`, `serverAuth` y `clientAuth` (sirve para llamar y para recibir llamadas, FR-009), CN con el nombre de la identidad |

- **Algoritmos:** ECDSA P-256 con SHA-256 para la CA y para los certificados de los componentes y de Hécate, como fija
  el doc de Hécate. Una solicitud con otra clave se rechaza con `validation`.
- **Validez:** `notBefore` un minuto antes de emitir, para absorber pequeños desfases; `notAfter` al cumplirse la vida
  configurada (3 días por defecto) y nunca después de la intermedia (FR-009).
- **Número de serie:** 128 bits aleatorios, positivo, mostrado en hexadecimal.
- **Solicitud:** PEM estricto (el lector de RFC 7468 del cambio 005) → DER → x509-parser, con su firma verificada. Si
  pide un SAN, tiene que ser exactamente el SPIFFE ID que decide Hécate (FR-010); si pide otro, `validation`. El resto de
  la solicitud (sujeto y extensiones) se ignora: el perfil lo pone Hécate.
- **Huella de la CA raíz:** SHA-256 del DER del certificado, mostrada como `sha256:<64 hex>`.
- **Sin name constraints** en la intermedia: webpki no admite restricciones de nombre de tipo URI, y una restricción
  crítica que no entiende invalida la cadena.
- **Clave de la intermedia anterior** (clarify 2026-11-09): al renovar, pasa a firmar solo la CRL de sus certificados
  y se destruye cuando caduca el último (como mucho, 3 días). Al sustituir la CA, se destruye al momento (R10).

**Alternativas:** x509-cert de RustCrypto (firma con RustCrypto, una segunda pila criptográfica); el crate `openssl`
(descartado en el ADR 0007).

## R3. Token corto de las llamadas

**Decisión:** jsonwebtoken 11.1.0 sin features por defecto y con `aws_lc_rs`, solo EdDSA.

- **Claims:**
  - `iss`: el SPIFFE ID de quien llama, que tiene que coincidir con el SAN URI del certificado de la conexión;
  - `aud`: el SPIFFE ID de Hécate;
  - `iat` y `exp`, con `exp - iat` de 300 s como mucho y 30 s de margen;
  - `jti`: de 1 a 128 caracteres ASCII imprimibles.
- **Clave:** la clave de firma de llamadas registrada con el certificado de la conexión (FR-016). El `kid` de la
  cabecera no elige la clave.
- **Una sola vez:** Hécate guarda en memoria cada pareja `iss` y `jti` hasta que caduca. Rechaza los tokens con un `iat`
  anterior al arranque del proceso menos el margen, así que un reinicio solo deja repetir un token emitido en los 30 s
  previos. Se acepta: el token solo vale en una conexión con el certificado al que está ligada su clave, así que
  repetirlo exige la clave TLS del componente. Quitar el margen rechazaría tras cada reinicio los tokens de los equipos
  con el reloj algo atrasado.
- Un token rechazado da `auth` y deja fila (SC-006).

**Alternativas:** JWS propio con aws-lc-rs: reescribe reglas de validación (algoritmo, tiempos, audiencia) que la
librería ya prueba.

## R4. Lista de revocación

**Decisión:** una CRL X.509 v2 por cada intermedia que aún tiene certificados válidos, la vigente y, durante como mucho
3 días, la anterior. La firma la clave de esa intermedia, que está en memoria con Hécate desbloqueada.

- `thisUpdate` es el momento de generarla y `nextUpdate`, 300 s después (FR-020).
- Entradas: los certificados revocados con urgencia que no han caducado, con su fecha y el motivo (`keyCompromise` si
  es por un intento sospechoso, `unspecified` si lo pide el admin).
- `cRLNumber`: el instante de generación en milisegundos, siempre creciente.
- Se genera en cada petición: es una firma ECDSA sobre unas pocas entradas.
- La respuesta lleva las CRL en PEM y `expires_at` (contrato en [contracts/api.md](contracts/api.md)).

**Alternativas:** un JSON firmado propio (cada componente verificaría un formato propio).

## R5. Claves de firma de llamadas publicadas

**Decisión:** dos operaciones.

- `GET /v1/keys` devuelve la lista de las claves que valen para verificar llamadas: certificado válido e identidad
  habilitada. Es una lista de permitidas: una clave que no está ya no vale para llamadas nuevas (FR-017, SC-005). Lleva
  `expires_at` a 300 s.
- `GET /v1/keys/{kid}` devuelve cualquier clave registrada, también una antigua durante la retención de la auditoría,
  con su estado (`active` o `verify_only`), para verificar firmas pasadas.

Cada clave es un JWK de RFC 8037 (OKP, Ed25519) con `kid` (huella de RFC 7638), `alg: EdDSA` y `use: sig`, más tres
miembros propios: `cerbero_identity` (SPIFFE ID), `cerbero_status` y `cerbero_not_after` (caducidad del certificado).

**Por qué:** cada componente refresca la lista cada pocos minutos, y debe ser pequeña (una o dos claves por identidad).
Verificar firmas pasadas, en la auditoría, es raro y va por `kid`.

**Alternativas:** una sola lista con las antiguas marcadas (con 30 componentes renovando cada dos días crece unas 5.400
claves al año, y se descargaría entera cada pocos minutos).

## R6. Token de alta

**Decisión:** el texto `HET1.<identidad>.<id>.<secreto>.<huella>`.

- `<id>`: 8 bytes aleatorios en hexadecimal. Identifica el token en la lista del admin y en el registro.
- `<secreto>`: 32 bytes aleatorios en base64url sin relleno.
- `<huella>`: el SHA-256 de la CA raíz vigente en hexadecimal, con el que el componente comprueba a Hécate antes de
  enviar nada.

Hécate guarda solo el SHA-256 del secreto, en el evento de creación, y lo compara en tiempo constante. El token vale
para su identidad y su generación de CA, caduca a los 15 minutos (configurable) y solo lo gasta la emisión de un
certificado: un alta rechazada lo deja sin usar (clarify 2026-11-09, FR-011).

**Alternativas:**
- Un token opaco sin huella: el componente necesitaría la huella por otro camino.
- Un JWT firmado por Hécate: es más largo, y anularlo sigue exigiendo estado.

## R7. Copia de la clave de la CA raíz

**Decisión:** un sobre propio en PEM con la etiqueta `HECATE ROOT CA KEY`. El contenido es binario:
- la marca `HRK1`;
- los parámetros de Argon2id (`opslimit`, `memlimit`) y una sal de 16 bytes;
- un nonce de 12 bytes;
- el PKCS#8 de la clave cifrado con AES-256-GCM.

Los datos asociados son la cabecera entera y la huella de la CA raíz, así que la copia queda ligada a su certificado.
Usa el perfil de `HECATE_KDF_PROFILE`, como la frase de desbloqueo: cada apertura cuesta unos 22 s con `sensitive`.

- La crea y la abre solo la CLI (R9).
- La confirmación de la ceremonia relee una copia guardada, la abre con la frase y compara su clave pública con la de
  la CA raíz recién creada (clarify 2026-11-09).
- El formato es un contrato propio, en [contracts/formats.md](contracts/formats.md).

**Alternativas:** PKCS#8 cifrado con PBES2 y scrypt (trae RustCrypto); age (otra crate y otro formato para una sola
clave).

## R8. Certificado propio de Hécate

**Decisión:** Hécate genera su clave P-256 y la guarda en PKCS#8 en claro en la tabla `hecate_tls`.
- Es el riesgo aceptado de la spec: se lee sin desbloquear, también desde las copias, y vale mientras el certificado no
  caduque ni se revoque.
- Su certificado (`spiffe://<trust domain>/hecate`) se emite al crear o sustituir la CA.
- Se renueva solo con una clave nueva:
  - a los dos tercios de su vida, si está desbloqueada (una tarea comprueba cada minuto);
  - al desbloquearla, si caducó;
  - al momento, tras una revocación urgente del admin (FR-012). Su certificado no admite la revocación pasiva y la
    identidad `hecate` no se deshabilita (FR-018, FR-019).
- Nunca dura más que la intermedia que lo firma. Si caduca sin poder renovarse (sellada, o porque caducó la
  intermedia), la API deja de escuchar: la tarea de cada minuto cierra el listener y lo vuelve a abrir en cuanto hay un
  certificado nuevo, al desbloquear o al renovar la intermedia. Los componentes ven la conexión rechazada y siguen con
  lo que tienen (FR-012, FR-023). Con la intermedia caducada tampoco se crean tokens: `tokens.create` da `conflict`.
- Su serie y su caducidad salen en el bloque `ca` del estado, para que el admin pueda revocarlo con urgencia sin
  buscarla (FR-025, R17).
- La TLS presenta la cadena hasta la CA raíz incluida: un componente sin certificado comprueba con la huella del token
  que habla con la Hécate correcta (historia 2, escenario 5).

**Alternativas:** un fichero aparte con permisos `0600` (el mismo riesgo en otro sitio que proteger y copiar).

## R9. Ceremonias por el canal local

**Decisión:** operaciones en dos pasos. Lo pendiente vive solo en la memoria de Hécate, caduca a los 15 minutos y se
pierde al reiniciar, así que una ceremonia interrumpida no deja nada guardado (FR-004).

1. **Crear la CA:**
   1. `ca.create.begin` recibe el trust domain y la frase de la CA. Hécate comprueba:
      - que está desbloqueada y sin CA;
      - la regla del trust domain;
      - que la frase tiene al menos 12 caracteres y no es la de desbloqueo: la deriva con los parámetros guardados e
        intenta abrir la raíz de cifrado, lo que cuesta una pasada de Argon2id. No toca el contador común, porque que
        no la abra es el caso normal, y no da un oráculo nuevo de la frase de desbloqueo: quien llega al socket puede
        leer `hecate.db` y probar frases fuera de línea con el mismo coste.

      Después borra la frase, genera la clave de la intermedia y devuelve su solicitud.
   2. La CLI genera la CA raíz, firma la intermedia, escribe la copia protegida (`--out`, permisos `0600`, sin
      sobrescribir) y pide la ruta de una copia guardada y otra vez la frase. Abre la copia y compara su clave pública
      con la de la CA raíz.
   3. `ca.create.confirm` recibe los dos certificados. Hécate comprueba:
      - que la CA raíz está autofirmada;
      - que la intermedia es la suya y está firmada por la CA raíz;
      - los perfiles y las fechas.

      Después guarda, se emite su certificado y arranca la API.
   4. `ca.create.abort` descarta la ceremonia. Su motivo (`backup_check_failed`, `cancelled`) queda registrado: así un
      fallo de la frase que detecta la CLI también deja fila.
2. **Renovar la intermedia:** `ca.intermediate.begin` devuelve la solicitud y el certificado de la CA raíz. La CLI abre
   la copia (`--root-key`), comprueba que es la clave de esa CA raíz, firma y llama a `ca.intermediate.confirm`. Un fallo
   de frase o de clave se comunica con `ca.intermediate.abort` y no cuenta en el contador común.
3. **Sustituir la CA:**
   - `ca.replace.begin` recibe la prueba (la frase de desbloqueo o dos partes, como `secrets.destroy`, con el contador
     común) y la frase nueva de la CA.
   - Después, el mismo intercambio que al crearla, confirmación de la copia incluida.
   - `ca.replace.confirm` cambia de generación en una transacción y destruye la clave de la intermedia anterior.

La clave de la CA raíz nunca cruza el canal (FR-002). Contrato en
[contracts/control-protocol.md](contracts/control-protocol.md).

**Alternativas:**
- La ceremonia entera en el proceso de Hécate (descartada en el clarify: la clave pasaría por el proceso que atiende la
  red).
- Una herramienta aparte para la CA raíz (dos binarios que mantener).

## R10. Almacenamiento y Event Sourcing

**Decisión:** los aggregates que fija ARCHITECTURE para Hécate (Event Sourcing sí, CQRS no), con el estado reconstruido
al leer cada stream:

| Aggregate | Stream | Qué lleva |
|---|---|---|
| Key (CA) | `ca` | Creación y sustitución de la CA (generación, trust domain, huella) y cada versión de intermedia con su estado: `active`, `verify_only` y `retired` |
| Identity | `identity/<nombre>` | Tokens de alta (creación con el hash del secreto, anulación), altas, deshabilitar y habilitar, intentos de renovación sospechosos |
| Certificate | `certificate/<identidad>/<serie>` | Emisión (perfil, intermedia, clave de firma de llamadas, resumen de la petición y `Idempotency-Key`, DER), renovación y revocación |

- **Stream de la CA:** `ca` y no `key/ca`: `key/<identidad>` es el stream de la clave de ámbito de cada identidad
  (feature 002) y `ca` es un nombre de identidad válido, así que compartirían stream.
- **Material fuera de los eventos:** como en 002, se justifica en Complexity Tracking.
  - `ca_material`: los certificados de la CA y la clave de cada intermedia, cifrada bajo la raíz de cifrado con
    AES-256-GCM y datos asociados que la ligan a su generación, su versión y su huella. La fila se borra al retirar la
    intermedia: es un borrado criptográfico, con `secure_delete`.
  - `hecate_tls`: la clave propia de Hécate (R8).
- **Freno de altas:** tabla `enroll_attempts` (R13), aparte de `attempts`.
- **Registro de accesos:** dos columnas nuevas que admiten nulos, `identity` y `certificate` (FR-024).
- **Migración:** la del cambio 005, ampliada. Al abrir la base, las tablas nuevas se crean con `CREATE TABLE IF NOT
  EXISTS` y las dos columnas entran en la lista de columnas que le faltan al registro de accesos. Es idempotente,
  tolera dos conexiones que abren la base a la vez y una instalación del cambio 005 conserva sus datos. Sin
  `user_version`: con un solo mecanismo basta mientras las migraciones solo añadan.
- **Derivado, sin eventos en abanico:**
  - un token o un certificado de una generación de CA anterior no vale;
  - un token creado antes de deshabilitar su identidad no vale;
  - un certificado de un alta anterior no se renueva (FR-013).
- **Atomicidad:** cada operación es una transacción con la versión esperada de cada stream que toca. Dos altas con el
  mismo token compiten y solo una escribe (FR-011). Una renovación que pierde la carrera relee los streams y se
  reevalúa, así que la segunda de dos renovaciones simultáneas es un segundo intento (FR-015). El admin no pasa
  versión esperada (FR-026): la lee el servicio.
- **Órdenes repetidas del admin:** una orden que no cambia nada (deshabilitar una identidad deshabilitada, habilitar
  una habilitada, revocar un certificado revocado o caducado, anular un token que ya no está pendiente) se acepta sin
  eventos y deja su fila (FR-026). La
  urgencia sí es un cambio: `urgent` sobre una revocación o una deshabilitación pasivas las hace urgentes. Una
  identidad sin alta, que solo tiene tokens, da `not_found` en las órdenes de identidades.
- **Coste de las consultas:** las claves publicadas y la lista de revocación leen todos los streams de certificados,
  sin proyecciones; con 30 componentes que renuevan cada dos días son unos 5.400 certificados al año. El plan fija
  menos de 1 segundo con un año de datos, el umbral de SC-001, y una prueba lo comprueba.
- **Reservas:** `secrets.create` y `secrets.add_version` rechazan con `validation` las rutas de `hecate` y `admin`
  (FR-027).

**Alternativas:**
- Tablas de proyección: sería CQRS, que ARCHITECTURE no fija para Hécate.
- Un sexto aggregate para los tokens: pertenecen a su identidad.
- `PRAGMA user_version` para migrar: un segundo mecanismo para lo que ya hace el del cambio 005.

## R11. Contrato de la API y sobre común

**Decisión:** OpenAPI 3.1 en JSON en `shared/contracts/src/cerbero_contracts/openapi/`, dentro del módulo del paquete
`cerbero-contracts` para que viajen con él; las pruebas en Rust los leen por ruta:
- `common.v1.json`: el sobre y los códigos de error comunes, sin estado HTTP;
- `hecate.v1.json`: las operaciones de Hécate y el estado HTTP de cada código que usa.

El JSON se lee desde los dos lenguajes (principio I) y las pruebas lo cargan con serde_json, sin otra crate.

- **Sobre:** el del canal local (`status`, `data`, `error` con `code`, `message` y `retryable`, y `meta` con
  `request_id` y `duration_ms`). `error` admite además `retry_after_s`, solo con `rate_limited`, y `outcome_unknown`,
  solo con `timeout`, como lo usa Hermes ([docs/hermes.md](../../docs/hermes.md)). `X-Request-Id` es opcional, de 1 a
  128 caracteres ASCII imprimibles como `Idempotency-Key`, y se devuelve en `meta`; si no sigue esa regla, `validation`
  (FR-029).
- **Códigos:** los nueve cerrados del sobre común: los ocho que ya fijan GLOSSARY y Hermes (`validation`, `auth`,
  `not_found`, `conflict`, `rate_limited`, `provider_unavailable`, `timeout` e `internal`) más `unavailable`, que
  significa que el componente no puede atender ahora y vale reintentar (Hécate sellada).
  - Es la primera vez que el sobre común se escribe en `shared/contracts`.
  - Añadir el código ahí es la revisión del contrato común que pide el principio II; al consolidar, GLOSSARY y
    docs/hermes.md lo recogen (R19).
  - `unavailable` es el propio componente y `provider_unavailable`, un proveedor externo de Hermes: conviven.
  - Hécate no usa `provider_unavailable` ni `timeout`.
  - El canal local no cambia: sigue respondiendo `conflict` con Hécate sellada.
- **HTTP:** el sobre común no fija estados, porque Hermes devuelve dentro del sobre los fallos de sus proveedores
  ([docs/hermes.md](../../docs/hermes.md)); cada API los fija en su contrato. Los de Hécate, en `hecate.v1.json`:

  | Código de error | Estado HTTP |
  |---|---|
  | `validation` | 400 |
  | `auth` | 401 |
  | `not_found` | 404 |
  | `conflict` | 409 |
  | `rate_limited` | 429, con `Retry-After` |
  | `unavailable` | 503 |
  | `internal` | 500 |

- **Operaciones:**
  - `POST /v1/identities:enroll`;
  - `POST /v1/certificates:renew`;
  - `GET /v1/revocations`;
  - `GET /v1/keys` y `GET /v1/keys/{kid}`;
  - `GET /v1/capabilities`, que como todo exige certificado y token corto (principio V).

  Las `POST` exigen `Idempotency-Key`.
- **Registro:**
  - una petición que no es JSON estándar se rechaza sin fila y queda en los logs, sin su contenido (clarify
    2026-11-09);
  - las consultas atendidas de la lista de revocación y de las claves no dejan fila, y sus rechazos sí.

**Alternativas:** OpenAPI en YAML (las pruebas necesitarían un lector de YAML); gRPC (otro stack; ARCHITECTURE fija
OpenAPI).

## R12. Repetición e `Idempotency-Key`

**Decisión:** la repetición se reconoce por el contenido (FR-015):
- en el alta, el mismo token, la misma solicitud y la misma clave de firma de llamadas;
- en la renovación, el mismo certificado de conexión, la misma solicitud y la misma clave, con un token corto nuevo.

Las dos devuelven el mismo certificado, guardado en su evento. El evento de emisión guarda el SHA-256 del cuerpo
canónico y la `Idempotency-Key`; reutilizar esa clave con otro contenido da `conflict`. Las dos se buscan dentro de la
identidad (la del token en el alta, la del certificado en la renovación), así que dos componentes pueden usar la misma
clave sin chocar. Una repetición no cuenta como segundo intento ni como fallo del freno (FR-015, FR-034), y deja una
fila con `detail: repeat` para que el uso repetido de un token se vea en la auditoría.

**Alternativas:** deduplicar solo por `Idempotency-Key` (un componente que la pierde al reiniciarse gastaría la
renovación o el token).

## R13. Freno de altas

**Decisión:** la tabla `enroll_attempts` con los fallos seguidos, el fin de la espera y las peticiones rechazadas
durante ella. Sobrevive a los reinicios (FR-034).

- **Fallo:** toda alta rechazada, salvo `unavailable` (sellada) y `rate_limited` (el propio freno).
- **Esperas:** el décimo fallo seguido abre una espera de 1 minuto. Los siguientes la alargan a 2, 4 y 8 minutos, y
  después a 15, con la misma regla que el contador de desbloqueo de 001.
- Durante la espera, toda alta recibe `rate_limited` con `retry_after_s`, también con un token válido.
- **Registro:** una fila al cerrar cada tanda, con los fallos que la abrieron y las peticiones rechazadas durante la
  espera. La cierra la siguiente petición o la tarea de cada minuto (SC-017).
- Un alta emitida pone el contador a cero. `status` indica si las altas están frenadas (FR-025).
- No toca el contador común: un ataque al alta nunca bloquea el desbloqueo.

**Alternativas:** limitar por dirección de origen (en una red de casa se falsea o se comparte, y añade estado por
dirección).

## R14. Secretos en memoria

**Decisión:** el mismo enfoque que el FR-007 del cambio 005, con tipos que se borran al soltarse:

| Secreto | Dónde | Tipo |
|---|---|---|
| Frase de la CA | CLI; Hécate, solo al crear o sustituir la CA | `Passphrase` |
| Clave de la CA raíz | Solo la CLI | `RootCaKey`: su PKCS#8 en `SecretBytes`; cada firma carga el `KeyPair` de rcgen en un guardián que lo borra con `zeroize` al terminar |
| Contenido de la copia abierta | CLI | `Zeroizing` |
| Claves de las intermedias | Hécate, solo desbloqueada | `SecretBytes` descifrado con la raíz de cifrado en cada operación que firma y borrado al terminar |
| Clave de una ceremonia pendiente | Hécate | Dentro de la ceremonia, que se borra al confirmarla, anularla o caducar |
| Clave propia de Hécate | Hécate | PKCS#8 leído en `Zeroizing` y cargado en rustls |
| Secreto del token de alta | Hécate y CLI | `Zeroizing`; el canal ya borra peticiones y respuestas (cambio 005) |

Las copias dentro de aws-lc-rs y de rustls quedan en el riesgo residual ya aceptado (lo interno de las librerías). Lo
comprueban las mismas pruebas que en el cambio 005: tipos que no se clonan ni se muestran con `Debug` (`compile_fail`) y
pruebas de borrado.

## R15. Configuración

**Decisión:** tres variables nuevas, además de las del cambio 005:

| Variable | Por defecto | Rango |
|---|---|---|
| `HECATE_API_ADDR` | `127.0.0.1:8443` | Dirección y puerto |
| `HECATE_CERT_LIFETIME_HOURS` | `72` | 1 a 72 |
| `HECATE_ENROLL_TOKEN_MINUTES` | `15` | 1 a 60 |

Un valor no válido es un error de configuración (`error: …`, código 2, como en el cambio 005). Son fijos la intermedia (365 días), la CA
raíz (3.650 días), la validez de lo publicado (300 s), la vida máxima del token corto (300 s) y su margen (30 s).

## R16. Pruebas

**Decisión:** TDD (principio IV) en todos los niveles, con las herramientas del ADR 0006 y dos dependencias de
desarrollo nuevas: hyper-rustls 0.27.10 (el cliente del componente de prueba) y jsonschema 0.61.0 (pruebas de
contrato).

- **Unitarias y propiedades:** formato del token y de la copia, trust domain y SPIFFE ID, claims del token corto,
  política del freno, estados de los aggregates.
- **Aplicación:** en `crates/application/tests/`, con los dobles en memoria y el adaptador real de la PKI, como en 005.
  Los cortes de luz se prueban con un almacén que falla dentro de la transacción de cada operación, como
  `crash_during_change.rs` del cambio 005 (SC-007).
- **Integración:**
  - SQLite y la migración;
  - la API de extremo a extremo con un **componente de prueba** (`testing::TestComponent`). El componente genera sus
    claves y su solicitud, se da de alta sin certificado comprobando a Hécate por la huella y el SPIFFE ID, renueva con
    certificado y token corto, y descarga claves y CRL, que comprueba con webpki;
  - las fugas, como `no_secret_leak.rs`: ni la base, ni el registro, ni los logs, ni las respuestas contienen tokens,
    la frase de la CA, claves de la CA ni claves privadas de componentes (SC-003).
- **Contrato:** cada respuesta de la API se valida contra los OpenAPI de `shared/contracts` (R11), y cada operación
  nueva del canal local tiene su prueba de contrato (`contract_*_ops.rs`), como en el cambio 005.
- **Hora:** la aplicación valida los certificados con el port `Clock` (R1) y el componente de prueba comprueba a Hécate
  con el mismo reloj, así que las pruebas adelantan el tiempo con `FakeClock`.
- **TLS 1.2:** el cliente que demuestra que no conecta es un rustls con `tls12`, solo como dependencia de desarrollo;
  la comprobación de calidad confirma que el árbol normal no la activa.
- **CLI:** las ceremonias se prueban dentro del proceso, con un terminal guionizado y copias en directorios temporales.
- **Rendimiento:** SC-001 y SC-010, un alta o una renovación en menos de 1 segundo, y las claves publicadas y la lista
  de revocación en menos de 1 segundo con un año de datos (R10).
- **Plataformas:** la suite pasa en macOS y en el contenedor Linux del cambio 005 (FR-031).
- **Quickstart:** un ejemplo `component` (`cargo run --example component`) hace de componente a mano.

## R17. Salud

**Decisión:** solo por el canal local, sin `/health` en la API.
- `hecate status` añade un bloque `ca` con estos campos (FR-025), que se calculan con los eventos y también responden
  con Hécate sellada:
  - si existe la CA, con su trust domain y la huella;
  - la serie y la caducidad del certificado vigente de Hécate, para revocarlo con urgencia sin buscarla (R8);
  - los días que le quedan a la intermedia, cuando son menos de 30 (0 si ya caducó);
  - las identidades a menos de 24 horas de caducar;
  - los intentos sospechosos de las últimas 24 horas;
  - si las altas están frenadas.
- Los healthchecks del contenedor o de systemd ejecutan la CLI.
- Al consolidar, el doc de Hécate deja de listar `/health/live` y `/health/ready` (FR-032).

**Alternativas:** `/health` con mTLS (un orquestador sin certificado no podría usarlo); `/health` sin autenticar en un
puerto local (excepción al principio V).

## R18. OpenTelemetry

**Decisión:** se mantiene la excepción de 001, 002 y 005 (clarify 2026-11-09), justificada en Complexity Tracking solo
porque el colector y el backend siguen abiertos en DECISIONS. Lo que pasa queda en el registro de accesos y en logs
estructurados a stdout.

## R19. Consolidación al cerrar

Lo que fija el FR-032, revisado contra los docs del cambio 005:

- **[docs/hecate.md](../../docs/hecate.md):**
  - las claves publicadas solo para componentes autenticados (apartados 2 y 5);
  - el alta en la tabla de contratos y sin certificado de cliente;
  - la CA creada en su propia ceremonia, con la clave de la CA raíz solo en la CLI (diagrama de arranque y texto);
  - la sustitución de la CA (apartado 10);
  - el canal de resolución, que llega con la Access Policy (apartado 4);
  - la versión esperada solo en los secretos;
  - las operaciones de identidades en el canal local y la CLI (apartados 4 y 9);
  - el alta con la clave de firma de llamadas;
  - la respuesta sellada y su riesgo (apartados 4 y 12);
  - la salud sin `/health`;
  - las librerías del ADR 0009 (apartados 1 y 7);
  - el almacenamiento nuevo (apartados 3 y 8): los streams `ca`, `identity/<nombre>` y
    `certificate/<identidad>/<serie>` y las tablas `ca_material`, `hecate_tls` y `enroll_attempts`.
- **ARCHITECTURE (Identidad y seguridad):** la renovación de la intermedia sin ceremonia offline; el token de alta
  también para volver a dar de alta una identidad; las claves de una identidad deshabilitada, que se conservan para
  firmas pasadas.
- **GLOSSARY:** el token de alta, los términos revocación urgente, lista de revocación y clave de firma de llamadas, y
  el código `unavailable` en Envelope.
- **docs/hermes.md:** el código común `unavailable` en su lista de códigos.
- **DECISIONS:**
  - las desviaciones frente a step-ca y SPIFFE: la renovación única con detección de reutilización, las claves solo
    para componentes autenticados, sustituir la CA en lugar de revocar la intermedia y el alta sin certificado de
    cliente;
  - el riesgo aceptado de la clave del certificado de Hécate;
  - y, en la PR que fusiona, la fila de «Cambios pausados» se retira (FR-033).
- **`shared/contracts/README.md`:** dónde están los OpenAPI, dentro del módulo `cerbero_contracts`, y el sobre común.

## R20. Dependencias y cargo-deny

| Crate | Versión | Licencia | Uso |
|---|---|---|---|
| axum | 0.8.9 | MIT | Router de la API |
| hyper | 1.12.0 | MIT | HTTP |
| hyper-util | 0.1.21 | MIT | Servidor y cliente de pruebas |
| tower | 0.5.3 | MIT | Servicio para hyper |
| tokio-rustls | 0.26.6 | MIT o Apache-2.0 | TLS sobre tokio |
| rustls | 0.23.46 | Apache-2.0, ISC o MIT | TLS 1.3 con aws-lc-rs |
| rcgen | 0.14.10 | MIT o Apache-2.0 | Certificados y CRL |
| x509-parser | 0.18.1 | MIT o Apache-2.0 | Lectura y verificación |
| jsonwebtoken | 11.1.0 | MIT | Token corto |
| hyper-rustls | 0.27.10 | Apache-2.0, ISC o MIT | Desarrollo: cliente del componente de prueba |
| jsonschema | 0.61.0 | MIT | Desarrollo: pruebas de contrato, sin features por defecto y con `resolve-file` |

Las licencias están en la política de `deny.toml`, que añade MIT-0 (MIT sin la obligación de atribución) por
`borrow-or-share`, una dependencia de jsonschema. Los duplicados nuevos que aparezcan se resuelven con versiones o se
justifican en `deny.toml`, como en el cambio 005. Las crates de criptografía comparten aws-lc-rs 1.19.0 con la del
ADR 0007.
