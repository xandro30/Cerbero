# Contratos de la feature 008

| Contrato | Qué fija | Se publica en |
|---|---|---|
| [api.md](api.md) | La API de Hermes entre componentes: `chat.send_message`, las capacidades y la prueba de conexión, con su autenticación, su sobre y sus errores | `shared/contracts/src/cerbero_contracts/openapi/hermes.v1.json`, y los campos nuevos de `meta` en `common.v1.json` 1.1.0 |
| [proxy.md](proxy.md) | Lo que hace Envoy delante de Hermes y el formato de la cabecera que Hermes lee | Aquí y en `hermes/deploy/envoy/envoy.yaml`; al cerrar, en docs/hermes.md |
| [admin-channel.md](admin-channel.md) | Las operaciones del canal local de administración | Aquí y, al cerrar, en docs/hermes.md |
| [cli.md](cli.md) | Las órdenes de la CLI `hermes`, con su salida y su código de salida | Aquí y, al cerrar, en docs/hermes.md |
| [config-file.md](config-file.md) | El formato del fichero de configuración y sus reglas | Aquí y, al cerrar, en docs/hermes.md |
| [telegram.md](telegram.md) | La librería de Telegram: su manifiesto, cómo usa la Bot API, cómo dibuja el mensaje y cómo traduce los errores | `hermes/src/hermes/infrastructure/providers/telegram/manifest.json`; al cerrar, en docs/hermes.md |
| [fake.md](fake.md) | El proveedor fake de las pruebas: su manifiesto y sus bandejas de salida y de entrada, para mirarlo desde fuera de Hermes | Aquí y en las pruebas de Hermes |
| [conformance.md](conformance.md) | La batería de conformidad del tipo `chat` que pasan el fake y Telegram | Aquí y en las pruebas de contrato de Hermes |

La autenticación de cada llamada sigue la [guía de integración de Hécate](../../../docs/hecate/integracion.md): el
mismo certificado y el mismo token corto, con `aud` igual al SPIFFE ID de Hermes.
