# Feature Specification: El contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

**Feature Branch**: `fix/hermes-call-contract`

**Created**: 2026-11-19

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/012-hermes-call-contract. Corregir el contrato de llamada
de Hermes en dos puntos y añadir una fila de documentación que falta. Primero, el reintento de Hades tras un fallo
reintentable de Hermes, abierto en DECISIONS desde la 010: Hades deriva la clave de idempotencia de response_id + paso +
hash(input resuelto). Un reintento del bloque, o relanzarlo desde el paso fallido con el mismo input, lleva la misma
clave. Hermes guarda en la clave toda respuesta que llegó al proveedor, también un fallo. La repetición recibe el fallo
guardado durante las 24 horas que vive la clave. La guía de integración de Hermes da dos reglas a quien llama: repetir
con la misma clave ante el rate_limited de la instancia, unavailable o internal, y usar otra clave para reintentar un
fallo retryable del proveedor (provider_unavailable o su rate_limited). No afecta solo a los reintentos: tras arreglar
la causa de un fallo no reintentable que garantiza que no se envió nada, repetir con la misma clave recibe el fallo
guardado. Esos fallos son las credenciales rechazadas, la persona que no admite el bot y una petición rechazada por el
proveedor. Con Telegram, todo fallo del proveedor garantiza que no se envió nada salvo timeout con outcome_unknown, que
sale de un 5xx o de una respuesta perdida tras enviar. La decisión se toma con el propietario en este cambio, entre dos
caminos: (1) Hermes solo guarda en la clave lo que se envió o pudo enviarse: completed y timeout con outcome_unknown. Un
fallo del proveedor que garantiza que no se envió nada libera la clave, como un fallo antes del proveedor, y su entrada
de auditoría se escribe igual. Quien llama tiene una sola regla, repetir con la misma clave, y Hades funciona como está
diseñado. (2) Hades cambia la clave en cada intento, sin tocar Hermes. La recomendada es la 1. Segundo, el kid del
token corto: Hermes elige la clave de firma de llamadas por el kid de la cabecera del token y rechaza un token sin él.
hermes.v1.json no lo dice: su descripción remite a la guía de integración de Hécate, cuya cabecera es
{"alg": "EdDSA", "typ": "JWT"}, y la descripción de callToken no nombra el kid. Hécate no rechaza un kid en la
cabecera, así que el arreglo es decirlo en el contrato de Hermes, con la subida de versión que corresponda. Tercero, a
la tabla de specs de hecate/README.md le falta la fila de la 009, que también cambió Hécate. Con la decisión que se
tome se actualizan el código y las pruebas, si los toca, y los docs de Hermes que describen la idempotencia y los
errores. También docs/hades.md si hace falta, y la fila de DECISIONS, que pasa de Abiertas a Tomadas. Rama
fix/hermes-call-contract; commit de squash fix:."

## Clarifications

### Session 2026-11-19

- Q: ¿Qué regla consigue que el reintento de Hades tras un fallo sin envío llegue al proveedor, y que relanzar el paso
  tras arreglar su causa también llegue: que Hermes deje de guardar en la clave los fallos sin envío (camino 1), que
  Hades use una clave nueva en cada intento (camino 2), o el camino 1 solo con los fallos reintentables? → A: El camino
  1: la clave solo guarda lo que se envió o pudo enviarse, un envío completado o un resultado desconocido. Un fallo sin
  envío, reintentable o no, la deja libre, como un fallo antes del proveedor, sin perder su entrada de auditoría. Quien
  llama tiene una sola regla, repetir con la misma clave, y Hades funciona como está diseñado, también al relanzar tras
  arreglar la causa; la misma clave con otro cuerpo tras un fallo sin envío es una petición nueva.
- Q: Ante un fallo inesperado de Hermes mientras llama al proveedor, cuando no sabe si el mensaje salió, ¿responde
  `timeout` con `outcome_unknown` y lo guarda como un resultado desconocido, responde `internal` y deja la clave libre,
  o responde `internal` guardado, como hoy, pero sin `retryable`? → A: `timeout` con `outcome_unknown`, guardado como
  un resultado desconocido: es lo que el diseño de Hermes pide ante un resultado ambiguo, la repetición no reenvía y
  quien llama lo trata como cualquier `timeout`. `internal` queda solo para los fallos inesperados antes del
  proveedor, que no dejan clave y se repiten con la misma.

## User Scenarios & Testing *(mandatory)*

