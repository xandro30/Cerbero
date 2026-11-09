# Contrato: CLI `hecate`

La CLI conserva su nombre, sus órdenes, sus opciones, sus códigos de salida y los textos que comprueban las pruebas y los
quickstarts de 001 y 002 (FR-004). Solo cambia cómo se lanza: con el binario `hecate`, no con
`uv run --package cerbero-hecate hecate`. Los textos de ayuda los genera clap y pueden cambiar.

## Órdenes

| Orden | Opciones | Operaciones del canal |
|---|---|---|
| `hecate serve` | — | Arranca el servicio, siempre sellado |
| `hecate status` | — | `status` |
| `hecate init` | — | `init.begin`, `init.confirm` e `init.abort` |
| `hecate unseal` | `--shares` | `unseal` |
| `hecate seal` | — | `seal` |
| `hecate passphrase change` | `--with-shares` | `passphrase.change` |
| `hecate shares rekey` | `--with-shares` | `shares.rekey.begin`, `shares.rekey.confirm` y `shares.rekey.abort` |
| `hecate secrets create <ruta>` | `--type {string,json,pem}` (obligatoria), `--from-file`, `--max-versions`, `--rotate-every`, `--expires-at` | `secrets.create` |
| `hecate secrets list` | `--identity` | `secrets.list` |
| `hecate secrets get <ruta>` | — | `secrets.get` |
| `hecate secrets put <ruta>` | `--from-file` | `secrets.add_version` |
| `hecate secrets revoke <ruta>` | `--version` (obligatoria) | `secrets.revoke` |
| `hecate secrets restore <ruta>` | `--version` (obligatoria) | `secrets.restore` |
| `hecate secrets promote <ruta>` | `--version` (obligatoria) | `secrets.promote` |
| `hecate secrets destroy <ruta>` | `--version` (obligatoria), `--with-shares` | `secrets.destroy` |

La frase, las partes y los valores se piden sin eco; las partes se muestran una sola vez al inicializar y al repartir de
nuevo, y se confirman escribiendo dos de ellas.

## Salidas y códigos de salida

| Código | Cuándo | Salida |
|---|---|---|
| 0 | La operación termina bien | Las mismas líneas que la versión en Python en stdout (por ejemplo, `state: sealed` o `created <ruta> version <n> fingerprint <huella>`) |
| 1 | La operación falla, se cancela o el canal no responde | En stderr: `error (<código>): <mensaje>`, más ` (retry in <n>s)` si hay espera y ` [state: <estado>]` si la respuesta trae el estado; o `error: <mensaje>` si no hay canal |
| 2 | Falta configuración (`HECATE_DATA_DIR`) o los argumentos no son válidos | En stderr: `error: <mensaje>` o el error de uso |

`serve` termina con 0 al parar de forma ordenada.
