# Feature Specification: Lista de acceso por app

**Feature Branch**: `feature/009-app-access`

**Created**: 2026-11-15

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/009-app-access. Lista de acceso por app: Hécate guarda qué
identidades pueden llamar a cada app, y cada app la consulta y la aplica; la primera que la aplica es Hermes. Por qué
ahora: es la decisión del ADR 0013, que el propietario tomó al aclarar la 008 y que DECISIONS deja como diseñada y no
implementada; hoy cualquier componente con un certificado de Cerbero puede llamar a Hermes, y el propietario la ha
puesto por delante de la reestructuración de la documentación de Hermes. Incluye: en Hécate, la lista por app: el admin
concede y retira a una identidad el acceso a una app entera, sin permisos por función, por su canal local y su CLI, con
cada cambio versionado con quién y cuándo, como la Access Policy, y su fila en el registro de accesos. En la API de
Hécate, la consulta: una app autenticada con su certificado y su token corto obtiene la lista de quién puede llamarla,
solo la suya, con su contrato OpenAPI en shared/contracts. En Hermes, la comprobación: tras autenticar a quien llama,
Hermes solo atiende a las identidades de su lista, y las demás reciben auth sin efectos y sin decir por qué; mantiene la
lista al día con Hécate y sigue con su última copia si Hécate no responde, como hace con la lista de revocación y las
claves publicadas. El token corto sigue sin conceder nada. Pruebas: las de Hécate en macOS y en Linux, y las de Hermes
con HECATE_BIN, Docker y el E2E con Envoy. Consolidación: docs/hecate/acceso.md, docs/hecate.md, docs/hermes.md,
ARCHITECTURE, GLOSSARY y DECISIONS, donde «Acceso por app» pasa a construido. Fuera de alcance: los permisos por
función; los usuarios, los roles y el perfilado de las personas, que son de Ténaro; la comprobación en otras apps, que
aún no tienen API; y el acceso a la propia API de Hécate, que todos los componentes necesitan para su identidad y sus
secretos. Abierto para clarify: qué pasa con una app que no tiene a nadie en su lista y cómo se pasa de hoy, cuando
cualquiera puede llamar a Hermes; si las identidades hijas (las integraciones de Hades) heredan el acceso de su padre;
cuánto tarda en valer en Hermes una retirada, y qué hace Hermes si nunca ha tenido la lista; y si se puede conceder el
acceso a una identidad que aún no se ha dado de alta. Rama feature/009-app-access, creada desde main; commit de squash
feat:."

## Clarifications

### Session 2026-11-15

- Q: ¿Qué pasa con una app que no tiene a nadie en su lista, y cómo se pasa de hoy, cuando cualquier componente con un
  certificado de Cerbero puede llamar a Hermes? → A: Nadie la puede llamar: el acceso se deniega por defecto, como en la
  Access Policy. Antes de que Hermes aplique la lista, el admin concede los accesos que hagan falta (p. ej. Hades a
  Hermes); la feature no deja ninguno concedido de partida. Hoy Hades no existe y solo llaman componentes de prueba, así
  que la transición no corta nada real.
- Q: ¿Las identidades hijas (las integraciones de Hades) heredan el acceso de su padre? → A: La lista es comunicación
  entre apps y solo admite identidades de app. Una identidad hija cuenta como su app: si `hades` puede llamar a Hermes,
  sus integraciones también, porque es Hades comunicándose con Hermes. Qué recurso alcanza cada integración o service
  account es perfilado y queda fuera de esta feature; si se hace, las service accounts se crearían aquí, en Hécate.
- Q: ¿Cuánto puede tardar en valer en Hermes un cambio de su lista? → A: Una concesión vale en la llamada siguiente:
  si llega una identidad que no está en su copia, Hermes vuelve a pedir la lista, una petición a la vez, como hace con
  las claves desde T113. Una retirada vale como mucho en 240 s, con el refresco de la lista de revocación y las claves
  publicadas. Para cortar ya a un componente comprometido está la revocación urgente de su certificado.
- Q: ¿Qué hace Hermes si nunca ha tenido su lista (recién instalado o con su copia perdida) y Hécate no responde? → A:
  No está listo: la lista se suma a los pasos del arranque, como la lista de revocación y las claves publicadas, su
  salud dice por qué y la API responde `unavailable` hasta que la tiene.
- Q: ¿Se puede conceder el acceso a una identidad de app que aún no se ha dado de alta? → A: Sí, como en la Access
  Policy: vale desde que se da de alta, así que el admin prepara los accesos antes de desplegar un componente. Un
  nombre mal escrito no da error: queda en la lista sin efecto y se ve en ella.

