# 0013 — Acceso por app guardado en Hécate

Estado: aceptada · Fecha: 2026-11-15

## Contexto

La feature 008 abre la API de Hermes a los componentes de Cerbero. Autenticar a quien llama ya está resuelto: mTLS con
su certificado y un token corto firmado con su clave de firma de llamadas ([ADR 0011](0011-pila-hermes.md)). Falta
decidir quién puede llamar a Hermes, y en general a cada app, y dónde se guarda esa lista. El propietario lo decidió al
aclarar la feature, el 2026-11-14. Lo condicionan:

- el token corto lo firma quien llama: lo que diga sobre lo que puede hacer no prueba nada, porque se lo concedería a sí
  mismo;
- Hécate ya es la fuente de las identidades de Cerbero: las da de alta, las renueva, las revoca y publica sus claves de
  firma de llamadas;
- el principio I: cada app evoluciona por separado, y una lista de acceso repetida en cada una divergiría;
- los usuarios, los roles y el perfilado de las personas son del frontal de las apps, Ténaro: entre componentes solo hay
  validación interna;
- Hermes no tiene funciones peligrosas entre sí: quien puede enviar un mensaje puede consultar sus capacidades.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Una lista central en Hécate de qué identidades pueden llamar a cada app** | Un solo sitio para conceder y retirar, junto a las identidades; cada app consulta lo mismo | Hécate guarda una política más, y cada app tiene que consultarla |
| Permisos en el token corto | Sin consulta | Los firma quien llama: no concede nada que no se dé a sí mismo |
| Una lista en cada app | Sin dependencia de Hécate | Tantas listas como apps que mantener, y nada que impida que diverjan |
| Permisos por función | Más fino | Hermes no los necesita hoy, y multiplica lo que hay que conceder y revisar |

## Decisión

- **Hécate guarda qué identidades pueden llamar a cada app,** de forma central. El acceso es a la app entera, sin
  permisos por función.
- **El token corto no concede ningún acceso:** lleva `iss`, `aud`, `exp`, `jti` y, si quien llama lo pone, el contexto
  de auditoría en `cerbero_audit`. Quien recibe la llamada autentica con el certificado y el token, y decide el acceso
  con la lista de Hécate.
- **La lista de Hécate y su consulta desde cada app las construye la feature 009.** Hasta entonces, Hermes autentica a
  quien llama y no comprueba el acceso: cualquier componente de Cerbero con un certificado válido puede llamarlo.
- Los permisos de las personas no entran aquí: son del frontal (Ténaro).

## Consecuencias

- El token corto deja de llevar permisos por tipo de servicio y operación ([ARCHITECTURE](../../ARCHITECTURE.md#llamadas-entre-componentes)).
- Conceder o retirar el acceso a una app es un cambio en Hécate, sin tocar la app ni a quien llama.
- Hasta la 009, el acceso a Hermes depende solo de tener un certificado de Cerbero, que solo emite Hécate con un token de
  alta del admin. Queda en [DECISIONS](../../DECISIONS.md#abiertas) como diseñado y no implementado.
- Si una app llega a necesitar permisos por función, se añaden a esta lista sin cambiar el token.
