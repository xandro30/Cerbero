# Feature Specification: Hermes nota la revocación urgente de su propio certificado

**Feature Branch**: `fix/hermes-own-revocation`

**Created**: 2026-11-19

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/011-hermes-own-revocation. Arreglo en Hermes: detectar la
revocación urgente de su propio certificado, como exige el FR-010 de la feature 008 («Sin un certificado válido
(caducado, revocado o de una CA sustituida), Hermes NO DEBE estar listo, y DEBE decir que necesita un token de alta
nuevo»). Lo encontró el recorrido de la 010 en el compose de hermes/deploy/ con el fake: tras `hecate certificates
revoke <serie de Hermes> --urgent`, `hermes health` sigue diciendo `ready` más de 5 minutos, porque Hécate rechaza con
`auth` el refresco de las publicaciones de Hermes (su log dice `publications.refresh_failed` con `reason: auth`) y
Hermes nunca recibe la lista de revocación con su propia serie. Hermes solo da su identidad por no válida cuando Hécate
rechaza su renovación, a los dos tercios de la vida del certificado (unos 2 días), y mientras, `hermes enroll` con un
token nuevo responde `conflict` («hermes already has a valid identity»). El camino que funciona hoy, documentado en
docs/hermes/operacion.md («La identidad de Hermes deja de valer»), es parar Hermes, vaciar HERMES_STATE_DIR,
arrancarlo, `hecate tokens create hermes` y `hermes enroll`. Hecho cuando: tras una revocación urgente, Hermes dice
`not ready: needs a new enrollment token` en el siguiente refresco de las publicaciones (240 s como mucho) y
`hermes enroll` funciona sin vaciar el directorio de estado; pasan las pruebas unitarias del servicio de identidad, el
escenario de revocación en el E2E del compose y la suite de Hermes en macOS y Linux; y docs/hermes/operacion.md,
docs/hermes/cli.md (los motivos de `hermes health`) y la fila de la identidad de «Ante fallos» en docs/hermes.md
describen ese comportamiento, sin el paso de vaciar el directorio. Empieza con la 010 fusionada en main, porque toca los
mismos documentos. Abierto para clarify: qué rechazos de Hécate cuentan, además del refresco; qué pasa tras un reinicio
con el certificado revocado; y si la identidad vuelve a valer sola cuando Hécate acepta otra vez sus llamadas. Rama
fix/hermes-own-revocation, creada desde main; commit de squash fix:."

## Clarifications

### Session 2026-11-19

- Q: ¿Qué rechazos de Hécate deben hacer que Hermes dé su identidad por no válida: solo el del refresco de sus
  publicaciones (y el de la renovación, como hoy), o el de cualquier llamada que Hermes le hace con su identidad? → A:
  El de cualquier llamada que Hermes hace a Hécate con su identidad: el refresco de sus publicaciones, la renovación,
  una clave que no tiene, su lista de acceso ante quien no está en su copia, la resolución de sus secretos y su
  comprobación al aplicar la configuración. En la API de Hécate, `auth` a una llamada autenticada siempre significa que
  el certificado o el token corto de quien llama no valen (la única excepción, el estado sin su permiso, Hermes no la
  consulta), así que una sola regla cubre todos los caminos; Hermes lo nota en la primera llamada rechazada, y el
  refresco de cada 240 s acota el plazo.
- Q: Tras reiniciar Hermes con su certificado ya revocado, ¿basta con que lo note en la primera llamada a Hécate del
  arranque, o debe recordar el rechazo de antes del reinicio? → A: Basta con la primera llamada del arranque: el
  rechazo vive solo en memoria, como hoy la renovación rechazada. Hécate rechaza esa llamada (la que resuelve la
  contraseña de su base) segundos después de arrancar, y cada arranque vuelve a comprobarlo con Hécate, lo que además
  deshace un falso positivo cuya causa ya se corrigió.
- Q: Una vez que Hermes da su identidad por no válida, ¿hasta cuándo sigue así, y sigue llamando a Hécate con ella
  mientras tanto? → A: Hasta un alta nueva, o un reinicio, que vuelve a comprobarla con Hécate; mientras tanto, Hermes
  no llama a Hécate con esa identidad. Un certificado revocado con urgencia no vuelve a valer, y una identidad
  deshabilitada y vuelta a habilitar solo recupera certificado con un alta nueva, así que seguir llamando no sirve de
  nada; y cada llamada rechazada deja una fila en el registro de accesos de Hécate, que no se purga, y taparía las de
  quien use la clave robada (el polling de cada instancia lo haría cada 5 s).

