# Contrato: canal local, operaciones de identidades

Operaciones que esta feature añade al canal local de las features 001 y 002 (transporte, petición y sobre sin cambios,
en [specs/001-hecate-bootstrap/contracts/control-protocol.md](../../001-hecate-bootstrap/contracts/control-protocol.md)).
Decisiones en [research.md](../research.md) (R6, R9, R13, R17).

Los instantes se escriben como en el resto del canal (cambio 005): `isoformat` en UTC, sin fracción si los
microsegundos son 0.

Todas exigen Hécate desbloqueada salvo `status` (FR-023); sellada, responden `conflict`, como el resto del canal. Cada
una deja una fila en el registro de accesos con `subject: admin` y la identidad o el certificado
([data-model.md](../data-model.md#registro-de-accesos)). El admin no pasa versión esperada (FR-026).

## Ceremonias

| `op` | Cuándo | `args` | `data` si va bien |
|---|---|---|---|
| `ca.create.begin` | Sin CA y sin otra ceremonia | `trust_domain`, `ca_passphrase` | `ceremony_id`, `intermediate_csr` (PEM), `expires_at` |
| `ca.create.confirm` | Ceremonia de creación pendiente | `ceremony_id`, `root_certificate`, `intermediate_certificate` (PEM) | `trust_domain`, `root_fingerprint`, `intermediate_not_after` |
| `ca.create.abort` | Ceremonia de creación pendiente | `ceremony_id`, `reason` (`cancelled`, `backup_check_failed`) | — |
| `ca.intermediate.begin` | Con CA y sin otra ceremonia | — | `ceremony_id`, `intermediate_csr`, `root_certificate`, `expires_at` |
| `ca.intermediate.confirm` | Renovación pendiente | `ceremony_id`, `intermediate_certificate` | `intermediate_version`, `intermediate_not_after` |
| `ca.intermediate.abort` | Renovación pendiente | `ceremony_id`, `reason` (`cancelled`, `wrong_passphrase`, `wrong_root_key`) | — |
| `ca.replace.begin` | Con CA y sin otra ceremonia | `proof` (`{passphrase}` o `{shares}`, como en `secrets.destroy`), `ca_passphrase` | `ceremony_id`, `intermediate_csr`, `expires_at` |
| `ca.replace.confirm` | Sustitución pendiente | `ceremony_id`, `root_certificate`, `intermediate_certificate` | `trust_domain`, `root_fingerprint`, `generation` |
| `ca.replace.abort` | Sustitución pendiente | `ceremony_id`, `reason` (`cancelled`, `backup_check_failed`) | — |

- **Frase de la CA:** `ca_passphrase` llega a Hécate solo para comprobar que tiene 12 caracteres o más y que no es la de
  desbloqueo, y se borra después (FR-002).
- **Clave de la CA raíz:** nunca cruza el canal; la CLI firma la intermedia y envía solo certificados.
- **Comprobaciones al confirmar:** que la CA raíz está autofirmada, que la intermedia corresponde a la clave pendiente y
  está firmada por la CA raíz, los perfiles y las fechas ([formats.md](formats.md#perfiles-de-certificado)).
- **Pendiente:** una ceremonia caduca a los 15 minutos y se pierde al reiniciar o al sellar, sin dejar nada guardado
  (FR-004). Su `abort` queda registrado con el motivo, también cuando el fallo lo detecta la CLI.
- **Sustitución:** la prueba cuenta en el contador común. La frase de la CA no cuenta (Suposiciones de la spec).

## Tokens, identidades y certificados

| `op` | `args` | `data` si va bien |
|---|---|---|
| `tokens.create` | `identity` | `token` (se muestra una sola vez), `token_id`, `identity`, `expires_at` |
| `tokens.list` | — | `tokens`: lista de `{token_id, identity, expires_at}` de los pendientes, sin su valor |
| `tokens.annul` | `token_id` | `token_id` |
| `identities.list` | — | `identities`: lista de `{identity, status, certificate_not_after}` |
| `identities.get` | `identity` | `identity`, `status`, `certificates`: lista de `{serial, not_before, not_after, state, renewable}` |
| `identities.disable` | `identity`, `urgent` (booleano, por defecto `false`) | `identity`, `revoked` (series revocadas con urgencia) |
| `identities.enable` | `identity` | `identity` |
| `certificates.revoke` | `serial`, `urgent` (booleano, por defecto `false`) | `serial`, `reissued` (la serie nueva si era el certificado vigente de Hécate; si no, `null`) |

- **Órdenes repetidas:** una orden que no cambia nada (deshabilitar una identidad deshabilitada, habilitar una
  habilitada, revocar un certificado revocado o caducado, anular un token usado, anulado o caducado) se acepta, no
  genera eventos y deja su fila (FR-026). Solo un token que nunca existió da `not_found`. La
  urgencia sí es un cambio: `urgent` sobre una revocación o una deshabilitación pasivas las hace urgentes, y `revoked`
  trae solo las series revocadas ahora.
- **Identidad sin alta:** la que solo tiene tokens no existe para `identities.get`, `identities.disable` ni
  `identities.enable` (`not_found`); sus tokens se gestionan con `tokens.list` y `tokens.annul`.

## Errores nuevos por operación

| Código | Cuándo |
|---|---|
| `validation` | Trust domain o identidad mal formados; identidad reservada en `tokens.create`; frase de la CA de menos de 12 caracteres o igual a la de desbloqueo; certificados que no cumplen el perfil; una CA raíz de sustitución igual a la vigente; revocación pasiva del certificado de Hécate; deshabilitar `hecate` |
| `auth` | Prueba de `ca.replace.begin` que no vale (cuenta en el contador común) |
| `rate_limited` | Espera pendiente del contador común en `ca.replace.begin` |
| `conflict` | Hécate sellada; ya hay CA (`ca.create.begin`) o no la hay; otra ceremonia en curso; ceremonia desconocida o caducada; token para una identidad deshabilitada; token con la intermedia vigente caducada |
| `not_found` | Identidad (también la que solo tiene tokens), token o serie que no existen |

## `status`

Añade el bloque `ca` (FR-025), que también responde sellada:

```json
{ "ca": { "exists": true, "trust_domain": "cerbero.home", "root_fingerprint": "sha256:...",
          "hecate_certificate": { "serial": "5f1c...", "not_after": "2026-11-12T12:00:00+00:00" },
          "intermediate_days_left": null, "identities_expiring_24h": 0, "suspicious_renewals_24h": 0,
          "enrollment_throttled": false } }
```

- `hecate_certificate` es el certificado vigente de Hécate: con su serie, el admin lo revoca con urgencia
  (`certificates.revoke`).
- `intermediate_days_left` solo trae un número cuando quedan menos de 30 días, y 0 si ya caducó.
- Sin CA, solo aparece `exists: false`.

## Secretos

`secrets.create` y `secrets.add_version` rechazan con `validation` una ruta cuya identidad es `hecate` o `admin`
(FR-027). Revocar y destruir lo que ya existiera sigue igual.
