# Research: Lista de acceso por app

Decisiones técnicas de la feature 009. Cada una dice qué se decidió, por qué y qué se descartó. Los nombres concretos
de operaciones, eventos y ficheros se fijan aquí y en los [contratos](contracts/README.md).

## R0. Decisiones del propietario

- **Al aclarar la spec (2026-11-15):**
  - el acceso se deniega por defecto y la feature no deja ningún acceso concedido de partida;
  - la lista es entre apps y solo admite identidades de app; una identidad hija cuenta como su app, y lo que alcanza
    dentro de la app de destino es perfilado, fuera de esta feature;
  - una concesión vale en la llamada siguiente y una retirada en 240 s como mucho;
  - sin ninguna copia de su lista, Hermes no está listo;
  - se puede conceder el acceso a una identidad que aún no se ha dado de alta.
- **Al revisar el plan (2026-11-16):**
  - la lista de cada app es un aggregate nuevo, App Access, y no una parte de la Access Policy (R1);
  - como las identidades hijas no están construidas, su regla queda como regla de diseño (R10).
- **El orden:** la 009 va antes que la reestructuración de la documentación de Hermes, que será la 010.

## R1. El aggregate App Access

**Decisión:** un aggregate event-sourced por app, en el stream `access/<app>` del event store de las identidades, sin
tablas propias.

- **Eventos:** `CallerGranted` y `CallerRevoked`, con el instante, la identidad que llama y quién hizo el cambio
  (`by: admin`). Ningún evento lleva un secreto.
- **Estado:** las identidades que pueden llamar a la app, en un conjunto ordenado, la versión (cuántos eventos tiene su
  stream) y el historial. Una app sin stream tiene la lista vacía en la versión 0.
- **Reglas:**
  - la app y quien llama son nombres de identidad de Hécate, de un solo segmento (R10), y ninguno de los dos puede ser
    `hecate` ni `admin`;
  - una app no recibe acceso a sí misma;
  - conceder lo que ya está concedido, o retirar lo que no lo está, no escribe ningún evento;
  - se puede conceder a una identidad, o para una app, que aún no se ha dado de alta (FR-005).
- **El pasado:** aplicar los eventos hasta un instante da quién podía llamar a la app entonces (SC-006), como en la
  Access Policy.

**Alternativas:**

- La Access Policy de la app (descartada por el propietario): su versión se movería también con las lecturas y el
  permiso de estado, y mezclaría quién entra en la app con lo que ella puede usar.
- La Access Policy de quien llama: «quién puede llamar a esta app» obligaría a recorrer todas las políticas, y la lista
  de una app no tendría una versión propia.
- Una tabla de permisos: pierde el historial, o hay que llevarlo aparte.

## R2. Órdenes y consultas del admin

**Decisión:** cuatro operaciones del canal local y su orden de la CLI, como las de la Access Policy de la 006.

| Canal local | CLI | Qué hace |
|---|---|---|
| `access.grant` | `hecate access grant <identidad> <app>` | Concede a la identidad el acceso a la app |
| `access.revoke` | `hecate access revoke <identidad> <app>` | Se lo retira |
| `access.list` | `hecate access list` | Las apps con algún cambio, con su versión y cuántas identidades las llaman |
| `access.get` | `hecate access get <app>` | La lista vigente de la app y su historial |

- La identidad va antes que la app, como en `hecate policies grant-read <identidad> <ruta>`.
- Las órdenes no llevan versión esperada. Una que no cambia nada responde `changed: false`, sin eventos.
- Con Hécate sellada, las cuatro responden `conflict`, como el resto del canal.
- Cada operación deja su fila en el registro de accesos, también si no cambia nada o se rechaza (R6).

**Alternativas:** gestionar las listas por la API de red (FR-002 y la constitución lo impiden: la gestión es del admin
y va por el canal local); exigir una versión esperada, que en la 006 se descartó porque las órdenes son idempotentes.

## R3. La consulta de una app

**Decisión:** `GET /v1/callers`, con `operationId` `listCallers`, como `listRevocations` y `listKeys`: lo que Hécate
publica para quien pregunta.

