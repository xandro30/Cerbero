# Data Model: Access Policy de Hécate y resolución de secretos

Lo que la feature 006 añade al modelo de Hécate. Usa el event store (`events`) y el registro de accesos de las features
anteriores: **no hay tablas nuevas ni migración**. Decisiones en [research.md](research.md) (R1 a R5 y R8).

## Access Policy (`policy/<identidad>`)

Un stream por identidad, que solo existe cuando el admin le ha cambiado algo. Cada evento lleva su instante (`at`),
como todos los de Hécate, y ninguno lleva un valor.

| Evento | Datos | Efecto |
|---|---|---|
| `ReadGranted` | `secret` (ruta), `by` (`admin`) | La identidad puede resolver ese secreto de otra identidad |
| `ReadRevoked` | `secret`, `by` | Deja de poder |
| `StatusGranted` | `by` | La identidad puede consultar el estado agregado |
| `StatusRevoked` | `by` | Deja de poder |

- **Estado derivado:** `reads`, el conjunto de rutas ajenas que puede resolver; `status`, si tiene el permiso de
  estado; y `version`, la del stream. Sin stream, `reads` vacío, `status` falso y versión 0.
- **Puede resolver** `secret://<dueño>/<resto>` si `<dueño>` es la propia identidad (regla por defecto, sin guardar) o
  si la ruta exacta está en `reads` (FR-005, FR-006).
- **Historial:** los eventos con su instante. Lo que podía hacer en un momento pasado sale de aplicarlos hasta ese
  instante (SC-006).
- **Repetir una orden** que no cambia nada (conceder lo que ya tiene, retirar lo que no tiene) no escribe eventos
  (FR-008).

### Transiciones

```text
Lectura de un secreto ajeno:  sin excepción ──ReadGranted──▶ concedida ──ReadRevoked──▶ sin excepción
Permiso de estado:            sin permiso ──StatusGranted──▶ concedido ──StatusRevoked──▶ sin permiso
```

### Reglas al cambiarla

| Regla | Requisito |
|---|---|
| La identidad sigue la regla de nombres (minúsculas, dígitos y `-`, empezando por letra, hasta 63) y no es `hecate` ni `admin`; no tiene que estar dada de alta | FR-006, FR-007 |
| La ruta del secreto sigue la regla de la feature 002, sin versión (`@N`) | FR-006, FR-017 |
| El secreto existe al conceder la lectura | FR-006 |
| El dueño del secreto no es la propia identidad | FR-006 |
| Hécate está desbloqueada | FR-014 |
| Sin versión esperada; si otro cambio se adelanta, se vuelve a leer y a evaluar | FR-008 |

## Secret (`secret/<ruta>`): lo que cambia

- **`rotate_every`:** al guardar un secreto nuevo, una duración ISO 8601 en días o en semanas, `P<n>D` o `P<n>W`, con n
  de 1 a 3650 días o de 1 a 520 semanas (FR-023, [formats.md](contracts/formats.md#rotate_every)). Se guarda como
  texto, igual que ahora, y los eventos no cambian de formato.
- **Debe rotarse:** tiene una versión `active` y un `rotate_every` que se lee como duración, y la fecha de creación de
  esa versión más la duración ya ha llegado: en el instante exacto ya cuenta. Sin versión activa no cuenta, y un valor
  antiguo que no es una duración se sigue leyendo y tampoco cuenta.

## Resolución

No guarda nada salvo las filas del registro. Por petición:

1. **Comprobación**, referencia a referencia y en el orden pedido: permiso (con la política leída una sola vez),
   secreto, versión resoluble según la feature 002 y material guardado. La primera que falla rechaza la petición entera
   con `not_found`, sin descifrar nada.
2. **Descifrado** de todas, solo si todas pasan.

| Campo de cada secreto resuelto | Qué es |
|---|---|
| `reference` | La referencia tal como se pidió |
| `version` | La versión resuelta |
| `value` | El valor en texto, en un tipo que se borra al soltarse |
| `cache_ttl_soft_s`, `cache_ttl_hard_s` | 300 y 3600 (`SecretsSettings`) |

## Estado agregado

| Campo | Qué cuenta | Sellada |
|---|---|---|
| `sealed` | Si Hécate está sellada | Sí |
| `secrets_expired` | Secretos con versiones vivas y `expires_at` pasada | Sí |
| `secrets_expiring_soon` | Secretos con versiones vivas que caducan en 7 días | Sí |
| `secrets_due_rotation` | Secretos que deben rotarse (ver Secret) | Sí |
| `identities_expiring_24h` | Identidades habilitadas cuyo certificado más reciente caduca en 24 horas | Sí |
| `suspicious_renewals_24h` | Intentos de renovación sospechosos en 24 horas | Sí |
| `intermediate_days_left` | Días que le quedan a la intermedia, solo si son menos de 30 (si no, `null`) | Sí |
| `enrollment_throttled` | Si las altas están frenadas | Sí |

- Ningún campo nombra un secreto ni una identidad (FR-011). Todos se calculan con metadatos, sin material cifrado.
- El `status` del canal local gana `secrets_due_rotation` (FR-013).

## Registro de accesos

| Operación | Método | Quién (`subject`) | `identity` | `certificate` | Ruta y versión |
|---|---|---|---|---|---|
| `secret_resolve` | `identity` | El SPIFFE ID de la conexión | La que llama | La serie presentada | Atendida: una fila por referencia, con ruta y versión. Rechazada: una fila con la primera referencia que falla |
| `status_get` | `identity` | El SPIFFE ID, o nadie | Si se conoce | Si se conoce | — |
| `policy_grant`, `policy_revoke` | `local` | `admin` | La de la política | — | La ruta en las lecturas |
| `policy_list`, `policy_get` | `local` | `admin` | La consultada | — | — |

- `status_get` solo deja fila al rechazar; una consulta atendida no la deja (FR-015).
- `policy_grant` y `policy_revoke` llevan `detail` `read` o `status`, y dejan fila también cuando no cambian nada.
- Los rechazos antes del caso de uso (sellada, `X-Request-Id` mal formado) dejan su fila con el registrador de la
  feature 003, sin `subject`.
- Una petición que no es JSON estándar no deja fila (FR-029 de la 003).
- Nunca un valor, una huella, una clave privada ni un token (FR-015, FR-016).

## Validaciones y requisitos

| Regla | Requisito |
|---|---|
| Hasta 100 referencias por petición, cada una con la regla de rutas y `@N` opcional (N de 1 a 9 cifras, sin ceros a la izquierda) | FR-017 |
| Todo o nada, con un único `not_found` igual en los tres casos | FR-003 |
| Un `not_found` de la resolución no se responde antes del tiempo mínimo (250 ms por defecto) | FR-004 |
| La versión: `active` por defecto; `deprecated` solo pedida y en su gracia; nunca `revoked` ni `destroyed` | FR-002 |
| El permiso de estado se comprueba en cada consulta; sin él, rechazo con fila | FR-011, FR-015 |
| `rotate_every`, una duración en días o semanas al guardar | FR-023 |