## User Scenarios & Testing *(mandatory)*

Hoy Hermes autentica a quien lo llama con su certificado y su token corto, pero no decide quién puede llamarlo:
cualquier componente de Cerbero con un certificado válido lo usa entero. El
[ADR 0013](../../docs/adr/0013-acceso-por-app.md) decidió dónde vive esa decisión: Hécate guarda, de forma central, qué
identidades pueden llamar a cada app, a la app entera y sin permisos por función, y el token corto no concede nada,
porque lo firma quien llama. Esta feature construye esa lista en Hécate, su consulta desde cada app y su aplicación en
Hermes, la primera app que la usa.

En esta spec, «app» es un componente de Cerbero que atiende llamadas de otros componentes por su API (hoy, Hermes);
«quien llama» es un componente autenticado con su certificado y su token corto; «lista de acceso de una app» son las
identidades que pueden llamarla; «conceder» y «retirar» son los cambios que el admin hace en esa lista; y «copia de la
lista» es la que una app guarda para decidir sin preguntar a Hécate en cada llamada.

Actores:

- **Admin**: concede y retira el acceso a cada app en Hécate, por su canal local, y consulta la lista vigente y su
  historial.
- **App**: consulta a Hécate su propia lista y la aplica a cada llamada. En esta feature, Hermes.
- **Quien llama**: un componente que llama a una app. En producción será Hades; en esta feature, componentes de prueba.
- **Hécate**: guarda la lista de cada app, la versiona y la da a cada app.

### User Story 1 - Conceder y retirar el acceso a una app (Priority: P1)

El admin decide qué identidades pueden llamar a cada app: concede a una identidad el acceso a una app entera, lo retira
cuando deja de hacer falta, y ve en cualquier momento quién puede llamar a cada app, quién lo cambió y cuándo.

**Why this priority**: es la fuente de verdad de todo lo demás. Sin ella no hay nada que consultar ni que aplicar, y
conceder o retirar el acceso pasa a ser un cambio en un solo sitio, sin tocar la app ni a quien llama.

**Independent Test**: con Hécate desbloqueada, conceder a una identidad el acceso a una app, consultar la lista de esa
app y su historial, retirarlo y volver a consultarla; repetir una orden que no cambia nada y comprobar que no crea una
versión nueva.

**Acceptance Scenarios**:

1. **Given** una identidad y una app, **When** el admin le concede el acceso, **Then** la lista de esa app la incluye,
   el cambio queda con quién y cuándo, y el registro de accesos tiene su fila.
2. **Given** una identidad con acceso a una app, **When** el admin se lo retira, **Then** la lista ya no la incluye, y
   el cambio queda con quién y cuándo.
3. **Given** una orden que no cambia nada (conceder lo que ya está concedido o retirar lo que no lo está), **When** se
   aplica, **Then** la lista no cambia ni gana una versión, la respuesta dice que no hubo cambios y el registro de
   accesos tiene su fila.
4. **Given** varias versiones de la lista de una app, **When** el admin consulta su historial, **Then** ve cada cambio
   con su versión, su fecha, la identidad y quién lo hizo, y puede saber quién podía llamar a la app en cualquier
   momento pasado.
5. **Given** una orden que nombra `hecate` o `admin`, como app o como quien llama, **When** se aplica, **Then** se
   rechaza con `validation` y no cambia nada.
6. **Given** Hécate sellada, **When** el admin da cualquier orden sobre las listas, **Then** se rechaza y no cambia
   nada, como las de la Access Policy.
7. **Given** una orden que nombra una identidad hija (p. ej. `hades/integration/x`), como app o como quien llama,
   **When** se aplica, **Then** se rechaza con `validation`: la lista solo admite identidades de app.

---

### User Story 2 - Una app consulta su propia lista (Priority: P2)

Cada app pregunta a Hécate, autenticada como cualquier componente, qué identidades pueden llamarla. Hécate le da su
lista y solo la suya, así que una app no puede saber quién llama a las demás.

**Why this priority**: es lo que deja a cada app aplicar la lista sin guardar una propia que diverja de la de Hécate
(principio I), y sirve igual para las apps que vengan después de Hermes.

**Independent Test**: con el cliente de referencia de Hécate dado de alta como una app, conceder el acceso a dos
identidades, consultar la lista y comprobar que trae exactamente esas dos; comprobar que nada en la consulta permite
pedir la lista de otra app.

**Acceptance Scenarios**:

1. **Given** una app con identidades concedidas, **When** consulta su lista con su certificado y su token corto,
   **Then** recibe exactamente esas identidades, con la versión de la lista.