## User Scenarios & Testing *(mandatory)*

Hermes tiene su identidad en Hécate: un certificado de 3 días que renueva una sola vez, a los dos tercios de su vida, y
con el que autentica cada llamada a Hécate. Cuando el admin revoca con urgencia ese certificado, por ejemplo porque su
clave se ha comprometido, Hécate deja de aceptar al momento las llamadas hechas con él y pone su serie en la lista de
revocación, que solo da a los componentes autenticados. Hermes ya no puede pedir esa lista, así que nunca ve su serie
en ella: hoy solo da su identidad por no válida cuando Hécate rechaza su renovación, unos dos días después. Mientras,
dice que está listo y no acepta un alta nueva, y el admin tiene que pararlo y vaciar su directorio de estado para
volver a darlo de alta. Este arreglo hace que Hermes entienda el rechazo de Hécate: su identidad ya no vale y necesita
un token de alta nuevo.

En esta spec, «identidad de Hermes» es su certificado vigente con sus claves; «válida», que Hécate la acepta; «sus
publicaciones» son la lista de revocación, las claves publicadas y su lista de acceso, que Hermes refresca cada 240 s;
y «rechazo» es la respuesta `auth` de Hécate a una llamada autenticada con la identidad de Hermes, que según la
[guía de integración de Hécate](../../docs/hecate/integracion.md) significa que su certificado o su token corto no
valen (revocado con urgencia, caducado, de otra CA, con la identidad deshabilitada o tras un segundo intento de
renovar).

Actores:

- **Admin**: revoca certificados en Hécate, crea los tokens de alta, da de alta a Hermes por su canal local, consulta
  su salud y sigue su operación.
- **Hermes**: refresca sus publicaciones, dice si está listo y atiende a los componentes con acceso.
- **Hécate**: emite y revoca la identidad de Hermes, y rechaza las llamadas hechas con un certificado que ya no vale.

### User Story 1 - Hermes deja de estar listo tras la revocación urgente de su certificado (Priority: P1)

El admin revoca con urgencia el certificado de Hermes. En la primera llamada que Hécate le rechaza, y como mucho en el
siguiente refresco de sus publicaciones, Hermes nota que Hécate ya no acepta su identidad: deja de estar listo, su
salud dice que necesita un token de alta nuevo y su API deja de atender.

**Why this priority**: es el FR-010 de la 008, que hoy no se cumple tras una revocación urgente. Un certificado
revocado porque su clave se comprometió no debe seguir sirviendo durante días, y sin este aviso el admin no sabe que
Hermes necesita un alta.

**Independent Test**: en el compose de pruebas, con Hermes listo, revocar con urgencia su certificado y consultar su
salud cada pocos segundos: dice `not ready: needs a new enrollment token` antes de que pasen 240 s desde la revocación
(más lo que tarde ese refresco), y una llamada a su API responde `unavailable` sin enviar nada.

**Acceptance Scenarios**:

1. **Given** Hermes listo, **When** el admin revoca con urgencia su certificado, **Then** como mucho en el siguiente
   refresco de sus publicaciones, a los 240 s, Hermes deja de estar listo y su salud dice
   `not ready: needs a new enrollment token`, sin reiniciarse.
2. **Given** Hermes que ya ha notado el rechazo, **When** un componente con acceso llama a cualquier operación de su
   API, **Then** Hermes responde `unavailable`, sin efectos.
3. **Given** Hermes que ya ha notado el rechazo, **When** se consulta su log, **Then** dice que Hécate rechazó su
   identidad, sin secretos ni certificados.
4. **Given** Hécate sellada, caída o con un fallo inesperado durante un refresco, **When** el refresco falla, **Then**
   Hermes sigue listo con su última copia: solo un rechazo cambia la validez de su identidad.
5. **Given** el certificado de otro componente revocado con urgencia, **When** Hermes refresca sus publicaciones,
   **Then** sigue listo y rechaza a ese componente, como hoy.
6. **Given** el certificado de Hermes revocado con urgencia, **When** Hermes hace otra llamada a Hécate antes del
   siguiente refresco (resuelve un secreto, pide una clave que no tiene o su lista de acceso), **Then** deja de estar
   listo en esa misma llamada.
7. **Given** Hermes que ya ha notado el rechazo, **When** llegan sus refrescos, el polling de sus instancias o
   cualquier otra tarea que necesita a Hécate, **Then** no hace ninguna llamada a Hécate con esa identidad hasta un
   alta nueva o un reinicio.

