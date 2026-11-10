# 0009 — Pila de la API mTLS de Hécate

Estado: aceptada · Fecha: 2026-11-09

## Contexto

La feature 003 da a Hécate su primera API entre componentes: el alta con un token de un solo uso, la renovación de certificados, la lista de revocación y las claves con las que los componentes firman sus llamadas. El [ADR 0005](0005-rust-para-orthros-y-hecate.md) y el [0007](0007-pila-hecate-rust.md) dejan para esta feature el stack HTTP y TLS de la API y la PKI. Lo condicionan:

- la constitución (principio V): mTLS con identidad SPIFFE y un JWT corto firmado por quien llama y ligado al canal, así que la aplicación tiene que leer en cada petición el certificado del cliente;
- el alta llega sin certificado de cliente, autenticada por el token de alta: la TLS acepta conexiones con y sin certificado, y la aplicación decide;
- las primitivas del ADR 0007 (aws-lc-rs) y la regla de herramientas de la constitución: crates seguras y maduras, y todo el código propio en Rust;
- los formatos de la spec: certificados, solicitudes y claves en PEM según RFC 7468, una lista de revocación válida 5 minutos y la copia de la clave de la CA raíz protegida con una frase propia.

## Opciones

### HTTP

| Opción | A favor | En contra |
|---|---|---|
| **axum sobre hyper, con tokio-rustls** | Router y extractores maduros del equipo de tokio. El certificado del cliente pasa de la conexión TLS a cada petición con un bucle de aceptación propio | Más dependencias que hyper solo |
| hyper con hyper-util | Menos dependencias | Enrutado, errores y extracción del certificado escritos a mano |

### TLS

rustls con el proveedor aws-lc-rs es la TLS que preveía el ADR 0007: la misma criptografía que el resto de Hécate, un verificador de clientes configurable y comprobación de CRL. OpenSSL quedó descartado en el ADR 0007.

### Certificados y CRL

| Opción | A favor | En contra |
|---|---|---|
| **rcgen con x509-parser** | rcgen, del proyecto rustls, emite certificados, lee solicitudes y genera CRL con aws-lc-rs. x509-parser lee certificados y solicitudes y verifica sus firmas con aws-lc-rs | Dos crates: una para emitir y otra para leer |
| x509-cert de RustCrypto | Un solo modelo para leer y construir | Firma con las crates de RustCrypto, una segunda pila criptográfica |

### Token corto

| Opción | A favor | En contra |
|---|---|---|
| **jsonwebtoken** | Muy adoptada. Valida `exp`, `aud` e `iss` y verifica EdDSA con su backend aws-lc-rs | — |
| JWS propio con aws-lc-rs | Una dependencia menos | Reescribe reglas de validación que la librería ya cubre y prueba |

### Lista de revocación

| Opción | A favor | En contra |
|---|---|---|
| **CRL X.509** | Estándar; rustls y webpki la comprueban, y `nextUpdate` expresa los 5 minutos | Una CRL por cada intermedia que aún tiene certificados válidos |
| JSON firmado propio | Fácil de leer | Cada componente verifica un formato propio |

### Copia de la clave de la CA raíz

| Opción | A favor | En contra |
|---|---|---|
| **Sobre propio**: Argon2id de libsodium y AES-256-GCM de aws-lc-rs, en PEM con etiqueta y versión | Las mismas primitivas y perfiles que la frase de desbloqueo | Solo lo abre Hécate |
| PKCS#8 cifrado (PBES2 con scrypt) | Estándar: lo abren otras herramientas | Trae RustCrypto como segunda pila criptográfica |

## Decisión

- **HTTP:** axum 0.8 sobre hyper 1, con un bucle de aceptación propio sobre tokio-rustls 0.26 que pasa el certificado del cliente a cada petición.
- **TLS:** rustls 0.23 con aws-lc-rs, solo TLS 1.3, fijada en la configuración, y con preferencia por el intercambio de claves híbrido post-cuántico X25519MLKEM768. El certificado de cliente es opcional en la TLS, que solo comprueba que el cliente tiene su clave; la aplicación lo exige en todo salvo en el alta y valida su cadena con el verificador webpki de rustls, así que cada rechazo responde `auth` y queda registrado.
- **PKI:** rcgen 0.14 (aws-lc-rs, lectura de solicitudes, PEM y `zeroize`) para emitir certificados y CRL, y x509-parser 0.18 para leer certificados y solicitudes. ECDSA P-256 con SHA-256 para la CA y los certificados, como fija el doc de Hécate; Ed25519 para las claves de firma de llamadas.
- **Token corto:** jsonwebtoken 11 con su backend aws-lc-rs, con EdDSA.
- **Lista de revocación:** CRL X.509 v2 firmada por la intermedia que emitió los certificados, con `nextUpdate` a 5 minutos.
- **Copia de la clave de la CA raíz:** sobre propio versionado, con Argon2id (el perfil de la frase de desbloqueo) y AES-256-GCM, en PEM.
- **Contrato:** OpenAPI 3.1 en JSON en `shared/contracts`, dentro del paquete `cerbero-contracts`, con el sobre común. Las pruebas de contrato validan las respuestas con jsonschema.
- **Versiones:** la última estable de cada crate, con la regla del ADR 0007.

## Consecuencias

- No entra otra pila criptográfica: la TLS, los certificados, las CRL y los JWT firman y verifican con aws-lc-rs.
- La API vive en el mismo proceso que el canal local (`hecate serve`) y comparte su runtime.
- Las crates nuevas pasan por cargo-deny con la política de licencias del ADR 0008.
- La copia de la clave de la CA raíz solo la abre Hécate; su formato es un contrato propio, como el de las partes HEC2.
- Un componente que no use rustls tiene que comprobar la CRL y el SPIFFE ID con su propia librería.