Un componente que pide a Hermes algo con efecto, como Hades, envía en cada petición una `Idempotency-Key` suya. Hermes
guarda con esa clave la respuesta de cada petición que llegó al proveedor y la devuelve a toda repetición durante 24
horas, sin volver a enviar. Eso protege lo que se envió o pudo enviarse, pero guarda también los fallos que garantizan
que no salió nada. Hades deriva la clave del paso y de su input, así que un reintento con espera, o relanzar el paso
con el mismo input, recibe el fallo guardado y nunca llega a enviar. La guía de integración de Hermes lo esquiva
pidiendo otra clave para esos fallos: una regla distinta de la del resto de códigos, que el diseño de Hades no sigue.
Además, el contrato de Hermes no dice que el token corto lleva el `kid` de la clave de firma de quien llama, que Hermes
exige, y remite a la guía de Hécate, cuya cabecera no lo lleva.

En esta spec, «clave» es la `Idempotency-Key` de una petición; «repetir» es enviar la misma petición con la misma
clave; «fallo sin envío» es un fallo al enviar que garantiza que no salió nada, como que el proveedor no conecte, pida
esperar más de lo que cabe en el timeout de la función, rechace las credenciales de la instancia o la petición, o que la
persona no admita el bot; «resultado desconocido» es `timeout` con `outcome_unknown`, cuando el mensaje pudo llegar;
y «fallo antes del proveedor» es el de una petición que no llega al proveedor (no cumple el contrato, el canal no está
verificado, la instancia agotó su límite o Hermes no está listo).

Un error de Hermes que impide hacer la petición cuando la clave ya está reservada, como una instancia sin el token del
bot, es un fallo sin envío para la clave, que queda libre con su entrada de auditoría, y responde `internal`, como un
fallo antes del proveedor.

Actores:

- **Componente que llama** (Hades, o cualquier componente con acceso a Hermes): envía las peticiones con su clave y
  decide si repite.
- **Admin**: arregla la causa de un fallo sin envío (las credenciales de una instancia, o pide a la persona que vuelva a
  admitir el bot) y opera Hermes.
- **Hermes**: guarda las claves, envía por el proveedor y responde con el sobre común.
- **Quien desarrolla un componente que llama**: construye su cliente con el contrato de Hermes.

### User Story 1 - Un reintento tras un fallo sin envío llega a enviar (Priority: P1)

Hermes responde `provider_unavailable` con `retryable` porque no consigue conectar con el proveedor. Hades reintenta el
bloque con espera, como fija su diseño, y en cuanto el proveedor responde, el mensaje sale una sola vez.

**Why this priority**: es la contradicción abierta en DECISIONS: tal como están diseñados, Hades nunca consigue
reintentar un fallo reintentable del proveedor. Sin arreglarla, un corte breve con el proveedor hace fallar el bloque
aunque el proveedor vuelva en segundos.

**Independent Test**: con el proveedor `fake` configurado para no conectar, enviar un mensaje; quitar el fallo y
reintentar con la misma clave: el mensaje llega una vez a la bandeja del `fake`.

**Acceptance Scenarios**:

1. **Given** un envío que falla sin conectar (`provider_unavailable` con `retryable`), **When** quien llama lo reintenta
   con la misma clave, **Then** el reintento llega al proveedor y, si este responde, el mensaje sale.
2. **Given** un envío al que el proveedor pide esperar más de lo que cabe (`rate_limited` con `retry_after_s`), **When**
   quien llama lo reintenta con la misma clave pasada esa espera, **Then** el reintento llega al proveedor.
3. **Given** un reintento que acaba enviando, **When** se repite otra vez la misma petición con la clave del envío,
   **Then** devuelve la respuesta del envío, sin volver a enviar.
4. **Given** varios intentos que llegaron al proveedor, **When** se consulta la auditoría, **Then** cada intento tiene
   su entrada, también los que fallaron sin envío.

---

### User Story 2 - Repetir tras arreglar la causa de un fallo sin envío (Priority: P2)

Un envío falla porque el proveedor rechaza las credenciales de la instancia, o porque la persona no admite el bot. El
admin arregla la causa y se relanza el paso con el mismo input: el mensaje sale, sin esperar a que pasen las 24 horas
de la clave.

**Why this priority**: hoy la repetición recibe el fallo guardado durante 24 horas, así que relanzar el paso desde
Hades tras arreglar la causa no envía nada aunque ya no haya motivo para fallar. Va después de la historia 1 porque
pide una intervención del admin y es menos frecuente que un corte con el proveedor.

**Independent Test**: con el `fake` configurado para rechazar las credenciales, enviar un mensaje; quitar el fallo y
repetir la petición con la misma clave: el mensaje llega.

