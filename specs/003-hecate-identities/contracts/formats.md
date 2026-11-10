# Contrato: formatos de las identidades

Formatos que fija esta feature. Todos los PEM siguen RFC 7468 con el lector del cambio 005 (espacios ASCII, con
contenido y base64 estricto) y, además, un solo bloque con la etiqueta que corresponde (FR-029).

## Token de alta

```text
HET1.<identidad>.<id>.<secreto>.<huella>
```

| Parte | Regla |
|---|---|
| `HET1` | Versión del formato |
| `<identidad>` | `IdentityName` ([data-model.md](../data-model.md#valores)) |
| `<id>` | 16 caracteres hexadecimales en minúsculas (8 bytes aleatorios) |
| `<secreto>` | 43 caracteres base64url sin relleno (32 bytes aleatorios) |
| `<huella>` | 64 caracteres hexadecimales en minúsculas: SHA-256 del DER de la CA raíz vigente |

- Hécate guarda el SHA-256 del secreto y nunca el token (FR-007).
- Un texto que no sigue este formato da `validation`; uno bien formado que no corresponde a un token pendiente da
  `auth`.

## Copia de la clave de la CA raíz

PEM con la etiqueta `HECATE ROOT CA KEY`. El contenido son estos bytes:

| Campo | Bytes | Valor |
|---|---|---|
| Marca | 4 | `HRK1` en ASCII |
| `opslimit` de Argon2id | 8 | Entero sin signo big-endian |
| `memlimit` de Argon2id | 8 | Entero sin signo big-endian, en bytes |
| Sal | 16 | Aleatoria |
| Nonce | 12 | Aleatorio |
| Cifrado | el resto | AES-256-GCM del PKCS#8 de la clave de la CA raíz, con la etiqueta de 16 bytes al final |

- La clave de cifrado es Argon2id de la frase de la CA con esos parámetros: los del perfil de `HECATE_KDF_PROFILE` al
  crearla.
- Los datos asociados son los 48 bytes de cabecera, seguidos de los 32 bytes de la huella de la CA raíz. La copia solo
  se abre junto al certificado de su CA raíz.
- Una marca desconocida o un cifrado que no se abre da el mismo error, sin revelar si la frase es incorrecta o la copia
  es de otra CA: la renovación de la intermedia aborta entonces con `wrong_passphrase`. Una copia que se abre pero cuya
  clave no es la de la CA raíz aborta con `wrong_root_key`.

## Perfiles de certificado

ECDSA P-256 con SHA-256 en todos, en PEM `CERTIFICATE`.

| Campo | CA raíz | Intermedia | Componente y Hécate |
|---|---|---|---|
| Sujeto | `CN=Cerbero Root CA <td>` | `CN=Cerbero Intermediate CA <td> <generación>.<versión>` | `CN=<identidad>` |
| SAN | — | — | Un único URI: `spiffe://<td>/<identidad>` |
| `basicConstraints` | `CA:true`, `pathLen 1`, crítica | `CA:true`, `pathLen 0`, crítica | `CA:false`, crítica |
| `keyUsage` | `keyCertSign`, `cRLSign` | `keyCertSign`, `cRLSign` | `digitalSignature` |
| `extKeyUsage` | — | — | `serverAuth`, `clientAuth` |
| Validez | 10 años | 1 año, sin pasar de la CA raíz | La vida configurada (3 días por defecto), sin pasar de la intermedia; `notBefore` un minuto antes de emitir |
| Serie | 128 bits aleatorios, positivo | ídem | ídem |

## Solicitud de certificado

- PEM `CERTIFICATE REQUEST` (PKCS#10), con firma válida y clave P-256.
- Si pide un SAN, tiene que ser exactamente el SPIFFE ID que decide Hécate; si no, `validation` (FR-010).
- Hécate ignora el sujeto y las demás extensiones.
- Al renovar, la clave tiene que ser distinta de la del certificado actual (FR-014).

## Clave de firma de llamadas

PEM `PUBLIC KEY` (SubjectPublicKeyInfo de RFC 8410, Ed25519). Distinta de la clave del certificado y, al renovar, de la
actual (FR-008, FR-014). Su `kid` es la huella JWK de RFC 7638 en base64url.

## Token corto

JWT compacto con cabecera `{"alg": "EdDSA", "typ": "JWT"}` y estos claims:

| Claim | Regla |
|---|---|
| `iss` | SPIFFE ID de quien llama; igual al SAN URI del certificado de la conexión |
| `aud` | SPIFFE ID del destinatario (para Hécate, `spiffe://<td>/hecate`) |
| `iat`, `exp` | Segundos Unix; `exp - iat` de 300 s como mucho; 30 s de margen al comprobarlos |
| `jti` | De 1 a 128 caracteres ASCII imprimibles; no se acepta dos veces con el mismo `iss` |

Los permisos y el contexto de auditoría que lleva para otras operaciones llegan con la Access Policy (cambio siguiente).

## CRL

- X.509 v2 en PEM `X509 CRL`, firmada por la intermedia que emitió los certificados.
- `thisUpdate` es el momento de generarla, `nextUpdate` 300 s después y `cRLNumber` el instante en milisegundos.
- Entradas: los revocados con urgencia que no han caducado, con su fecha y el motivo (`keyCompromise` tras un intento
  sospechoso; `unspecified` si lo pide el admin, que RFC 5280 §5.3.1 escribe sin la extensión de motivo).

## Clave publicada

Un JWK de RFC 8037 con los miembros `kty: OKP`, `crv: Ed25519`, `x`, `kid`, `alg: EdDSA` y `use: sig`, más tres propios:

| Miembro | Valor |
|---|---|
| `cerbero_identity` | SPIFFE ID de la identidad |
| `cerbero_status` | `active` (vale para llamadas) o `verify_only` (solo firmas pasadas) |
| `cerbero_not_after` | Caducidad del certificado con el que se registró |

## Huella de la CA raíz

`sha256:` seguido de los 64 caracteres hexadecimales en minúsculas del SHA-256 del DER del certificado. En el token de
alta va sin el prefijo.
