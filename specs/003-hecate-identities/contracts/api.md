# Contrato: API mTLS de Hécate (v1)

La primera API de Hécate entre componentes, con el contrato común del principio II: el mismo sobre y los mismos códigos
que el resto, la versión mayor en la ruta, `Idempotency-Key` en lo que tiene efecto y `/capabilities`. Decisiones en
[research.md](../research.md) (R1, R3-R5, R11-R13). Al implementar se escribe en
`shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`.

## Transporte

- TLS 1.3 en `HECATE_API_ADDR` (por defecto, `127.0.0.1:8443`), HTTP/1.1 o HTTP/2. Cuerpos en JSON estándar UTF-8 de
  hasta 64 KiB.
- Hécate presenta la cadena entera: su certificado (`spiffe://<trust domain>/hecate`), la intermedia y la CA raíz.
- El certificado de cliente es opcional en la TLS, que solo comprueba que el cliente tiene su clave. Hécate valida la
  cadena, la caducidad y la revocación en todas las operaciones salvo en el alta: un certificado que no vale recibe
  `auth` y deja fila. El componente presenta su certificado y la intermedia que lo firmó (`chain[0]` de su alta o de
  su renovación).
- Sin CA, la API no escucha. Tampoco sin certificado propio vigente: si caduca sin poder renovarse (Hécate sellada, o
  con la intermedia caducada), deja de escuchar hasta tener uno nuevo. Con Hécate sellada y certificado vigente,
  escucha y responde `unavailable` a todo (FR-023).
- Un handshake que falla (firma que no corresponde al certificado, cliente sin TLS 1.3 o saludo mal formado) no llega
  a Hécate como petición: no tiene respuesta ni fila en el registro, y queda en los logs estructurados con el motivo y
  la dirección de origen.

## Autenticación