**Acceptance Scenarios**:

1. **Given** un envío que falló porque el proveedor rechazó las credenciales de la instancia, **When** el admin las
   arregla y se relanza el paso con el mismo input, **Then** el mensaje sale.
2. **Given** un envío que falló porque la persona no admite el bot, **When** la persona vuelve a admitirlo y se relanza
   el paso con el mismo input, **Then** el mensaje sale.
3. **Given** la causa sin arreglar, **When** se relanza el paso, **Then** responde el mismo fallo, sin enviar nada.

---

### User Story 3 - Lo que pudo enviarse no se repite (Priority: P1)

Un envío acaba completado, o con resultado desconocido. Toda repetición con su clave devuelve la misma respuesta durante
24 horas, sin volver a enviar, como hoy.

**Why this priority**: es la garantía que da sentido a la clave. Arreglar el reintento no puede abrir la puerta a un
mensaje duplicado, como un aviso de bloqueo que llega dos veces.

**Independent Test**: con el `fake` configurado para cortar después de enviar, enviar un mensaje y repetirlo varias
veces con la misma clave: el `fake` recibe un solo mensaje y cada repetición responde `timeout` con `outcome_unknown`.

**Acceptance Scenarios**:

1. **Given** un envío completado, **When** se repite con su clave, **Then** devuelve la misma respuesta sin volver a
   enviar.
2. **Given** un envío con resultado desconocido, **When** se repite con su clave, **Then** responde `timeout` con
   `outcome_unknown` sin volver a enviar.
3. **Given** un corte de Hermes después de reservar la clave y antes de guardar la respuesta, **When** se repite la
   petición, **Then** responde `timeout` con `outcome_unknown`, como hoy, aunque el proveedor no hubiera llegado a
   enviar: Hermes no sabe si llegó.
4. **Given** un fallo inesperado de Hermes mientras llama al proveedor, **When** se responde la petición y se repite
   con su clave, **Then** las dos respuestas son `timeout` con `outcome_unknown`, sin volver a enviar.

---

### User Story 4 - El contrato dice cómo es el token (Priority: P2)

Quien desarrolla un componente que llama a Hermes construye su cliente solo con el contrato de Hermes y la guía de
Hécate que este nombra. El contrato dice que la cabecera del token corto lleva el `kid` de su clave de firma de
llamadas, y su primera llamada autentica.

**Why this priority**: el contrato es la única dependencia entre componentes (principio II de la constitución). Hoy un
cliente que siga la guía de Hécate envía un token sin `kid`, y Hermes responde `auth` sin que el contrato diga por qué.
Va después de las historias 1 y 3 porque los clientes de hoy ya envían el `kid`: es un hueco del contrato, no un fallo
en marcha.

**Independent Test**: leer el contrato de Hermes y comprobar que dice el `kid` y lo que pasa sin él; las pruebas de
contrato siguen pasando.

**Acceptance Scenarios**:

1. **Given** el contrato de Hermes, **When** se lee cómo es el token corto, **Then** dice que su cabecera lleva el `kid`
   de la clave de firma de llamadas que Hécate dio a quien llama, y que sin él la llamada responde `auth`.
2. **Given** el contrato de Hermes, **When** se lee la `Idempotency-Key`, **Then** dice qué recibe una repetición con la
   misma clave: la respuesta guardada de un envío completado o con resultado desconocido, y un intento nuevo tras un
   fallo sin envío o antes del proveedor.
3. **Given** un token sin `kid`, **When** llega a Hermes, **Then** responde `auth`, como hoy: el comportamiento no
   cambia.

---

### User Story 5 - Los documentos dicen lo mismo (Priority: P3)

El admin y quien desarrolla un componente encuentran una sola regla de repetición por código de error, la misma en la
guía de integración de Hermes, en su diseño de las llamadas, en su tabla de códigos, en su contrato y en el diseño de
Hades. La decisión queda registrada, y la tabla de specs de Hécate lista la 009.

**Why this priority**: las dos reglas de hoy y la contradicción con Hades salieron de leer los documentos juntos; tras
el arreglo, ninguno puede quedarse con la regla anterior. Va la última porque documenta lo que fijan las anteriores.

**Independent Test**: buscar en los documentos vigentes la regla de repetición de cada código y comprobar que coinciden;
leer DECISIONS y el README de Hécate.

**Acceptance Scenarios**:

1. **Given** la tabla de errores de la guía de integración de Hermes, **When** se lee qué hace quien llama con cada
   código, **Then** la regla de repetición coincide con la del diseño de las llamadas, la de la tabla de códigos, la
   del contrato y la de Hades.
