# Contratos del cambio 005

La migración conserva los contratos de Hécate; solo cambia el formato de las partes de recuperación.

| Contrato | Dónde está | ¿Cambia? |
|---|---|---|
| Canal local: transporte, petición, sobre, códigos de error y operaciones de sellado | [contrato de 001](../../001-hecate-bootstrap/contracts/control-protocol.md) | No: se exige el «JSON UTF-8» del contrato y se rechaza el que no es estándar (FR-018) |
| Canal local: operaciones de secretos y `status` ampliado | [contrato de 002](../../002-hecate-secrets/contracts/control-protocol.md) | No; FR-018 exige el formato estándar en `json`, `pem` y `expires_at` |
| CLI `hecate` | [cli.md](cli.md) | Solo cómo se lanza |
| Configuración (`HECATE_DATA_DIR`, `HECATE_SOCKET_PATH`, `HECATE_KDF_PROFILE`) | [plan.md](../plan.md), Technical Context | No |
| Partes de recuperación | [recovery-share.md](recovery-share.md) | Sí: de `HEC1` a `HEC2` |

Las specs de 001 y 002 están fusionadas y no se editan; las pruebas de contrato trasladadas comprueban que sus contratos
se siguen cumpliendo.