2. **Given** una app sin nadie en su lista, **When** la consulta, **Then** recibe una lista vacía, sin error.
3. **Given** cualquier consulta, **When** Hécate responde, **Then** solo trae la lista de la identidad que pregunta:
   la consulta no permite nombrar otra app.
4. **Given** una consulta sin un certificado o un token válidos, **When** llega, **Then** Hécate responde `auth`, como
   al resto de operaciones.
5. **Given** Hécate sellada, **When** una app consulta su lista, **Then** Hécate responde `unavailable`, como a las
   demás operaciones salvo el estado.

---

### User Story 3 - Hermes atiende solo a quien tiene acceso (Priority: P3)

Hermes, tras autenticar a quien lo llama como hasta ahora, comprueba que su identidad está en la lista de Hermes. Si no
lo está, responde `auth` sin hacer nada y sin decir qué falló, como ante cualquier credencial que no vale. La lista la
mantiene al día con Hécate y, si Hécate no responde, sigue con la última que tiene.

**Why this priority**: es el objetivo de la feature: que llamar a Hermes deje de depender solo de tener un certificado
de Cerbero. Va la última porque necesita la lista de Hécate y su consulta.

**Independent Test**: con una Hécate de pruebas y Hermes detrás de Envoy, conceder el acceso a Hermes a un componente
de prueba y no a otro, y comprobar que el primero envía y el segundo recibe `auth` sin que salga ningún mensaje;
retirar el acceso al primero y comprobar que deja de enviar en el plazo de FR-009; sellar Hécate y comprobar que
Hermes sigue aplicando la última lista.

**Acceptance Scenarios**:

1. **Given** un componente autenticado que está en la lista de Hermes, **When** llama a cualquier operación de la API
   de Hermes, **Then** Hermes lo atiende como hasta ahora.
2. **Given** un componente autenticado que no está en la lista de Hermes, **When** llama a cualquier operación, **Then**
   Hermes responde `auth`, sin efectos y con el mismo mensaje que ante cualquier otro fallo de autenticación, y solo el
   log dice el motivo.
3. **Given** un acceso que el admin retira en Hécate, **When** pasan como mucho 240 s, **Then** ese componente recibe
   `auth` desde entonces, sin reiniciar Hermes.
4. **Given** Hécate sellada o caída, **When** Hermes intenta poner al día su copia, **Then** sigue aplicando la última
   que tiene: nadie gana el acceso ni lo pierde por ello.
5. **Given** un token corto que declara permisos, roles o el acceso a Hermes, **When** llega con un componente que no
   está en la lista, **Then** Hermes responde `auth`: lo que dice el token no cuenta.
6. **Given** la lista de Hermes vacía, también si nunca se le ha concedido nada, **When** llama cualquier componente
   autenticado, **Then** Hermes responde `auth` sin efectos.
7. **Given** un acceso que el admin acaba de conceder en Hécate, **When** ese componente llama a Hermes por primera vez,
   **Then** Hermes vuelve a pedir su lista y lo atiende en esa misma llamada.
8. **Given** un componente que no está en la copia de Hermes, **When** llama y Hécate no responde a la petición de la
   lista, **Then** Hermes responde `unavailable`, que vale reintentar, sin efectos.
9. **Given** Hermes sin ninguna copia de su lista y Hécate caída, **When** se consulta su salud o llega una llamada,
   **Then** no está listo, su salud dice que le falta la lista de acceso, y la API responde `unavailable`.

---

### Edge Cases

- **Una app sin nadie en su lista:** no la puede llamar nadie, también si nunca se le ha concedido nada; al pasar de
  hoy, el admin concede antes los accesos que hagan falta (FR-010).
- **Las identidades hijas** (las integraciones de Hades, bajo `hades/integration/<nombre>`): no figuran en la lista y
  cuentan como su app; están diseñadas pero no construidas, así que en esta feature ninguna llama a nada (FR-011).
- **Hermes sin ninguna copia de su lista** (recién instalado o con su copia perdida) y con Hécate caída: no está listo,
  su salud dice que le falta la lista de acceso y la API responde `unavailable` hasta tenerla (FR-009).
- **Muchas llamadas de un componente que no está en la lista:** cada una puede hacer que Hermes vuelva a pedir la
  lista, pero nunca más de una petición a la vez (FR-009).
- **Una identidad deshabilitada o con su certificado revocado:** ya la rechaza la autenticación, esté o no en la lista;
  la lista no hace falta tocarla para cortarla.