2. **Given** DECISIONS, **When** se busca el reintento de Hades tras un fallo reintentable de Hermes, **Then** ya no
   está entre lo abierto y figura entre lo tomado, con su ADR.
3. **Given** la tabla de specs del README de Hécate, **When** se busca la 009, **Then** tiene su fila.

---

### Edge Cases

- **Dos peticiones iguales a la vez y la primera falla sin envío:** la segunda espera a que termine la primera, como
  hoy, y, como la clave queda libre, intenta el envío ella misma: es un reintento más.
- **La misma clave con otro cuerpo tras un fallo sin envío:** es una petición nueva, como tras un fallo antes del
  proveedor, porque la clave quedó libre; tras un envío completado o con resultado desconocido sigue respondiendo
  `conflict`.
- **Un fallo inesperado de Hermes mientras llama al proveedor**, sin saber si el mensaje salió: responde `timeout` con
  `outcome_unknown` y la clave lo guarda, como un resultado desconocido (FR-004).
- **Un fallo inesperado antes del proveedor:** responde `internal`, no deja clave y se repite con la misma, como hoy.
- **Un reintento que acaba con resultado desconocido tras un fallo sin envío:** la clave guarda ese resultado y toda
  repetición lo devuelve (FR-004).
- **Un corte de Hermes entre el fallo sin envío y guardar su resultado:** la repetición responde `timeout` con
  `outcome_unknown`, como tras cualquier corte, porque Hermes no sabe si llegó al proveedor.
- **Los fallos antes del proveedor** (el límite de la instancia, `unavailable`, `internal`, un canal sin verificar): no
  dejan clave y se repiten con la misma, como hoy (FR-006).
- **Una repetición pasadas 24 horas:** es una petición nueva, como hoy.
- **Relanzar el paso con el input cambiado:** Hades deriva otra clave y es un efecto nuevo e intencionado, como fija su
  diseño.
- **Un proveedor que sigue caído durante muchos reintentos:** cada intento llega al proveedor, cuenta en el límite y en
  el estado de la instancia y deja su entrada de auditoría; lo acotan la espera entre reintentos y el timeout del
  bloque de Hades.
- **Un token con `kid` dirigido a Hécate:** Hécate no lo rechaza, así que quien llama puede usar la misma cabecera con
  los dos.
- **Un `kid` que Hermes no tiene en su copia de las claves publicadas:** lo pide a Hécate, como hoy.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Un reintento de Hades tras un fallo sin envío reintentable (el proveedor no conecta, o pide esperar más
  de lo que cabe en el timeout de la función) DEBE poder llegar al proveedor y enviar el mensaje, siguiendo el diseño
  de Hades: el reintento con espera del bloque, con la clave que deriva del paso y de su input.
- **FR-002**: Hermes DEBE guardar en la clave solo la respuesta de lo que se envió o pudo enviarse: un envío completado
  o un resultado desconocido. Un fallo sin envío, reintentable o no, NO DEBE quedar en la clave: la deja libre, como un
  fallo antes del proveedor, sin perder su entrada de auditoría (FR-005).
- **FR-003**: Repetir el paso tras arreglar la causa de un fallo sin envío no reintentable (las credenciales rechazadas,
  la persona que no admite el bot o una petición rechazada por el proveedor) DEBE llegar al proveedor, sin esperar a
  que pasen las 24 horas de la clave.
- **FR-004**: Lo que se envió o pudo enviarse NO DEBE volver a enviarse: una petición completada, o con resultado
  desconocido (también tras un corte de Hermes), DEBE devolver la misma respuesta a toda repetición con su clave
  durante 24 horas. Un fallo inesperado de Hermes mientras llama al proveedor, cuando no sabe si el mensaje salió,
  DEBE responder `timeout` con `outcome_unknown` y quedar en la clave como un resultado desconocido; `internal` DEBE
  quedar solo para los fallos inesperados antes del proveedor, que no dejan clave.
- **FR-005**: Cada intento que llega al proveedor DEBE dejar su entrada de auditoría, también el que falla sin envío,
  como hoy (FR-029 de la 008).
- **FR-006**: Los fallos antes del proveedor NO DEBEN cambiar: no dejan clave y se repiten con la misma.
- **FR-007**: El contrato de Hermes DEBE decir que la cabecera del token corto lleva el `kid` de la clave de firma de
  llamadas que Hécate dio a quien llama, y que una llamada sin él responde `auth`. La autenticación NO DEBE cambiar.
