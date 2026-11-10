# Contratos de la feature 003

| Contrato | Quién lo usa | Fichero |
|---|---|---|
| API entre componentes: alta, renovación, lista de revocación, claves publicadas y capacidades | Cada componente, contra Hécate (principio II) | [api.md](api.md); al implementar, `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json` |
| Sobre común y los nueve códigos de error de las APIs, sin estado HTTP | Todas las APIs entre componentes | [api.md](api.md#sobre-y-errores) y [research R11](../research.md#r11-contrato-de-la-api-y-sobre-común); al implementar, `shared/contracts/src/cerbero_contracts/openapi/common.v1.json` |
| Operaciones nuevas del canal local | La CLI de Hécate | [control-protocol.md](control-protocol.md) |
| Órdenes nuevas de la CLI | El admin | [cli.md](cli.md) |
| Formatos: token de alta, copia de la clave de la CA raíz, perfiles de certificado, CRL, claves publicadas, token corto y huella | Hécate, la CLI y los componentes | [formats.md](formats.md) |

- Los contratos del canal de las features 001 y 002 y el formato `HEC2` siguen como los dejó el cambio 005. Esta feature
  solo añade operaciones, el bloque `ca` de `status` y el rechazo de FR-027 en `secrets.create` y
  `secrets.add_version`.
- Los OpenAPI se escriben en JSON (OpenAPI 3.1) en `shared/contracts/src/cerbero_contracts/openapi/`, dentro del
  paquete `cerbero-contracts`, el único sitio compartido entre componentes (principio I), con las pruebas de contrato
  que validan cada respuesta ([research R11](../research.md#r11-contrato-de-la-api-y-sobre-común)).
- Instantes: ISO 8601 extendido con desfase y microsegundos, como en el canal (`2026-11-09T12:00:00.000000+00:00`).
