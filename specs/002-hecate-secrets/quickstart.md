# Quickstart: Secretos de Hécate

Guía para comprobar la feature de principio a fin. Contrato en [contracts/control-protocol.md](contracts/control-protocol.md)
y modelo en [data-model.md](data-model.md).

## Requisitos

- Rama `feature/002-hecate-secrets` con `uv sync --all-packages`.
- Una Hécate inicializada y desbloqueada (ver el [quickstart de la feature 001](../001-hecate-bootstrap/quickstart.md)):

```bash
export HECATE_DATA_DIR="$(mktemp -d)" HECATE_KDF_PROFILE=moderate
```

## Pruebas automáticas

```bash
uv run --package cerbero-hecate pytest hecate/tests
```

Incluye la resolución por identidad, que no tiene canal todavía: se prueba como caso de uso.

## Recorrido manual

Con `hecate serve` en marcha, inicializado y desbloqueado:

| Paso | Comando | Resultado esperado |
|---|---|---|
| 1 | `hecate secrets create secret://hermes/telegram/family_bot/bot_token --type string` | Pide el valor sin mostrarlo; versión 1 y su huella |
| 2 | `hecate secrets create secret://hermes/certs/proxy --type pem --from-file proxy.pem` | Versión 1; con un fichero que no es PEM, error de validación |
| 3 | `hecate secrets list --identity hermes` | Las dos rutas, sin valores |
| 4 | `hecate secrets put secret://hermes/telegram/family_bot/bot_token` | Versión 2 activa; la 1 `deprecated` |
| 5 | `hecate secrets get secret://hermes/telegram/family_bot/bot_token` | Versiones con estado, fecha y huella; ningún valor |
| 6 | `hecate secrets revoke … --version 1` | La 1 pasa a `revoked` |
| 7 | `hecate secrets destroy … --version 1` | Pide la frase; con una incorrecta, `auth` y sigue `revoked`; con la correcta, `destroyed` |
| 8 | `hecate seal` y `hecate secrets list` | `conflict`: está sellada |
| 9 | `hecate status` | Incluye los recuentos de caducados y por caducar |

## Comprobaciones de seguridad

- Buscar los valores escritos en `${HECATE_DATA_DIR}` y en los logs: no deben aparecer (SC-002).
- Tras destruir una versión, ni restaurando la copia de `hecate.db` hecha después se recupera su valor (SC-004).