- **FR-008**: El contrato de Hermes DEBE decir qué recibe una repetición con la misma clave (lo que se guarda y lo que
  no), porque Hades depende de ello (principio II de la constitución). El contrato DEBE subir la versión que
  corresponda a sus cambios.
- **FR-009**: La guía de integración de Hermes, su diseño de las llamadas, su tabla de códigos, su contrato y el diseño
  de Hades DEBEN dar la misma regla de repetición para cada código de error; ningún documento vigente DEBE conservar la
  regla anterior.
- **FR-010**: Lo que guarda la clave, como lo fijan el FR-002 y el FR-004, DEBE quedar en un ADR nuevo, con su
  contexto, sus opciones y su porqué, y en la tabla de decisiones tomadas de DECISIONS; la fila abierta «Reintento de
  Hades tras un fallo reintentable de Hermes» DEBE retirarse.
- **FR-011**: La tabla de specs del README de Hécate DEBE tener la fila de la 009.
- **FR-012**: Fuera de lo que fijan el FR-002 y el FR-004, nada más DEBE cambiar: ni la clasificación de los fallos de
  cada proveedor, ni el reintento interno de Hermes, ni la vida de las claves, ni la API de Hécate, ni el canal local
  ni las salidas de la CLI de Hermes.

### Key Entities

- **Clave de idempotencia**: la `Idempotency-Key` de quien llama, guardada con su identidad y la huella de la
  petición cuando la petición se envió o pudo enviarse; mientras vive, 24 horas, devuelve su respuesta a toda
  repetición.
- **Respuesta guardada**: la que Hermes devuelve a una repetición sin volver a enviar: la de un envío completado o con
  resultado desconocido. Un fallo sin envío no deja respuesta guardada.
- **Token corto**: el JWT que firma quien llama en cada petición, con una cabecera que lleva el algoritmo, el tipo y el
  `kid` de su clave de firma de llamadas.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Tras un fallo sin envío reintentable, un reintento con la misma clave envía el mensaje al primer intento
  en el que el proveedor responde, en el 100 % de los recorridos.
- **SC-002**: Tras arreglar la causa de un fallo sin envío no reintentable, relanzar el paso con el mismo input envía el
  mensaje al primer intento, sin esperar 24 horas.
- **SC-003**: Ninguna repetición de una petición completada o con resultado desconocido vuelve a enviar: 0 mensajes
  duplicados en todos los recorridos.
- **SC-004**: Quien construye un cliente nuevo siguiendo solo el contrato de Hermes y la guía de Hécate que nombra
  autentica su primera llamada.
- **SC-005**: Para cada código de error, la guía de integración de Hermes, su diseño de las llamadas, su tabla de
  códigos, su contrato y el diseño de Hades dan la misma regla de repetición: 0 contradicciones en la revisión.
- **SC-006**: DECISIONS no tiene abierto el reintento de Hades, que figura entre lo tomado con su ADR, y la tabla de
  specs del README de Hécate tiene la fila de la 009.

## Assumptions

- **Base**: `main` tras la 011 (`5e7be30`).
- **Hades no está construido**: su diseño vive en `docs/hades.md`; este cambio no construye nada de Hades.
- **La clasificación de los fallos no cambia**: en Telegram y en el `fake`, todo fallo del proveedor garantiza que no
  se envió nada salvo el resultado desconocido, y Hermes reintenta por dentro, como hoy, solo lo que no salió.
- **Hécate no cambia**: no rechaza un `kid` en la cabecera del token corto, y su guía de integración sigue describiendo
  lo que ella exige.
- **El versionado del contrato**: una subida de parche si solo cambian descripciones, sin cambiar esquemas ni
  operaciones.
- **El ADR**: resolver una decisión abierta pasa a un ADR (constitución, «Consolidación»), que sería el 0014.
- **Pruebas**: las de la idempotencia de Hermes en los niveles que toque la regla del FR-002, las de contrato y la suite
  de Hermes en macOS y en Linux.
- **Nombres concretos**: `Idempotency-Key`, `provider_unavailable`, `rate_limited`, `timeout`, `outcome_unknown`,
  `retryable`, `conflict`, `internal`, `auth` y `kid` aparecen porque son el contrato vigente de Hermes, no una
  elección de este cambio.
- **Fuera de alcance**: construir Hades, cambiar la clasificación de los fallos de los proveedores, cambiar la vida de
  las claves y cambiar Hécate o su API.
- **Rama**: `fix/hermes-call-contract`, creada desde `main` por el propietario; commit de squash `fix:`.