| Operación | Credencial |
|---|---|
| `POST /v1/identities:enroll` | El token de alta, en el cuerpo. Sin certificado de cliente; si llega uno, no se usa |
| Todas las demás | Certificado de cliente válido de la CA vigente y emitido por Hécate, más `Authorization: Bearer <token corto>` firmado con la clave de firma de llamadas registrada con ese certificado ([formats.md](formats.md#token-corto)) |

## Sobre y errores

El sobre es el del canal local:

```json
{
  "status": "completed | failed",
  "data": { },
  "error": { "code": "validation", "message": "...", "retryable": false, "retry_after_s": 30 },
  "meta": { "request_id": "...", "duration_ms": 12 }
}
```

`meta.request_id` repite `X-Request-Id` si llega (de 1 a 128 caracteres ASCII imprimibles; si no, `validation`), o lo
genera Hécate. `retry_after_s` solo aparece con `rate_limited`.

El sobre y los nueve códigos comunes están en `common.v1.json`, sin estado HTTP
([research R11](../research.md#r11-contrato-de-la-api-y-sobre-común)). Esta tabla es el estado que fija Hécate para
los códigos que usa; no usa `provider_unavailable` ni `timeout`.

| Código | HTTP | Cuándo | `retryable` |
|---|---|---|---|
| `validation` | 400 | Cuerpo que no es JSON estándar (sin fila en el registro), campos que faltan o sobran, PEM que no sigue RFC 7468, solicitud o clave no válida, otra identidad, clave repetida, `X-Request-Id` mal formado | No |
| `auth` | 401 | Token de alta, certificado o token corto que no valen; identidad deshabilitada; certificado revocado, caducado, de otra CA o sin renovación; segundo intento de renovar | No |
| `not_found` | 404 | Clave pedida por `kid` que no existe; una ruta que no existe o un método que la ruta no admite, que no son operaciones (sin fila en el registro) | No |
| `conflict` | 409 | `Idempotency-Key` reutilizada con otro contenido en la misma identidad (se evalúa dentro de cada identidad) | No |
| `rate_limited` | 429 | Altas frenadas (FR-034), con la cabecera `Retry-After` | Sí, tras `retry_after_s` |
| `unavailable` | 503 | Hécate sellada | Sí |
| `internal` | 500 | Fallo inesperado | Sí |

`unavailable` es nuevo en el sobre común: el componente no puede atender ahora y vale reintentar más tarde. El canal
local sigue respondiendo `conflict` con Hécate sellada.

## Operaciones

### `POST /v1/identities:enroll`

Primer certificado de una identidad (historia 2). Cabeceras: `Idempotency-Key` (de 1 a 128 caracteres ASCII
imprimibles, obligatoria) y `X-Request-Id` (opcional).

```json
{ "token": "HET1....", "csr": "-----BEGIN CERTIFICATE REQUEST-----\n...", "call_key": "-----BEGIN PUBLIC KEY-----\n..." }
```

Si va bien:

```json
{
  "identity": "spiffe://cerbero.home/hermes",
  "serial": "5f1c...",
  "certificate": "-----BEGIN CERTIFICATE-----\n...",
  "chain": ["-----BEGIN CERTIFICATE-----\n(intermedia)", "-----BEGIN CERTIFICATE-----\n(CA raíz)"],
  "not_after": "2026-11-12T12:00:00.000000+00:00",
  "call_key_id": "NzbLsXh8uDCcd-6MNwXF4W_7noWXFZAfHkxZsRGC9Xs"
}
```

- La identidad la decide el token (FR-010). El token queda usado en la misma transacción que la emisión (FR-011).
- **Repetición:** el mismo token, la misma solicitud y la misma clave devuelven el mismo certificado (FR-015).
- **Rechazo:** un alta rechazada deja el token sin usar (FR-011) y cuenta para el freno (FR-034).

### `POST /v1/certificates:renew`

Renovación (historia 3), con certificado de cliente y token corto. Cabeceras: `Authorization`, `Idempotency-Key` y
`X-Request-Id` (opcional). Cuerpo: `{ "csr": "...", "call_key": "..." }`, con claves nuevas. Si va bien, devuelve los
mismos campos que el alta.

- **Repetición:** la misma petición con un token corto nuevo devuelve el mismo certificado sin efectos (FR-015).
- **Segundo intento** con un certificado ya renovado: `auth`, una fila de posible robo y la revocación urgente de
  todos los certificados válidos de la identidad (FR-015).

### `GET /v1/revocations`

Las CRL de las intermedias con certificados válidos (FR-020), en PEM:

```json
{ "crls": ["-----BEGIN X509 CRL-----\n..."], "expires_at": "2026-11-09T12:05:00.000000+00:00" }
```

### `GET /v1/keys`

Las claves que valen para verificar llamadas, en una lista de permitidas: una clave que no está no vale para llamadas
nuevas (FR-017).

```json
{ "keys": [ { "kty": "OKP", "crv": "Ed25519", "x": "...", "kid": "...", "alg": "EdDSA", "use": "sig",
              "cerbero_identity": "spiffe://cerbero.home/hermes", "cerbero_status": "active",
              "cerbero_not_after": "2026-11-12T12:00:00.000000+00:00" } ],
  "expires_at": "2026-11-09T12:05:00.000000+00:00" }
```

### `GET /v1/keys/{kid}`

Cualquier clave registrada, también una antigua durante la retención de la auditoría, para verificar firmas pasadas.
Devuelve `{ "key": { ...JWK con cerbero_status: "active" o "verify_only"... } }`, o `not_found`.

### `GET /v1/capabilities`

```json
{ "component": "hecate", "api": "v1",
  "operations": ["identities:enroll", "certificates:renew", "revocations", "keys", "keys/{kid}", "capabilities"] }
```

## Registro de accesos

- Toda operación con efecto, y todo rechazo de una petición en JSON estándar, deja una fila con quién, qué identidad y
  qué certificado (FR-024).
- La repetición exacta de un alta deja una fila con `detail: repeat`: el uso repetido de un token queda a la vista.
- Las consultas atendidas de `revocations` y `keys` no dejan fila; sus rechazos sí.
- Mientras las altas están frenadas, una fila por tanda (FR-034).

## Lo que hace cada componente

El lado del componente lo implementa cada uno contra este contrato (principio I). El componente de prueba de la feature
lo cubre entero.

1. Genera en su equipo una clave P-256 para el certificado y una Ed25519 para firmar sus llamadas.
2. Antes de enviar el token, comprueba a Hécate:
   - la cadena que presenta encadena con una CA raíz cuya huella es la del token;
   - el certificado de Hécate tiene el SAN `spiffe://<trust domain>/hecate`.
3. En mTLS presenta su certificado y la intermedia que lo firmó (`chain[0]`), porque quien recibe solo confía en la CA
   raíz.
4. Renueva hacia los dos tercios de la vida de su certificado (SC-008). Si recibe `unavailable` o Hécate no acepta la
   conexión, conserva el actual y reintenta más tarde.
5. Refresca `revocations` y `keys` antes de su `expires_at`. Si Hécate no responde, sigue con su última copia (principio
   VI).
6. Firma cada llamada con un token corto. Al recibir una, comprueba:
   - la cadena hasta la CA raíz y la CRL;
   - que el SPIFFE ID del certificado es el `iss` del token;
   - que la clave del token está en la lista de permitidas.
