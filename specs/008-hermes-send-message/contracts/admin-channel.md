# Canal local de administración

HTTP/1.1 sobre el socket Unix `HERMES_ADMIN_DIR/admin.sock`, con el directorio en 0700 y el socket en 0600. Su único
cliente es la CLI `hermes` ([cli.md](cli.md)). Usa el sobre y los códigos comunes, sin API Contract publicado,
`/capabilities` ni `Idempotency-Key`; su seguridad viene del acceso local (principio II). Envoy no monta ese directorio.

- Cuerpo en JSON estándar UTF-8, de 8 MiB como mucho, porque `config.plan` y `config.apply` llevan el fichero entero (de
  1 MiB como mucho) dentro de un texto JSON; con más, o con un JSON que no es estándar, `validation`.
- Cada operación deja una línea de log con su nombre y su resultado, nunca con el token de alta, los códigos ni
  direcciones. La de `health`, que el healthcheck del contenedor pide cada 10 s, va a nivel `debug` mientras Hermes está
  listo, y a `info` con el motivo cuando no lo está.
- Las que cambian algo dejan su entrada de auditoría con `actor: admin` (FR-029): `config.apply` y `contacts.code`. El
  alta queda en el log.
- Responden mientras Hermes está en marcha, también sin estar listo; las que necesitan la base o Hécate responden
  `unavailable` si no las tienen.

## Operaciones

| Operación | Ruta | Cuerpo | `data` de la respuesta | Errores |
|---|---|---|---|---|
| `identity.enroll` | `POST /identity/enroll` | `{"token": "HET1...."}` | `spiffe_id`, `serial` y `not_after` | `validation` (el token no tiene el formato `HET1`); `auth` (Hécate no tiene la huella del token, no es `hecate`, o rechaza el token); `conflict` (Hermes ya tiene una identidad válida); `unavailable` (Hécate sellada o caída) |
| `config.plan` | `POST /config/plan` | `{"yaml": "<texto del fichero>"}` | `changes` y `plan_digest` | `validation`, con la lista de fallos en `data.errors`; `unavailable` (la base, o Hécate para comprobar los secretos) |
| `config.apply` | `POST /config/apply` | `{"yaml": "<texto del fichero>", "plan_digest": "sha256:..."}` | `changes` aplicados | `validation`; `conflict` (la base cambió desde el plan); `unavailable` |
| `config.export` | `GET /config/export` | — | `yaml`, el texto del fichero | `unavailable` |
| `contacts.code` | `POST /contacts/code` | `{"contact": "contacto_demo", "instance": "family_bot"}` | `code` (`XXXX-XXXX`) y `expires_at` | `not_found` (el contacto, o su canal en esa instancia); `unavailable` |
| `health` | `GET /health` | — | `live` (siempre `true`), `ready` y, si no está listo, `reason`: `needs a new enrollment token`, `database unavailable` o `configuration not loaded` | — |

### Cambios

`changes` es una lista ordenada (primero lo que se borra, después lo que cambia y al final lo que se crea), cada cambio
con:

| Campo | Valores |
|---|---|
| `action` | `create`, `update` o `delete` |
| `kind` | `variable`, `instance`, `template_version`, `contact` o `channel` |
| `name` | El nombre: `family_bot`, `device_unknown v2`, `contacto_demo telegram/family_bot`... |
| `detail` | Qué cambia, sin valores de secretos ni direcciones; en un canal que se borra o cambia de instancia, si pierde su verificación |

`plan_digest` es el SHA-256 de la configuración del fichero y de la que había en la base al calcular el plan. Una
aplicación con otra huella responde `conflict` sin aplicar nada.

### Errores de validación

`data.errors` es una lista de `{"path": "...", "message": "..."}`, con la ruta dentro del fichero (por ejemplo,
`instances[0].secrets.bot_token`) y el motivo, sin valores. `error.message` dice cuántos hay.
