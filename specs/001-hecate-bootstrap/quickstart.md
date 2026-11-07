# Quickstart: Arranque de Hécate

Guía para comprobar de principio a fin que la feature funciona. El contrato está en
[contracts/control-protocol.md](contracts/control-protocol.md) y el modelo en [data-model.md](data-model.md).

## Requisitos

- Repositorio en la rama `feature/001-hecate-bootstrap` con `uv sync --all-packages`.
- Un directorio de datos vacío para la prueba:

```bash
export HECATE_DATA_DIR="$(mktemp -d)"
```

Opcional, para ir más rápido en el recorrido: `export HECATE_KDF_PROFILE=moderate` (por defecto es `sensitive`, que tarda decenas de segundos por derivación; ver [research.md](research.md), R2).

## Pruebas automáticas

```bash
uv run --package cerbero-hecate pytest hecate/tests
```

Debe pasar todo: unitarias, integración (SQLite, criptografía, socket) y contrato.

## Recorrido manual

En una terminal, arrancar el proceso:

```bash
uv run --package cerbero-hecate hecate serve
```

En otra terminal (con el mismo `HECATE_DATA_DIR`):

| Paso | Comando | Resultado esperado |
|---|---|---|
| 1 | `hecate status` | `uninitialized` |
| 2 | `hecate init` | Pide la frase dos veces, avisa de que perder frase y dos partes es irrecuperable, muestra 3 partes y pide confirmar 2; termina en `sealed` |
| 3 | `hecate unseal` (frase correcta) | `unsealed` |
| 4 | `hecate seal` | `sealed` |
| 5 | `hecate unseal` tres veces con una frase incorrecta | El 3.er fallo provoca una espera de 1 minuto; un 4.º intento inmediato responde con la espera pendiente |
| 6 | Parar y arrancar `hecate serve`; `hecate status` | `sealed` y la espera pendiente se mantiene |
| 6b | Esperar 1 minuto (la espera del paso 5) | `hecate status` ya no muestra espera |
| 7 | `hecate unseal --shares` con las partes 1 y 3 | `unsealed` y recomienda cambiar la frase |
| 8 | `hecate passphrase change --with-shares` con dos partes como prueba | Frase nueva; tras reiniciar, la anterior falla y la nueva desbloquea |
| 9 | `hecate shares rekey` | Partes nuevas; una parte antigua se rechaza como de otro juego |
| 10 | `hecate unseal --shares` con un carácter cambiado en una parte | Aviso de parte no válida sin contar como intento |

## Comprobaciones de seguridad

- Buscar la frase y una parte en `${HECATE_DATA_DIR}` y en los logs del proceso: no deben aparecer (SC-006).
- Matar el proceso durante `hecate init` antes de confirmar: tras arrancar, `status` es `uninitialized` (SC-008).