- **Conceder a una identidad que aún no se ha dado de alta:** se acepta, como en la Access Policy, y vale desde que se
  da de alta (FR-005).
- **Conceder a una app el acceso a sí misma:** no tiene sentido y se rechaza con `validation`.
- **La propia API de Hécate:** no usa esta lista; todos los componentes la necesitan para su identidad y sus secretos.
- **El canal local de administración de Hermes:** no usa esta lista; su seguridad viene del acceso local, como fija la
  constitución.
- **Una consulta repetida muchas veces:** Hécate la atiende sin dejar una fila por cada una, como el estado, para que el
  refresco de las apps no llene el registro.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hécate DEBE guardar, para cada app, la lista de identidades que pueden llamarla. El acceso DEBE ser a la
  app entera, sin permisos por función.
- **FR-002**: El admin DEBE conceder y retirar el acceso por el canal local de administración de Hécate y su CLI; la API
  de red NO DEBE ofrecer esa gestión. Las órdenes DEBEN exigir Hécate desbloqueada, NO DEBEN llevar una versión
  esperada, y una orden que no cambia nada NO DEBE crear una versión nueva. Cada orden DEBE dejar su fila en el registro
  de accesos, también si no cambia nada.
- **FR-003**: Cada cambio de la lista de una app DEBE quedar versionado con quién y cuándo. El admin DEBE poder ver la
  lista vigente de cada app, las apps que tienen lista y el historial de cada una, y DEBE poder saberse quién podía
  llamar a una app en cualquier momento pasado.
- **FR-004**: Las identidades reservadas `hecate` y `admin` NO DEBEN poder figurar como app ni como quien llama, y una
  app NO DEBE poder recibir acceso a sí misma: esas órdenes DEBEN responder `validation` sin cambiar nada.
- **FR-005**: El admin DEBE poder conceder el acceso aunque la identidad que llama, o la app, aún no se hayan dado de
  alta, como en la Access Policy: vale desde que se dan de alta. Los nombres DEBEN seguir las reglas de nombres de
  identidades de Hécate; uno bien formado que no corresponde a ningún componente queda en la lista sin efecto, y se ve
  en ella.
- **FR-006**: La API de Hécate DEBE ofrecer a una app autenticada con su certificado y su token corto la consulta de su
  lista: las identidades que pueden llamarla y la versión de la lista. La consulta DEBE devolver solo la lista de la
  identidad que pregunta, sin permitir nombrar otra app, y una lista vacía no es un error. Con Hécate sellada DEBE
  responder `unavailable`. DEBE publicarse en el contrato OpenAPI de Hécate en shared/contracts y listarse en sus
  capacidades. Una consulta atendida NO DEBE dejar fila en el registro de accesos; un rechazo, sí.
- **FR-007**: El token corto NO DEBE conceder ningún acceso: ni Hécate ni las apps DEBEN tomar en cuenta permisos, roles
  ni accesos que declare.
- **FR-008**: Hermes DEBE comprobar, después de autenticar a quien llama, que su identidad está en su copia de la lista,
  en todas las operaciones de su API de red. Si no lo está, DEBE responder `auth`, sin efectos y con el mismo mensaje
  que cualquier otro fallo de autenticación, y su log DEBE decir el motivo, con el `request_id` de la petición. El
  canal local de administración de Hermes NO DEBE usar esta lista.
- **FR-009**: Hermes DEBE mantener su copia de la lista al día con Hécate:
  - una concesión DEBE valer en la llamada siguiente: si llega una identidad autenticada que no está en su copia,
    Hermes DEBE volver a pedir la lista a Hécate antes de responder, con una sola petición a la vez, y si Hécate no
    responde a esa petición, DEBE responder `unavailable`, como con una clave que aún no tiene;
  - una retirada DEBE valer como mucho a los 240 s, con el refresco de la lista de revocación y las claves publicadas;
  - DEBE guardar la copia para arrancar con ella y seguir con la última que tiene mientras Hécate no responda o esté
    sellada;
  - sin ninguna copia (recién instalado o con la copia perdida), Hermes NO DEBE estar listo: la lista es un paso más
    del arranque, como la lista de revocación y las claves publicadas, su salud por el canal local DEBE decir que le
    falta la lista de acceso, y la API DEBE responder `unavailable` hasta que la tiene.
- **FR-010**: El acceso DEBE denegarse por defecto: una app sin nadie en su lista NO DEBE atender a ningún componente,
  también si nunca se le ha concedido nada. La feature NO DEBE dejar ningún acceso concedido de partida: antes de que
  una app aplique la lista, el admin concede los que hagan falta, y la operación de Hermes lo dice.
