# Contrato: órdenes nuevas de la CLI `hecate`

Órdenes que añade esta feature. Las de 001 y 002, los códigos de salida y el formato de los errores siguen como en el
[contrato del cambio 005](../../005-hecate-rust/contracts/cli.md). Las frases se piden sin eco.

## Órdenes

| Orden | Opciones | Operaciones del canal |
|---|---|---|
| `hecate ca create` | `--trust-domain <td>` y `--out <ruta>` (obligatorias) | `ca.create.begin`, `ca.create.confirm` y `ca.create.abort` |
| `hecate ca renew-intermediate` | `--root-key <ruta>` (obligatoria) | `ca.intermediate.begin`, `ca.intermediate.confirm` y `ca.intermediate.abort` |
| `hecate ca replace` | `--out <ruta>` (obligatoria), `--with-shares` | `ca.replace.begin`, `ca.replace.confirm` y `ca.replace.abort` |
| `hecate tokens create <identidad>` | — | `tokens.create` |
| `hecate tokens list` | — | `tokens.list` |
| `hecate tokens annul <token-id>` | — | `tokens.annul` |
| `hecate identities list` | — | `identities.list` |
| `hecate identities get <identidad>` | — | `identities.get` |
| `hecate identities disable <identidad>` | `--urgent` | `identities.disable` |
| `hecate identities enable <identidad>` | — | `identities.enable` |
| `hecate certificates revoke <serie>` | `--urgent` | `certificates.revoke` |

## Ceremonias

`hecate ca create`:

1. Pide la frase de la CA y llama a `ca.create.begin`, que tarda lo que una pasada de Argon2id.
2. Genera la CA raíz, firma la intermedia y escribe la copia protegida en `--out`, con permisos `0600`. Si el fichero
   existe, no lo sobrescribe y termina con error.
3. Escribe `root key saved to <ruta>; store it on two separate media` y pide
   `path of a saved copy to verify:` y otra vez la frase. Abre la copia y compara su clave con la de la CA raíz.
4. Si coincide, llama a `ca.create.confirm` e imprime `created ca for <td>` y `root fingerprint sha256:<hex>`.
5. Si no coincide, llama a `ca.create.abort` con `backup_check_failed`, borra la copia que escribió y termina con
   `error: the saved copy does not open with that passphrase`. No se crea nada.

Las otras dos ceremonias:

- **`hecate ca renew-intermediate`:** abre la copia de `--root-key` con la frase, comprueba que es la clave de la CA raíz
  que devuelve `ca.intermediate.begin` y firma. Imprime `renewed intermediate version <n> until <instante>`. Una copia
  que no se abre (frase incorrecta o copia de otra CA, que no se distinguen) llama a `abort` con `wrong_passphrase`, y
  una que se abre con otra clave, con `wrong_root_key`. Si no puede leer `--root-key`, termina con error antes de
  empezar la ceremonia.
- **`hecate ca replace`:** pide la prueba (la frase, o dos partes con `--with-shares`) y la frase nueva de la CA. Sigue
  como `ca create` e imprime `replaced ca for <td> generation <n>` y la huella nueva.

Cancelar con Ctrl-C o fin de entrada llama a `abort` con `cancelled` y termina con `cancelled` y código 1, como en el
cambio 005. La copia que ya se escribió se borra, porque su CA no llega a existir. La clave de la CA raíz y las frases
se borran de la memoria al terminar, también si falla (FR-028).

## Salidas

| Orden | Salida si va bien (stdout) |
|---|---|
| `tokens create` | `token <token>`, `id <token-id> identity <identidad> expires <instante>` y el aviso `this token is shown only once` |
| `tokens list` | Una línea por token: `<token-id> <identidad> expires <instante>`, o `no pending tokens` |
| `tokens annul` | `annulled <token-id>` |
| `identities list` | Una línea por identidad: `<identidad> <estado> certificate until <instante>`, o `no identities` |
| `identities get` | `identity <identidad> <estado>` y una línea por certificado: `<serie> <estado> renewable <yes/no> <desde> <hasta>` |
| `identities disable` / `enable` | `disabled <identidad>` (con `revoked <n> certificates` si es urgente) / `enabled <identidad>` |
| `certificates revoke` | `revoked <serie>` (con `urgent` si lo es) y, con el de Hécate, `reissued <serie nueva>` |
| `status` | Añade, después de las líneas que ya imprime, las del bloque `ca`: `ca: <td> sha256:<hex>` o `ca: none`, `hecate certificate <serie> until <instante>`, `intermediate days left: <n>` si quedan menos de 30 (0 si ya caducó), `identities expiring within 24h: <n>`, `suspicious renewals within 24h: <n>` y `enrollment throttled: <yes/no>` |

Los errores siguen el formato del cambio 005: `error (<código>): <mensaje>` en stderr y código 1.