- **Autenticación:** el certificado de la conexión y un token corto, como el resto de la API.
- **La app es quien pregunta:** la operación no tiene parámetros, así que no hay forma de pedir la lista de otra
  (FR-006).
- **Respuesta:** la app, las identidades que pueden llamarla, ordenadas, y la versión de su lista. Una lista vacía es
  una respuesta como cualquier otra.
- **Sellada:** `unavailable`, como todas las operaciones salvo el estado; las apps siguen con su copia.
- **Filas:** atendida no deja ninguna, como el estado, para que el refresco de las apps no llene el registro; un
  rechazo deja la suya (`callers_list`).
- **Capacidades:** `GET /v1/capabilities` lista también `callers`.

**Alternativas:** `GET /v1/access/{app}`, que dejaría pedir la lista de otra app; responder también sellada, como el
estado, que rompe la regla de que solo el estado responde sellada sin ganar nada, porque las apps tienen su copia; o
meter la lista en la respuesta de `GET /v1/keys`, que mezcla dos cosas que cambian por motivos distintos.

## R4. La copia en Hermes

**Decisión:** la lista de Hermes es una publicación más de Hécate, junto a la lista de revocación y las claves
publicadas.

- **Refresco:** cada 240 s, en el mismo refresco que ya existe (`refresh_publications`), con una llamada más a Hécate.
- **Disco:** se guarda en el directorio de estado, en `access.json` (0600), con la app, las identidades, la versión y
  cuándo se obtuvo, y se vuelve a leer al arrancar.
- **Arranque:** el paso de las publicaciones exige también la lista: con una copia guardada, arranca con ella; sin
  ninguna, Hécate tiene que darla, y mientras no la da Hermes no está listo (FR-009).
- **Salud:** sin copia, `hermes health` dice `not ready: access list not loaded`. La salud mira los motivos en el
  orden del arranque: identidad, lista de acceso, base y configuración.
- **Hécate sellada o caída:** el refresco falla y lo deja en el log (`publications.refresh_failed`), y Hermes sigue
  con la última copia.

**Alternativas:** un refresco aparte para la lista, con el mismo ritmo y los mismos fallos que el de las
publicaciones; o preguntar a Hécate en cada llamada, que el propietario descartó al aclarar por la latencia y por la
dependencia de Hécate en cada petición.

## R5. La comprobación en Hermes

**Decisión:** un paso más de `CallAuthenticator`, después del certificado y del token y antes de guardar el `jti`.

- **Quien está en la copia** sigue como hasta ahora.
- **Quien no está:** Hermes vuelve a pedir la lista a Hécate antes de responder. Las búsquedas que llegan a la vez
  comparten la misma petición, como la de una clave que no tiene (T113 de la 008), y la lista nueva sustituye a la
  copia y se guarda. Si sigue sin estar, `auth`; si Hécate no responde, `unavailable`.
- **El mismo mensaje:** el rechazo es `auth` con el mensaje de cualquier otro fallo de autenticación. Solo el log dice
  el motivo, `not_allowed`, con el `request_id` de la petición.
- **Todas las operaciones de la API de red** pasan por la dependencia de autenticación, así que todas comprueban el
  acceso. El canal local no.
- **Antes del `jti`:** quien no tiene acceso no deja fila en la tabla de los `jti`.

**Alternativas:**

- Comprobar después del `jti`: cada llamada rechazada dejaría una fila para nada.
- Recordar a quien no tiene acceso durante un tiempo: una concesión no valdría en la llamada siguiente.
- Un intervalo mínimo entre búsquedas: una concesión hecha justo después de una búsqueda tendría que esperar. Solo
  puede provocar una búsqueda un componente de Cerbero con un certificado y un token válidos, y nunca hay más de una a
  la vez.

## R6. Registro de accesos

**Decisión:** cinco operaciones nuevas, sin migración, porque la columna no tiene `CHECK` (como en la 006):

- `access_grant`, `access_revoke`, `access_list` y `access_get`, de las órdenes del admin, con `subject: admin`, la app
  como identidad y la identidad que llama en el detalle;
- `callers_list`, de los rechazos de la consulta de la API.

## R7. Contratos y versiones

**Decisión:**