---

### User Story 2 - El admin da a Hermes una identidad nueva sin vaciar su directorio de estado (Priority: P2)

Con Hermes que ya sabe que su identidad no vale, el admin crea un token de alta nuevo y da de alta a Hermes como en la
puesta en marcha, sin pararlo ni tocar su directorio de estado. Hermes vuelve a estar listo con su configuración y sus
canales de siempre.

**Why this priority**: hoy el alta responde `conflict` y obliga a parar Hermes y vaciar su directorio de estado. Con
el arreglo, recuperarse de una revocación urgente es lo mismo que recuperarse de un certificado caducado. Depende de la
historia 1, porque Hermes solo acepta un alta cuando sabe que su identidad no vale.

**Independent Test**: tras la revocación de la historia 1, crear un token de alta para Hermes y darlo de alta, sin
parar Hermes ni tocar su directorio de estado: el alta escribe la serie nueva, la salud vuelve a `ready` y un envío
llega a su contacto.

**Acceptance Scenarios**:

1. **Given** Hermes que dice `needs a new enrollment token` tras una revocación urgente, **When** el admin lo da de
   alta con un token nuevo, **Then** el alta funciona, sin `conflict`, y escribe
   `enrolled <spiffe id> serial <serie nueva> until <fecha>`.
2. **Given** el alta hecha, **When** se consulta la salud de Hermes, **Then** dice `ready` sin haberlo reiniciado, y
   Envoy presenta el certificado nuevo sin reiniciarse.
3. **Given** el alta hecha, **When** un componente con acceso envía un mensaje, **Then** llega a su contacto, con la
   configuración, los canales verificados y el registro que había antes de la revocación.
4. **Given** un rechazo a una llamada hecha con la identidad revocada que llega después del alta, **When** Hermes lo
   recibe, **Then** no invalida la identidad nueva.
5. **Given** Hermes con una identidad válida, **When** el admin intenta otra alta, **Then** responde `conflict`, como
   hoy.

---

### User Story 3 - La operación de Hermes describe lo que hace de verdad (Priority: P3)

El admin que sigue el procedimiento «La identidad de Hermes deja de valer» recupera Hermes con un token nuevo y
`hermes enroll`, sin pararlo ni vaciar su directorio de estado, y ningún documento vigente dice ya que Hermes no nota
la revocación urgente de su certificado.

**Why this priority**: los documentos describen hoy el rodeo; tras el arreglo describirían un comportamiento que ya no
existe. Va la última porque documenta lo que construyen las dos primeras.

**Independent Test**: recorrer el procedimiento en el compose de pruebas siguiendo solo el documento, y buscar en los
documentos vigentes el paso de vaciar el directorio de estado y la limitación de la revocación urgente.

**Acceptance Scenarios**:

1. **Given** el procedimiento «La identidad de Hermes deja de valer», **When** el admin lo sigue tras una revocación
   urgente, **Then** recupera Hermes sin pararlo ni vaciar su directorio de estado.
2. **Given** la referencia de la CLI, **When** el admin busca el motivo `needs a new enrollment token`, **Then**
   incluye la revocación urgente, que Hermes nota en la primera llamada que Hécate le rechaza.
3. **Given** el documento de alto nivel de Hermes, **When** se lee «Ante fallos», **Then** la fila de la identidad dice
   cuándo nota Hermes una revocación urgente y cómo se recupera.
4. **Given** DECISIONS, **When** se busca la revocación urgente de Hermes, **Then** ya no figura entre lo abierto.

---

### Edge Cases

- **Hécate sellada o caída en un refresco:** Hermes sigue listo con su última copia; si su certificado se revocó
  mientras, lo nota en la primera llamada a la que Hécate vuelve a responder.
- **Una revocación mientras Hermes arranca, antes de estar listo:** su salud da el motivo de la identidad en cuanto
  Hécate le rechaza una llamada del arranque, y el alta funciona aunque el arranque no haya terminado.
- **Un reinicio de Hermes con su certificado ya revocado:** lo nota en la primera llamada a Hécate del arranque,
  segundos después; hasta entonces, su salud da el motivo del paso del arranque en el que está (FR-007).
- **Una renovación en curso cuando llega la revocación:** Hécate rechaza también la renovación, como hoy; el alta nueva
  descarta la renovación en curso.