- **FR-011**: La lista DEBE admitir solo identidades de app: una orden que nombra una identidad hija, como app o como
  quien llama, DEBE responder `validation` sin cambiar nada. Una identidad hija cuenta como su app, aquella de la que
  es hija; como las identidades hijas están diseñadas pero no construidas, esta regla queda en el diseño de Hécate y
  en DECISIONS para quien las construya, y en esta feature ninguna hija puede autenticarse. Lo que alcanza cada
  identidad hija dentro de la app de destino es perfilado y queda fuera de esta feature.
- **FR-012**: Toda respuesta de la nueva consulta de Hécate y de Hermes, también las de error, DEBE cumplir su contrato
  publicado y el sobre común de Cerbero.
- **FR-013**: Al cerrar el cambio, DEBEN consolidarse docs/hecate/acceso.md, docs/hecate.md, docs/hermes.md,
  ARCHITECTURE, GLOSSARY y DECISIONS, donde «Acceso por app» pasa de diseñado a construido. Si el plan introduce
  tecnología nueva, DEBE decidirse en un ADR antes de usarla.

### Key Entities *(include if feature involves data)*

- **Lista de acceso de una app**: la app (una identidad de app, no una hija), las identidades de app que pueden
  llamarla, su versión y su historial de cambios.
- **Cambio de acceso**: conceder o retirar, la identidad que llama, la app, quién lo hizo y cuándo; queda en el
  historial y en el registro de accesos.
- **Copia de la lista en una app**: la lista que la app obtuvo de Hécate, con su versión y cuándo la obtuvo; es lo que
  la app aplica a cada llamada.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: El 100 % de las llamadas a Hermes de componentes autenticados que no están en su lista reciben `auth`, sin
  enviar nada ni dejar efectos.
- **SC-002**: El 100 % de las llamadas de componentes que están en la lista se atienden como antes de la feature.
- **SC-003**: Un acceso concedido en Hécate vale en Hermes en la llamada siguiente, y uno retirado deja de valer como
  mucho a los 240 s, sin reiniciar Hermes.
- **SC-004**: Con Hécate sellada o caída durante una hora, Hermes sigue aplicando su última lista: ningún componente
  gana ni pierde el acceso por ello.
- **SC-005**: Ninguna app obtiene la lista de otra app por la consulta de Hécate.
- **SC-006**: Cada concesión y cada retirada se puede atribuir a quien la hizo y fechar, y la lista de cualquier app se
  puede reconstruir en cualquier momento pasado.
- **SC-007**: El 100 % de las respuestas de la nueva consulta de Hécate y de Hermes, también las de error, cumplen su
  contrato publicado.

## Assumptions

- **Base**: `main` tras la feature 008 (`caa94c5`). Hécate tiene su API mTLS, la Access Policy y su registro de
  accesos; Hermes autentica con el certificado que reenvía Envoy y el token corto, y mantiene al día la lista de
  revocación y las claves publicadas.
- **Apps**: hoy solo Hermes atiende llamadas de otros componentes por una API, así que es la única que aplica la lista;
  las demás la consultarán igual cuando tengan API.
- **Transición**: hoy Hades no existe y a Hermes solo lo llaman componentes de prueba, así que denegar por defecto no
  corta ninguna llamada real; quien despliegue la 009 concede antes los accesos que necesite.
- **Quien gestiona**: solo el admin, por el canal local de Hécate. Los usuarios, los roles y el perfilado de las
  personas son del frontal de las apps, Ténaro.
- **Tamaño**: cada lista tiene pocas identidades (decenas como mucho), y hay pocas apps.
- **Hécate sellada**: la consulta responde `unavailable`, como el resto de la API salvo el estado; las apps siguen con
  su copia.
- **Pruebas**: las de Hécate en macOS y en Linux; las de Hermes con una Hécate real (`HECATE_BIN`), Docker y el E2E
  con Envoy.
- **Nombres concretos**: `auth`, `validation`, `unavailable`, el sobre común y el OpenAPI de shared/contracts aparecen
  porque son el contrato común de Cerbero, no una elección de esta feature.
- **Fuera de alcance**: los permisos por función; los usuarios, los roles y el perfilado de las personas; el perfilado
  de lo que alcanza cada integración o service account dentro de una app; la comprobación en otras apps; el acceso a la
  propia API de Hécate; los permisos que salen de las declaraciones; y la reestructuración de la documentación de
  Hermes, que es el cambio siguiente.
- **Rama**: `feature/009-app-access`, creada desde `main`; commit de squash `feat:`.
