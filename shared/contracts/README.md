# Contratos publicados de Cerbero: esquema UDM, sobre común y API Contracts

Único código compartido entre proyectos: contratos, nunca dominio (ver [ARCHITECTURE](../../ARCHITECTURE.md#1-principios)).

## OpenAPI

Los OpenAPI 3.1 de las APIs entre componentes viven en JSON dentro del módulo, en
[`src/cerbero_contracts/openapi/`](src/cerbero_contracts/openapi/), para que viajen con el paquete:

| Fichero | Qué fija |
|---|---|
| `common.v1.json` | El sobre común (`status`, `data`, `error` con `code`, `message` y `retryable`, `meta`) y los nueve códigos de error cerrados. No fija estados HTTP: cada API fija los suyos |
| `hecate.v1.json` | La API de Hécate: alta, renovación, lista de revocación, claves de firma de llamadas publicadas, resolución de secretos, estado agregado y capacidades, con sus estados HTTP |

Las pruebas de contrato de cada componente validan sus respuestas, también las de error, contra estos ficheros.