- **La identidad de Hermes deshabilitada en Hécate**, con o sin la opción urgente, o **un segundo intento de
  renovar**, que revoca con urgencia todos sus certificados: Hécate también rechaza sus llamadas y el resultado es el
  mismo. Una identidad deshabilitada necesita que el admin la habilite antes de crearle el token, como dice la
  operación de Hécate.
- **Un rechazo que llega de una llamada hecha con la identidad anterior**, después de que un alta nueva la sustituya:
  no cuenta (FR-004).
- **Un alta con un token gastado, caducado o de otra CA:** Hécate la rechaza, como hoy; Hermes sigue sin estar listo y
  el admin crea otro token.
- **Varias llamadas a Hécate a la vez cuando llega la revocación:** las que ya estaban en curso reciben también su
  rechazo; desde el primero, no empieza ninguna otra con esa identidad (FR-008).
- **Conexiones nuevas a la base tras el alta**, porque la base cortó las que había o hacen falta más: Hermes las abre
  en cuanto termina el alta, porque vuelve a pedir a Hécate la contraseña de su base, que pudo perder mientras su
  identidad no valía (FR-006).
- **Un desfase de más de 30 s entre los relojes de Hermes y Hécate:** Hécate rechaza también el token corto de cada
  llamada, y Hermes no puede distinguirlo de una revocación, así que dice que necesita un token nuevo y deja de llamar
  a Hécate. Un alta nueva no lo arregla, porque Hécate vuelve a rechazar la siguiente llamada; se arregla corrigiendo
  el reloj y reiniciando Hermes (FR-007, FR-008).
- **La revocación del certificado de otro componente:** Hermes rechaza a ese componente tras el refresco, como hoy, y
  su propia salud no cambia.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Si Hécate rechaza con `auth` cualquier llamada que Hermes le hace con su identidad (el refresco de sus
  publicaciones, la renovación, una clave que no tiene, su lista de acceso ante quien no está en su copia, la
  resolución de sus secretos o su comprobación al aplicar la configuración), Hermes DEBE dar su identidad por no
  válida, como hoy cuando Hécate rechaza su renovación: NO DEBE estar listo, y su salud por el canal local DEBE decir
  `needs a new enrollment token` (FR-010 de la 008).
- **FR-002**: Hermes DEBE notarlo sin reiniciarse, en la primera llamada que Hécate le rechace, y como mucho en el
  siguiente refresco de sus publicaciones, que llega a los 240 s de la revocación.
- **FR-003**: Un refresco que falla por otro motivo (Hécate sellada o caída, un fallo inesperado, una respuesta que no
  llega) NO DEBE cambiar la validez de la identidad: Hermes DEBE seguir con su última copia, como fija el FR-008 de la
  008.
- **FR-004**: Un rechazo DEBE contar solo para la identidad con la que se hizo la llamada: si llega después de que un
  alta nueva la haya sustituido, NO DEBE invalidar la identidad nueva.
- **FR-005**: Mientras su identidad no es válida, la API de red de Hermes DEBE responder `unavailable` a toda
  operación, sin efectos, y el log de Hermes DEBE decir que Hécate rechazó su identidad, sin secretos ni certificados.
- **FR-006**: `hermes enroll` DEBE aceptar un token de alta nuevo en cuanto Hermes da su identidad por no válida, sin
  parar Hermes ni vaciar su directorio de estado. Tras el alta, Hermes DEBE volver a estar listo con la identidad nueva
  sin reiniciarse y conservar su base (la configuración, la verificación de los canales, la idempotencia y la
  auditoría), y Envoy DEBE pasar al certificado nuevo sin reiniciarse. Hermes DEBE poder abrir conexiones nuevas a su
  base en cuanto termina el alta, sin esperar al refresco de la contraseña, que pudo perder mientras su identidad no
  valía.
- **FR-007**: Tras un reinicio con su certificado revocado, Hermes DEBE notarlo en la primera llamada a Hécate del
  arranque (la que resuelve la contraseña de su base, segundos después de arrancar). El rechazo DEBE vivir solo en
  memoria: NO DEBE guardarse en su directorio de estado, así que cada arranque vuelve a comprobarlo con Hécate.
- **FR-008**: Una identidad rechazada DEBE seguir no válida hasta un alta nueva, o hasta un reinicio, que vuelve a
  comprobarla con Hécate (FR-007). Mientras tanto, Hermes NO DEBE empezar ninguna llamada a Hécate con ella
  (refrescar sus publicaciones, renovar, resolver sus secretos, también los del polling de sus instancias y los de la
  aplicación de la configuración, ni pedir una clave o su lista de acceso): lo que necesitaba esa llamada DEBE fallar
  como si Hécate la hubiera rechazado, para que el registro de accesos de Hécate solo muestre a quien siga usando el
  certificado revocado. El alta nueva, que no usa esa identidad, sí llega a Hécate.