- **`hecate.v1.json`** gana `listCallers`, con su esquema de datos y sus respuestas 200, 401, 500 y 503, y pasa a la
  1.1.0, porque solo añade. La prueba de contrato de Hécate fija esa versión.
- **`hermes.v1.json`** no cambia sus operaciones. Las descripciones del 401 y del 503 dicen ya que quien no tiene
  acceso recibe `auth` y que Hermes responde `unavailable` sin su lista o si Hécate no responde al buscar a quien
  llama, y pasa a la 1.0.1.
- **`common.v1.json`** no cambia.

## R8. Pruebas

**Decisión:**

- **Hécate:** el dominio (las reglas del aggregate, con proptest sobre secuencias de órdenes), la aplicación (las
  órdenes, Hécate sellada, las órdenes sin cambio y sus filas), el canal local, la CLI con assert_cmd, la API con el
  componente de prueba (la lista, sellada, los rechazos con fila y la consulta atendida sin fila), el contrato, y la
  suite entera en macOS y en el contenedor `rust:1.99.1-bookworm`.
- **Hermes:**
  - unitarias: quien está entra; quien no está recibe `auth` tras volver a pedir la lista; una concesión vale en la
    llamada siguiente; Hécate sin responder al buscar da `unavailable`; las búsquedas a la vez comparten una petición;
    lo que diga el token no cuenta; la copia se refresca, se guarda y se lee al arrancar; el motivo nuevo de la salud;
  - contra una Hécate real: la consulta y el fichero de la copia;
  - API y E2E: el componente de prueba con acceso envía, otro sin acceso recibe `auth` sin que salga nada, y una
    retirada vale en el siguiente refresco;
  - la prueba de fugas y la comprobación del contrato siguen pasando.

## R9. Consolidación (FR-013)

- **docs/hecate/acceso.md:** el acceso por app construido: el aggregate, las órdenes, la consulta, denegar por defecto
  y la regla de las identidades hijas.
- **docs/hecate.md:** los seis aggregates y el párrafo del acceso.
- **docs/hecate/integracion.md:** la operación nueva en su tabla, y cómo la usa una app (copia, refresco y búsqueda
  ante quien no está).
- **docs/hecate/cli.md** y **docs/hecate/auditoria.md:** las órdenes `hecate access` y las cinco operaciones del
  registro.
- **docs/hermes.md:** la comprobación en la autenticación, la lista entre las publicaciones, el motivo nuevo de la
  salud y, en la operación, que el admin concede antes los accesos.
- **ARCHITECTURE:** los seis aggregates de Hécate y la línea de las llamadas entre componentes: quien recibe la llamada
  decide con la lista de Hécate.
- **GLOSSARY:** el término nuevo, App Access.
- **DECISIONS:** «Acceso por app» pasa a construido; la fila de las identidades hijas gana que en la lista cuentan como
  su app; y la referencia del principio VII para la autorización entre servicios.
- **README:** lo que ya hacen Hécate y Hermes, en «Estado del Proyecto».

## R10. Identidades hijas

**Contexto:** las identidades hijas están diseñadas, no construidas (DECISIONS). Hécate solo admite nombres de un
segmento (`[a-z][a-z0-9-]{0,62}`), y Hermes solo acepta un SPIFFE ID con un segmento, así que en la 009 ninguna hija
puede autenticarse ni figurar en una lista.

**Decisión:** la lista solo admite identidades de app, y la gramática de los nombres ya lo garantiza: un nombre con `/`
es `validation`. La regla «una hija cuenta como su app» queda en docs/hecate/acceso.md y en la fila de DECISIONS de
las identidades hijas, para quien las construya. FR-011 y la historia 3 de la spec se alinearon: el escenario que no se
podía probar se quitó.

**Alternativa:** construir ya la regla en Hermes, con SPIFFE IDs de varios segmentos y certificados sintéticos
(descartada por el propietario: adelanta parte de las identidades hijas a esta feature).

## R11. Sin tecnología nueva

En Hécate, los crates de los ADR 0007 y 0009; en Hermes, las dependencias del ADR 0011. No hace falta ningún ADR: la
decisión ya está en el ADR 0013.