- **FR-009**: Las demás causas del FR-010 de la 008 DEBEN seguir como hoy: sin identidad o con el certificado caducado,
  Hermes no está listo y pide un token; ante una CA sustituida, sigue intentando conectar y vuelve a estar listo si
  Hécate se presenta otra vez con la CA de siempre. La renovación rechazada es un rechazo más (FR-001 y FR-008). El
  motivo de la salud y las salidas de `hermes enroll` y `hermes health` NO DEBEN cambiar, ni ningún contrato: la API
  de Hermes, su canal local y la API de Hécate.
- **FR-010**: Al cerrar el cambio, DEBEN describir este comportamiento la operación de Hermes (el procedimiento «La
  identidad de Hermes deja de valer», sin el paso de vaciar el directorio de estado), su CLI (el motivo
  `needs a new enrollment token` de `hermes health`), su documento de alto nivel (la fila de la identidad en «Ante
  fallos»), su seguridad («Lo que no cubre») y su README de desarrollo (lo que recorre el E2E), y DECISIONS DEBE
  retirar la fila abierta «Revocación urgente de Hermes».

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Tras revocar con urgencia el certificado de Hermes, su salud dice
  `not ready: needs a new enrollment token` en 240 s como mucho, más lo que tarde ese refresco, en el 100 % de los
  recorridos.
- **SC-002**: Desde que Hermes nota el rechazo hasta el alta nueva, ninguna llamada a su API envía nada ni deja otro
  efecto.
- **SC-003**: Con un token de alta nuevo, el alta de Hermes funciona al primer intento sin pararlo ni tocar su
  directorio de estado, y en 2 minutos como mucho Hermes vuelve a estar listo y un envío llega a su contacto con la
  configuración y los canales de antes.
- **SC-004**: Con Hécate sellada o caída durante una hora, Hermes sigue listo con su última copia: ningún fallo que no
  sea un rechazo de Hécate le hace dar su identidad por no válida.
- **SC-005**: El admin recupera Hermes de una revocación urgente siguiendo solo su procedimiento de operación, y ningún
  documento vigente conserva el paso de vaciar el directorio de estado ni dice que Hermes no nota esa revocación.
- **SC-006**: Desde que Hermes nota el rechazo hasta el alta nueva o un reinicio, no empieza ninguna llamada más a
  Hécate con esa identidad: el registro de accesos de Hécate deja de ganar filas suyas.

## Assumptions

- **Base**: `main` tras la 010 (`f11cd29`). Hermes refresca sus publicaciones cada 240 s y ya da su identidad por no
  válida sin identidad, caducada, ante una CA sustituida o con la renovación rechazada.
- **Hécate no cambia**: tras una revocación urgente rechaza con `auth` toda llamada autenticada con ese certificado, y
  solo da la lista de revocación a los componentes autenticados. Hécate nunca llama a nadie, así que no puede avisar a
  Hermes.
- **El significado de `auth`**: en la API de Hécate, una llamada autenticada solo recibe `auth` si el certificado o el
  token corto de quien llama no valen; la única excepción es el estado sin su permiso, que Hermes no consulta. Un
  secreto que no se resuelve responde `not_found`.
- **El intervalo**: el refresco de las publicaciones sigue cada 240 s; el arreglo no lo cambia.
- **Pruebas**: unitarias del servicio de identidad de Hermes; el escenario en el E2E del compose (la revocación urgente
  de Hermes, su salud en 240 s, un token nuevo y el alta sin vaciar el directorio de estado, y un envío después); y la
  suite de Hermes en macOS y en Linux.
- **Nombres concretos**: `auth`, `unavailable`, `conflict`, `hermes health`, `hermes enroll`, Envoy y el canal local
  aparecen porque son el contrato y la operación vigentes de Hermes, no una elección de este arreglo.
- **Fuera de alcance**: cambiar Hécate o su API; distinguir un desfase de relojes de una revocación; la revocación de
  otros componentes, que ya funciona; y que Hermes renueve antes o refresque más a menudo.
- **Rama**: `fix/hermes-own-revocation`, creada desde `main` por el propietario; commit de squash `fix:`.
