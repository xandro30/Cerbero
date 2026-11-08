# Feature Specification: Identidades de Hécate

**Feature Branch**: `feature/003-hecate-identities`

**Created**: 2026-11-08

**Status**: Draft

**Input**: User description: "Identidades de Hécate: CA interna, alta de componentes con token de un solo uso, y certificados de corta duración con renovación y revocación por mTLS"

## Clarifications

### Session 2026-11-08

- Q: ¿Entran en esta feature las claves con las que cada componente firma el token corto de sus llamadas, y su
  publicación? → A: Sí: el componente registra su clave de firma de llamadas al darse de alta y al renovar, Hécate las
  publica, y la renovación, la lista de revocación y las claves publicadas exigen además ese token (constitución,
  principio V).
- Q: ¿Cómo se protege la copia de la clave de la CA raíz y dónde se hace la ceremonia? → A: En el equipo de Hécate, por
  su canal local; la clave solo existe en memoria durante la ceremonia y sale protegida con una frase propia de la CA,
  distinta de la de desbloqueo, para guardarla en dos soportes.
- Q: ¿Qué se puede hacer ante una intermedia comprometida o una clave de la CA raíz perdida? → A: Sustituir la CA entera
  (CA raíz e intermedia nuevas) y volver a dar de alta todos los componentes con tokens nuevos.
- Q: ¿Qué deben recibir los componentes cuando llaman a Hécate mientras está sellada? → A: Hécate responde con su
  certificado vigente, pero solo para decir que está sellada: alta, renovación, lista de revocación y claves se rechazan
  con ese motivo. La clave de su certificado está disponible sin desbloquear (riesgo aceptado, acotado a 3 días); si
  caduca estando sellada, deja de responder hasta desbloquearla, y al desbloquearla lo renueva sola.
- Q: ¿En cuánto tiempo como máximo debe dejar de valer en todo Cerbero un certificado revocado con urgencia, o las
  claves de una identidad deshabilitada? → A: 5 minutos: Hécate indica que la lista de revocación y las claves
  publicadas valen 5 minutos y los componentes las refrescan antes de que venzan; si no pueden refrescarlas, siguen con
  su última copia.
- Q: ¿Qué debe hacer Hécate cuando alguien intenta renovar por segunda vez un certificado ya renovado? → A: Rechazarlo,
  registrarlo como posible robo de clave, revocar con urgencia todos los certificados válidos de esa identidad
  (incluido el recién emitido) y contarlo en el estado, sin el nombre; la identidad tiene que darse de alta de nuevo con
  un token del admin.
- Q: ¿Se pueden seguir guardando secretos para las identidades reservadas `hecate` y `admin`, que nunca podrán
  leerlos? → A: No: guardar un secreto o una versión nueva en una ruta cuya identidad está reservada se rechaza; el
  resto sigue igual y se pueden preparar secretos antes del alta de un componente.
- Q: ¿Los cambios del admin sobre identidades, certificados y tokens deben llevar la versión esperada, como los de los
  secretos? → A: No: son órdenes que no dependen de lo leído y funcionan al momento; la atomicidad evita estados a
  medias, y para cortar a un componente se deshabilita su identidad. La regla de docs/hecate.md se matiza al consolidar.

## User Scenarios & Testing *(mandatory)*

Cada componente de Cerbero necesita una identidad para hablar con los demás: un certificado de corta duración cuyo
único nombre es su identidad (`spiffe://<trust domain>/<componente>`), emitido por la autoridad de certificados propia
de Cerbero (la CA interna). La CA tiene dos niveles: una CA raíz que vive fuera de Hécate y solo se usa en ceremonias, y
una intermedia que Hécate custodia y con la que firma. Además del certificado, cada llamada entre componentes lleva un
token corto firmado por quien llama con una clave propia, distinta de la del certificado (constitución, principio V);
Hécate registra y publica esas claves. Esta feature cubre crear la CA, dar de alta componentes con un token de un solo
uso, renovar sus certificados, publicar sus claves de firma de llamadas, revocar, y renovar o sustituir la CA.

En esta spec, «token» a secas es el token de alta; el que acompaña a cada llamada es siempre el «token corto». «CA
raíz» es la autoridad raíz de la CA de Cerbero, y «raíz de cifrado», la clave raíz de Hécate de la feature 001.

Actores:

- **Admin**: el único usuario privilegiado; crea, renueva y sustituye la CA, genera los tokens de alta, revoca
  certificados y deshabilita identidades.
- **Componente**: una pieza de Cerbero (p. ej. Hermes o Hades) que necesita identidad. Genera sus claves privadas en su
  propio equipo, se da de alta con un token y renueva su certificado antes de que caduque.

Todas las operaciones exigen Hécate desbloqueada (feature 001), salvo los recuentos del estado.

### User Story 1 - Crear la CA de Cerbero (Priority: P1)

El admin crea la CA en una ceremonia, en el equipo de Hécate, y elige el trust domain del entorno (p. ej.
`cerbero.home`) y una frase propia de la CA. Se genera la CA raíz, que firma una intermedia para Hécate. La clave de la
CA raíz solo existe durante la ceremonia y sale de ella protegida con esa frase, para que el admin la guarde fuera de
Hécate en dos soportes distintos; Hécate se queda con el certificado de la CA raíz y con la intermedia, cuya clave
protege su raíz de cifrado.

**Why this priority**: sin CA no se puede emitir ninguna identidad; es la base del resto.

**Independent Test**: hacer la ceremonia y comprobar que la intermedia está firmada por la CA raíz, que la clave de la
CA raíz solo sale protegida con la frase de la CA, que Hécate no la guarda en ningún sitio y que la clave de la
intermedia nunca está en claro en disco.

**Acceptance Scenarios**:

1. **Given** Hécate desbloqueada y sin CA, **When** el admin hace la ceremonia con un trust domain válido y una frase de
   la CA válida, **Then** quedan creadas la CA raíz y la intermedia, y el admin recibe la clave de la CA raíz protegida
   con esa frase y la huella de la CA raíz.
2. **Given** una frase de la CA de menos de 12 caracteres o igual a la frase de desbloqueo, **When** el admin hace la
   ceremonia, **Then** se rechaza sin crear nada.
3. **Given** la ceremonia terminada, **When** se inspecciona lo que Hécate guarda y registra, **Then** no aparecen la
   clave de la CA raíz ni la frase de la CA, y la clave de la intermedia solo aparece cifrada.
4. **Given** una ceremonia interrumpida antes de terminar (p. ej. por un corte de luz), **When** Hécate vuelve,
   **Then** no queda nada guardado y la ceremonia se puede repetir.
5. **Given** una CA ya creada, **When** el admin intenta crearla otra vez, **Then** se rechaza: la CA solo cambia
   renovando la intermedia (User Story 6) o sustituyéndola (User Story 7).
6. **Given** Hécate sellada, **When** se intenta la ceremonia, **Then** se rechaza indicando que está sellada.

---

### User Story 2 - Dar de alta un componente (Priority: P1)

El admin genera un token de un solo uso para una identidad (p. ej. `hermes`) y se lo entrega al componente. El
componente genera sus claves (la de su certificado y la de firma de sus llamadas), comprueba con la huella de la CA raíz
que lleva el token que habla con la Hécate correcta y pide su primer certificado con el token. Hécate lo emite con la
identidad del token como único nombre, registra la clave de firma de llamadas del componente y marca el token como
usado.

**Why this priority**: es como un componente obtiene su identidad por primera vez; sin alta no hay nada que renovar.

**Independent Test**: generar un token, dar de alta un componente de prueba y comprobar que recibe un certificado de 3
días con solo su identidad, que su clave de firma de llamadas queda registrada, que el token no sirve una segunda vez y
que ninguna clave privada del componente llega a Hécate.

**Acceptance Scenarios**:

1. **Given** un token válido para `hermes`, **When** el componente pide su certificado con el token, una solicitud
   firmada con la clave de su certificado y la clave pública con la que firmará sus llamadas, **Then** recibe un
   certificado de 3 días cuyo único nombre es `spiffe://<trust domain>/hermes`, junto con la cadena hasta la CA raíz; su
   clave de firma de llamadas queda registrada y el token queda usado.
2. **Given** un token usado, caducado o anulado, **When** alguien lo presenta en una petición nueva, **Then** se
   rechaza sin emitir nada y queda registrado; solo la repetición exacta de la petición con la que se usó devuelve el
   mismo certificado.
3. **Given** un token para `hermes`, **When** la solicitud pide otra identidad, **Then** se rechaza sin emitir nada.
4. **Given** un token recién generado, **When** el admin lo recibe, **Then** es la única vez que se muestra: Hécate no
   puede volver a mostrarlo.
5. **Given** un componente con su token, **When** comprueba a Hécate antes de enviarlo, **Then** la identidad que
   presenta Hécate encadena con la CA raíz cuya huella lleva el token.
6. **Given** una identidad deshabilitada, **When** el admin pide un token para ella, **Then** se rechaza.
7. **Given** tokens pendientes, **When** el admin los lista, **Then** ve la identidad y la caducidad de cada uno, sin
   su valor, y puede anular cualquiera; un token anulado ya no sirve.
8. **Given** una identidad que ya tiene certificados, **When** se da de alta otra vez con un token nuevo, **Then**
   recibe un certificado nuevo y los anteriores ya no se renuevan.

---

### User Story 3 - Renovar el certificado (Priority: P1)

Antes de que caduque su certificado, el componente pide uno nuevo identificándose con el actual y con un token corto
firmado con su clave de firma de llamadas registrada, y aporta claves nuevas. Hécate comprueba que el certificado lo
emitió ella, que es válido y no está revocado ni renovado, que el token corto es válido y que la identidad sigue
habilitada; emite otro certificado de 3 días con la misma identidad y registra la clave de firma de llamadas nueva.

**Why this priority**: con certificados de 3 días, sin renovación todas las identidades caerían a los 3 días del alta.

**Independent Test**: dar de alta un componente de prueba, renovar y comprobar que el certificado nuevo tiene la misma
identidad, otra clave y 3 días de vida, que la clave de firma de llamadas nueva queda registrada y que cada caso
inválido se rechaza.

**Acceptance Scenarios**:

1. **Given** un certificado válido de `hermes`, **When** el componente renueva identificándose con él y con un token
   corto válido, y aporta una clave de certificado y una clave de firma de llamadas nuevas, **Then** recibe un
   certificado nuevo de 3 días con la misma identidad y su clave de firma de llamadas nueva queda registrada.
2. **Given** un certificado caducado, revocado, de una identidad deshabilitada, de otro trust domain o que Hécate no
   emitió con la CA raíz vigente, **When** se intenta renovar con él, **Then** se rechaza y queda registrado.
3. **Given** una petición sin token corto, o con uno caducado, ya usado, firmado con otra clave o cuyo emisor no es la
   identidad del certificado, **When** se intenta renovar, **Then** se rechaza y queda registrado.
4. **Given** una solicitud que repite la clave del certificado o la clave de firma de llamadas actuales, o que pide otra
   identidad, **When** se intenta renovar, **Then** se rechaza.
5. **Given** un certificado ya renovado, **When** se intenta renovar otra vez con él, **Then** se rechaza, queda
   registrado como posible robo de su clave y todos los certificados válidos de esa identidad, incluido el recién
   emitido, quedan revocados con urgencia; repetir la misma petición con un token corto nuevo, en cambio, devuelve el
   mismo certificado sin revocar nada.
6. **Given** Hécate sellada, **When** un componente intenta renovar, **Then** Hécate responde que está sellada, no
   emite nada y el componente conserva su certificado actual hasta que caduque.
7. **Given** una identidad habilitada cuyo certificado más reciente caduca en menos de 24 horas, **When** se consulta
   el estado de Hécate, **Then** la cuenta, sin su nombre, también con Hécate sellada.
8. **Given** un intento de renovación sospechoso (escenario 5) en las últimas 24 horas, **When** se consulta el estado
   de Hécate, **Then** lo cuenta, sin el nombre de la identidad, también con Hécate sellada.

---

### User Story 4 - Publicar las claves de firma de llamadas (Priority: P2)

Un componente que recibe una llamada de otro comprueba el token corto que la acompaña con las claves que publica
Hécate. Cada clave vale para verificar llamadas mientras su certificado es válido y la identidad está habilitada;
después, Hécate la conserva solo para verificar firmas pasadas.

**Why this priority**: sin ellas, ningún componente puede comprobar quién le llama; va detrás de la renovación porque
Hécate ya verifica los tokens cortos que recibe con su propio registro.

**Independent Test**: dar de alta y renovar componentes de prueba, pedir las claves publicadas y comprobar cuáles
valen para verificar llamadas, que las de certificados caducados solo sirven para firmas pasadas y que las de una
identidad deshabilitada dejan de valer al instante en Hécate, con lo publicado válido como mucho 5 minutos.

**Acceptance Scenarios**:

1. **Given** componentes dados de alta, **When** uno pide las claves publicadas identificándose con su certificado y un
   token corto válido, **Then** recibe las claves de firma de llamadas registradas, cada una con su identidad y si vale
   para verificar llamadas.
2. **Given** un componente que renovó, **When** se piden las claves, **Then** su clave nueva vale para verificar
   llamadas, la anterior sigue valiendo hasta que caduca su certificado y después solo sirve para firmas pasadas.
3. **Given** una identidad deshabilitada o un certificado revocado con urgencia, **When** se piden las claves,
   **Then** sus claves dejan de valer para llamadas nuevas desde ese momento.
4. **Given** una petición sin certificado válido o sin token corto válido, **When** se piden las claves, **Then** se
   rechaza.
5. **Given** las claves publicadas, **When** un componente las recibe, **Then** indican que valen como mucho 5
   minutos.

---

### User Story 5 - Revocar un certificado o deshabilitar una identidad (Priority: P2)

Si un componente se compromete o se retira, el admin revoca su certificado o deshabilita su identidad. Por defecto la
revocación es pasiva: el certificado ya no se renueva y deja de valer al caducar (como mucho, 3 días). Si es urgente,
además entra en una lista corta de certificados revocados que los componentes consultan, y su clave de firma de
llamadas deja de valer: todo Cerbero lo sabe en 5 minutos como mucho.

**Why this priority**: es la respuesta ante un compromiso; con certificados de 3 días la revocación pasiva ya acota el
daño, por eso va detrás del alta y la renovación.

**Independent Test**: revocar certificados de forma pasiva y urgente y deshabilitar una identidad, y comprobar que
nada de eso se puede renovar, que la lista de revocación contiene justo los revocados con urgencia que no han caducado
y que una identidad deshabilitada no puede darse de alta.

**Acceptance Scenarios**:

1. **Given** un certificado válido, **When** el admin lo revoca sin urgencia, **Then** ya no se renueva, no aparece en
   la lista de revocación y su clave de firma de llamadas sigue valiendo hasta que caduca.
2. **Given** un certificado válido, **When** el admin lo revoca con urgencia, **Then** ya no se renueva, aparece en la
   lista de revocación hasta que caduca y su clave de firma de llamadas deja de valer para llamadas nuevas.
3. **Given** una identidad habilitada, **When** el admin la deshabilita, **Then** ninguno de sus certificados se
   renueva, sus claves de firma de llamadas dejan de valer para llamadas nuevas, sus tokens pendientes quedan anulados,
   no se pueden generar tokens para ella y, si lo pide con urgencia, sus certificados válidos entran en la lista de
   revocación.
4. **Given** una identidad deshabilitada, **When** el admin la habilita de nuevo, **Then** puede volver a darse de
   alta, pero solo con un token nuevo.
5. **Given** un componente, **When** pide la lista de revocación identificándose con su certificado y un token corto
   válido, **Then** la recibe firmada por la CA de Cerbero e indicando que vale como mucho 5 minutos; sin ellos, se
   rechaza.
6. **Given** identidades dadas de alta, **When** el admin las lista y consulta una, **Then** ve su estado, la caducidad
   de su certificado más reciente y, por certificado, número de serie, validez y estado, sin claves privadas ni tokens.

---

### User Story 6 - Renovar la CA intermedia (Priority: P3)

Antes de que caduque la intermedia, el admin hace otra ceremonia con una copia de la clave de la CA raíz y la frase de
la CA: la CA raíz firma una intermedia nueva, con la que Hécate emite desde ese momento. Los certificados emitidos con
la anterior siguen valiendo hasta caducar.

**Why this priority**: pasa una vez al año, pero si se olvida, Hécate deja de poder emitir; el estado avisa con
antelación.

**Independent Test**: renovar la intermedia y comprobar que las altas y renovaciones nuevas usan la nueva, que los
certificados emitidos con la anterior siguen siendo válidos hasta caducar y que el estado avisaba de la caducidad.

**Acceptance Scenarios**:

1. **Given** una CA creada, una copia de la clave de la CA raíz y la frase de la CA, **When** el admin renueva la
   intermedia, **Then** las emisiones siguientes usan la intermedia nueva y la anterior deja de emitir.
2. **Given** certificados emitidos con la intermedia anterior, **When** se renueva la intermedia, **Then** siguen
   siendo válidos hasta caducar y se pueden renovar, obteniendo uno de la intermedia nueva.
3. **Given** una clave de CA raíz que no corresponde a la CA de Hécate, o una frase de la CA incorrecta, **When** se
   intenta renovar la intermedia, **Then** se rechaza sin cambiar nada.
4. **Given** una intermedia que caduca en menos de 30 días, **When** se consulta el estado de Hécate, **Then** lo
   indica, también con Hécate sellada.

---

### User Story 7 - Sustituir la CA (Priority: P3)

Si la intermedia se compromete, o se pierden la clave de la CA raíz o la frase de la CA, el admin sustituye la CA entera
en una nueva ceremonia: CA raíz e intermedia nuevas, con una frase de la CA nueva. Desde ese momento Hécate solo acepta
la CA nueva y cada componente vuelve a darse de alta con un token nuevo.

**Why this priority**: es un caso raro y caro (repetir todas las altas), pero sin él una intermedia comprometida
permitiría emitir identidades falsas hasta que caducase, y una CA raíz perdida dejaría a Hécate sin poder renovar la
intermedia.

**Independent Test**: sustituir la CA con componentes dados de alta y comprobar que sus certificados ya no se
renuevan, que los tokens pendientes ya no sirven, que un alta nueva obtiene un certificado de la CA nueva y que sin
la prueba no se sustituye nada.

**Acceptance Scenarios**:

1. **Given** una CA con componentes dados de alta, **When** el admin la sustituye demostrándolo con la frase actual de
   Hécate o con dos partes y una frase de la CA nueva, **Then** quedan una CA raíz y una intermedia nuevas, el admin
   recibe la clave de la CA raíz nueva protegida y su huella, y Hécate se identifica con un certificado de la CA nueva.
2. **Given** la CA sustituida, **When** un componente intenta renovar con un certificado de la CA anterior, **Then** se
   rechaza y sus claves de firma de llamadas ya no valen; con un token nuevo, que lleva la huella de la CA raíz nueva,
   se da de alta y obtiene un certificado de la CA nueva.
3. **Given** tokens pendientes antes de la sustitución, **When** alguien los presenta después, **Then** se rechazan.
4. **Given** un intento de sustitución sin una prueba válida, **When** se pide, **Then** se rechaza sin cambiar nada y
   cuenta como intento fallido en el contador común.

---

### Edge Cases

- Dos altas simultáneas con el mismo token: solo una obtiene certificado.
- Dos renovaciones distintas del mismo certificado, a la vez o una tras otra (un robo de clave o un fallo del
  componente): solo la primera obtiene certificado; la segunda se trata como un intento sospechoso (User Story 3,
  escenario 5) y la identidad necesita un alta nueva.
- Se pierde la respuesta de un alta o de una renovación: repetir la misma petición devuelve el mismo certificado, sin
  gastar otro token de alta ni contar como segundo intento.
- Se va la luz a mitad de una ceremonia, un alta, una renovación, una revocación o una sustitución de la CA: queda el
  estado anterior o el nuevo, nunca a medias.
- Hécate sellada: responde con su certificado vigente, pero solo para decir que está sellada; no emite, renueva ni
  revoca, no ofrece la lista de revocación ni las claves publicadas, y los componentes conservan sus certificados hasta
  que caducan. Si su propio certificado caduca estando sellada, deja de responder hasta que se desbloquea.
- Hécate sellada más de 3 días, o un componente apagado más de 3 días: sus certificados caducan y cada componente
  afectado necesita un token nuevo del admin.
- La intermedia caduca sin renovarla: Hécate no emite hasta renovarla. Ningún certificado caduca después que la
  intermedia que lo firmó.
- Un certificado de otro trust domain (p. ej. `cerbero.test` en casa) o que no encadena con la CA raíz vigente: se
  rechaza en cualquier operación.
- Una identidad mal formada (vacía, con mayúsculas o caracteres no permitidos) o reservada (`hecate`, `admin`): se
  rechaza al generar el token.
- Un secreto en una ruta de una identidad reservada (p. ej. `secret://admin/...`): se rechaza al guardarlo. Si ya
  existía alguno de antes, el admin puede revocarlo y destruirlo, pero no añadirle versiones.
- Se pierden las copias de la clave de la CA raíz o la frase de la CA: no se puede renovar la intermedia; la salida es
  sustituir la CA (User Story 7).
- Relojes desincronizados más de lo que aguantan los tokens cortos (minutos): las llamadas se rechazan; por eso los
  equipos deben estar sincronizados.
- El admin revoca un certificado que el componente acaba de renovar: se revoca ese certificado, no el nuevo. Para cortar
  a un componente se deshabilita su identidad, que cubre todos sus certificados.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: El admin MUST poder crear la CA en una ceremonia: una CA raíz que firma una intermedia para Hécate, dentro
  del trust domain que elige. Hécate tiene una sola CA.
- **FR-002**: La ceremonia MUST hacerse en el equipo de Hécate, por su canal local, y la clave de la CA raíz MUST NOT
  guardarse en Hécate: solo existe en memoria durante la ceremonia y sale de ella protegida con una frase propia de la
  CA, de al menos 12 caracteres y distinta de la de desbloqueo, para guardarla en dos soportes distintos. Hécate guarda
  el certificado de la CA raíz y su huella, nunca la frase de la CA.
- **FR-003**: La clave de la intermedia MUST guardarse solo cifrada bajo la raíz de cifrado de Hécate, y solo se usa
  con Hécate desbloqueada.
- **FR-004**: Una ceremonia interrumpida MUST NOT dejar nada guardado ni cambiar la CA vigente.
- **FR-005**: El admin MUST poder renovar la intermedia con una copia de la clave de la CA raíz y la frase de la CA: la
  intermedia nueva emite desde ese momento y los certificados de la anterior siguen valiendo hasta caducar.
- **FR-006**: El admin MUST poder sustituir la CA entera (CA raíz e intermedia nuevas, con una frase de la CA nueva)
  ante una intermedia comprometida o la pérdida de la clave de la CA raíz o de su frase, demostrándolo con la frase
  actual de Hécate o con dos partes (un fallo cuenta en el contador común de intentos). Desde ese momento Hécate solo
  acepta certificados de la CA nueva, anula los tokens pendientes y deja sin renovación todos los certificados
  anteriores; cada componente vuelve a darse de alta con un token nuevo.
- **FR-007**: El admin MUST poder generar un token de alta para una identidad: de un solo uso, que caduca a los 15
  minutos, ligado a esa identidad y con la huella de la CA raíz. Se muestra una sola vez y Hécate guarda solo una forma
  que no permite recuperarlo. El admin MUST poder listar los tokens pendientes, sin su valor, y anularlos. Los nombres
  `hecate` (la identidad de Hécate) y `admin` (quien figura en el registro de accesos por las operaciones del admin)
  están reservados: no se generan tokens para ellos.
- **FR-008**: Un componente MUST poder obtener su primer certificado presentando un token válido, una solicitud
  firmada con la clave de su certificado y la clave pública con la que firmará sus llamadas, distinta de la del
  certificado. Hécate MUST NOT recibir nunca una clave privada de un componente.
- **FR-009**: Cada certificado MUST llevar como único nombre la identidad `spiffe://<trust domain>/<identidad>` y servir
  al componente para identificarse tanto al llamar como al recibir llamadas. Dura 3 días y nunca más que la intermedia
  que lo firma.
- **FR-010**: La identidad del certificado MUST decidirla Hécate: la del token en el alta y la del certificado
  presentado en la renovación. Una solicitud que pida otra identidad se rechaza.
- **FR-011**: Un token de alta MUST quedar usado al emitir el certificado, de forma atómica: dos altas con el mismo
  token nunca obtienen dos certificados.
- **FR-012**: Hécate MUST identificarse ante los componentes con un certificado de su propia identidad
  (`spiffe://<trust domain>/hecate`) emitido por la CA vigente, de modo que un componente pueda comprobarla con la
  huella de la CA raíz antes de enviar su token. Lo renueva ella sola mientras está desbloqueada, y al desbloquearla si
  caducó estando sellada. La clave de ese certificado, y solo esa, MUST estar disponible sin desbloquear, para que
  Hécate pueda responder sellada.
- **FR-013**: Una nueva alta de una identidad MUST dejar sin renovación los certificados anteriores de esa identidad.
- **FR-014**: Un componente MUST poder renovar su certificado mientras sea válido, aportando una clave de certificado y
  una clave de firma de llamadas nuevas. La renovación MUST rechazarse si el certificado caducó, está revocado, no lo
  emitió Hécate con la CA raíz vigente, su identidad está deshabilitada o alguna de las claves nuevas repite la actual.
- **FR-015**: Un certificado MUST renovarse una sola vez. Un segundo intento con el mismo certificado se rechaza, se
  registra como posible robo de su clave y revoca con urgencia todos los certificados válidos de esa identidad,
  incluido el recién emitido; la identidad solo vuelve a tener certificado dándose de alta con un token nuevo. Repetir
  la misma petición de alta (con el mismo token de alta) o de renovación (con un token corto nuevo) MUST devolver el
  mismo resultado sin efectos nuevos y no cuenta como segundo intento.
- **FR-016**: La renovación, la lista de revocación y las claves publicadas MUST exigir, además de un certificado válido
  del componente, un token corto firmado con su clave de firma de llamadas registrada, cuyo emisor es la identidad del
  certificado de la conexión, que caduca en minutos y que no se acepta dos veces.
- **FR-017**: Hécate MUST publicar para los componentes las claves de firma de llamadas registradas, cada una con su
  identidad: vale para verificar llamadas mientras su certificado es válido y la identidad está habilitada, y después
  solo para verificar firmas pasadas. La clave de un certificado revocado con urgencia o de una identidad deshabilitada
  deja de valer para llamadas nuevas en Hécate en ese momento. Lo publicado MUST indicar que vale como mucho 5 minutos,
  de modo que todo Cerbero deje de aceptar esa clave en ese plazo.
- **FR-018**: El admin MUST poder revocar un certificado: por defecto de forma pasiva (ya no se renueva, pero sigue
  siendo válido hasta caducar, igual que su clave de firma de llamadas); con urgencia, deja de ser válido en ese momento
  y entra en la lista de revocación hasta que caduca.
- **FR-019**: El admin MUST poder deshabilitar una identidad (sus certificados ya no se renuevan, sus claves de firma de
  llamadas dejan de valer para llamadas nuevas, sus tokens pendientes quedan anulados, no se generan tokens para ella
  y, con urgencia, sus certificados válidos entran en la lista de revocación) y volver a habilitarla; una identidad
  habilitada de nuevo solo vuelve a tener certificado dándose de alta con un token nuevo.
- **FR-020**: Hécate MUST ofrecer a los componentes la lista de los certificados revocados con urgencia que aún no han
  caducado, firmada por la CA. La lista MUST indicar que vale como mucho 5 minutos, de modo que un certificado revocado
  con urgencia deje de aceptarse en todo Cerbero en ese plazo.
- **FR-021**: Hécate MUST rechazar en cualquier operación los certificados de otro trust domain o que no encadenen con
  la CA raíz vigente.
- **FR-022**: El admin MUST poder listar las identidades con su estado y la caducidad de su certificado más reciente, y
  consultar cada una con sus certificados (número de serie, validez y estado), sin claves privadas ni tokens.
- **FR-023**: Todas las operaciones de esta feature MUST exigir Hécate desbloqueada, salvo los recuentos del estado
  (FR-025). Sellada, Hécate MUST responder a los componentes, con su certificado vigente, que está sellada, sin emitir,
  renovar ni ofrecer la lista de revocación ni las claves publicadas.
- **FR-024**: Hécate MUST registrar en el registro de accesos cada ceremonia, renovación o sustitución de la CA, token
  de alta generado o anulado, alta, renovación, revocación, cambio de estado de una identidad, consulta y rechazo, con
  quién (admin, identidad o la propia Hécate, como en la revocación automática de FR-015 o la renovación de su propio
  certificado), qué identidad y qué certificado. Nunca un token, una clave privada ni la frase de la CA.
- **FR-025**: El estado de Hécate MUST exponer, también sellada y sin nombres de identidades: si la CA existe, cuántas
  identidades habilitadas tienen su certificado más reciente a menos de 24 horas de caducar, cuántos intentos de
  renovación sospechosos (FR-015) hubo en las últimas 24 horas y los días que le quedan a la intermedia cuando son menos
  de 30.
- **FR-026**: Cada cambio de esta feature MUST aplicarse de forma atómica: un corte deja el estado anterior o el nuevo.
  Los cambios del admin sobre identidades, certificados y tokens MUST aplicarse sin exigir la versión esperada: son
  órdenes cuyo resultado no depende de lo que el admin leyó antes, y las identidades cambian solas al renovar.
- **FR-027**: Guardar un secreto o una versión nueva (feature 002) en una ruta cuya identidad está reservada (`hecate`,
  `admin`) MUST rechazarse. Para el resto de identidades se pueden seguir preparando secretos antes de su alta.

### Key Entities *(include if feature involves data)*

- **CA raíz**: la autoridad de confianza de un trust domain. Su certificado y su huella los conocen Hécate y los
  componentes; su clave vive fuera de Hécate, protegida con la frase de la CA. Al sustituir la CA, una CA raíz nueva
  reemplaza a la anterior.
- **CA intermedia**: firmada por la CA raíz y custodiada por Hécate, que firma con ella. Dura 1 año; al renovarla, la
  anterior deja de emitir.
- **Identidad**: un componente dentro de un trust domain (`spiffe://<trust domain>/<nombre>`), con estado `enrolled`
  (dada de alta y habilitada) o `disabled` (deshabilitada). Nace con su primera alta; habilitarla de nuevo la devuelve a
  `enrolled`, pero necesita un token nuevo para volver a tener certificado. Los nombres `hecate` y `admin` están
  reservados.
- **Token de alta**: de un solo uso, ligado a una identidad, con caducidad y estado (pendiente, usado, anulado o
  caducado). Hécate no guarda su valor.
- **Certificado**: de una identidad, con número de serie, validez de 3 días, estado (`valid`, `revoked`, `expired`) y si
  se puede renovar (deja de poderse al renovarlo, al revocarlo, al volver a dar de alta su identidad o al sustituir la
  CA). Es válido mientras no ha caducado, no está revocado con urgencia y encadena con la CA raíz vigente: la
  revocación pasiva solo impide renovarlo y no cambia su estado; la urgente lo pasa a `revoked`.
- **Clave de firma de llamadas**: la clave pública con la que un componente firma el token corto de sus llamadas. Se
  registra con cada certificado y vale para verificar llamadas mientras ese certificado es válido y la identidad está
  habilitada; después, solo para verificar firmas pasadas.
- **Lista de revocación**: los certificados revocados con urgencia que aún no han caducado, firmada por la CA y válida
  5 minutos.
- **Registro de accesos**: el de las features 001 y 002, ampliado con las operaciones de esta feature.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Con Hécate desbloqueada, un alta y una renovación tardan cada una menos de 1 segundo desde que el
  componente envía la petición.
- **SC-002**: El 100 % de los intentos con un token de alta usado, caducado, anulado o de otra identidad se rechazan sin
  emitir nada y quedan registrados, salvo la repetición exacta de la petición con la que se usó, que devuelve el mismo
  certificado.
- **SC-003**: Ninguna inspección de lo que Hécate guarda o registra encuentra la clave de la CA raíz, la frase de la CA,
  la clave de la intermedia sin cifrar, un token de alta en claro ni una clave privada de un componente.
- **SC-004**: Ningún certificado emitido lleva otra identidad que la que le corresponde, y ningún certificado de otro
  trust domain o que no encadene con la CA raíz vigente se acepta.
- **SC-005**: Un certificado revocado o de una identidad deshabilitada no se renueva en ningún caso. Uno revocado con
  urgencia está en la lista de revocación desde que se revoca hasta que caduca, y en 5 minutos como mucho ningún
  componente que refresque lo que publica Hécate acepta ya ese certificado ni su clave de firma de llamadas; lo mismo
  con las claves de una identidad deshabilitada.
- **SC-006**: El 100 % de las llamadas de un componente a Hécate sin certificado válido, sin token corto válido, con un
  token corto repetido o con un emisor distinto de la identidad del certificado se rechazan y quedan registradas.
- **SC-007**: Un corte de luz en cualquier punto de una ceremonia, un alta, una renovación, una revocación o una
  sustitución de la CA deja el estado anterior o el nuevo, y un componente cuyo token de alta llegó a usarse recupera
  su certificado repitiendo la petición.
- **SC-008**: Un componente que renueva a los dos tercios de la vida de su certificado mantiene su identidad sin
  intervención del admin mientras Hécate esté desbloqueada; con Hécate sellada, la conserva hasta que caduca su último
  certificado.
- **SC-009**: Tras renovar la intermedia, las altas y renovaciones siguen funcionando sin cambios en los componentes, y
  los certificados emitidos con la anterior siguen valiendo hasta caducar.
- **SC-010**: Tras sustituir la CA, ningún certificado ni token de alta de la CA anterior se acepta, y un componente con
  un token nuevo vuelve a tener identidad en menos de 1 segundo desde que lo presenta.
- **SC-011**: Tras un segundo intento de renovar el mismo certificado, en 5 minutos como mucho ningún componente que
  refresque lo que publica Hécate acepta ya ningún certificado de esa identidad emitido antes del intento, y el estado
  de Hécate lo cuenta.

## Assumptions

- **Trust domain**: uno por Hécate, elegido al crear la CA (`cerbero.home` en casa, `cerbero.test` en pruebas);
  sustituir la CA mantiene el trust domain.
- **Nombre de una identidad**: la misma regla que el primer segmento de las rutas de secretos (minúsculas, dígitos y
  `-`, empezando por letra, hasta 63 caracteres), de modo que la identidad `hermes` es la dueña de
  `secret://hermes/...`; las identidades reservadas no pueden tener secretos (FR-027).
- **Duraciones**: certificados de 3 días, tokens de alta de 15 minutos, tokens cortos de las llamadas de pocos minutos,
  intermedia de 1 año y CA raíz de unos 10 años; las de certificados y tokens de alta son configurables.
- **Renovación a los dos tercios**: el momento lo decide el componente; Hécate acepta la renovación en cualquier momento
  mientras el certificado sea válido.
- **Relojes sincronizados** (NTP) en todos los equipos, como fija ARCHITECTURE: los tokens cortos solo valen unos
  minutos.
- **Token corto de las llamadas**: lo firma el componente con su clave de firma de llamadas registrada y lleva su
  identidad como emisor, a Hécate como destinatario, una caducidad de minutos y un identificador único. Los permisos que
  llevará para otras operaciones se evalúan con la Access Policy (feature siguiente).
- **El alta no lleva token corto ni certificado de cliente**, porque el componente aún no tiene ninguno: la autentica
  el token de alta de un solo uso. Es la excepción inherente al arranque frente al principio V de la constitución, y
  el plan la justificará.
- **Frase de la CA**: la elige y la guarda el admin; Hécate no la guarda. Si se pierde, la salida es sustituir la CA
  (User Story 7). Un fallo con ella queda registrado pero no cuenta en el contador común de intentos: no protege la
  raíz de cifrado de Hécate, sino una copia que el admin guarda fuera.
- **Refresco de lo publicado**: los componentes refrescan la lista de revocación y las claves publicadas antes de que
  venzan (5 minutos); si Hécate no está disponible, siguen con su última copia (constitución, principio VI).
- **Claves de firma de llamadas antiguas**: se conservan para verificar firmas pasadas durante la retención de la
  auditoría (un año o más, configurable, como fija docs/hecate.md).
- **El lado del componente** (generar y guardar sus claves, darse de alta, renovar, firmar sus llamadas, verificar las
  de los demás, consultar la lista de revocación y comprobar a quien le llama) lo implementa cada componente contra el
  contrato que publica Hécate, porque los componentes no comparten código (constitución, principio I). Esta feature lo
  prueba con un componente de prueba.
- **Primera API de Hécate entre componentes**: el alta, la renovación, la lista de revocación y las claves publicadas
  siguen el contrato común entre componentes (constitución, principio II). Las operaciones del admin van por el canal
  local de la feature 001.
- **El admin opera desde el equipo de Hécate**, por el canal local; la gestión desde Ténaro llegará con Ténaro.
- **Riesgo aceptado**: la clave del certificado propio de Hécate no depende del desbloqueo, para que Hécate pueda
  responder sellada. Quien se lleve el disco podría hacerse pasar por Hécate mientras ese certificado sea válido (como
  mucho 3 días); se registrará en DECISIONS. El adapter de hardware (fuera de alcance) permitiría protegerla sin
  desbloquear.
- **Fuera de alcance**: la Access Policy y la resolución de secretos por el canal autenticado (feature siguiente), la
  consulta del estado por los componentes (ámbito `status`, con la Access Policy), las identidades hijas de las
  integraciones de Hades, `wrap`/`unwrap`, la firma de checkpoints de auditoría, el desbloqueo desde Ténaro, el adapter
  de hardware y los avisos (los emite un cronjob de Hades a partir del estado).
- **Una sola instancia de Hécate**, sin alta disponibilidad; decenas de identidades.
- **Consolidación**: al cerrar la feature se actualizan docs/hecate.md, ARCHITECTURE, GLOSSARY y DECISIONS donde hoy
  contradicen esta spec: las claves publicadas solo para componentes autenticados (docs las da como públicas), el alta
  sin certificado de cliente («siempre por mTLS»), la operación de alta en la tabla de contratos, sustituir la CA ante
  una intermedia comprometida, la ceremonia en el equipo de Hécate con la clave de la CA raíz guardada fuera («ceremonia
  offline»), el canal de resolución con la Access Policy, el token de alta también para volver a dar de alta una
  identidad, la respuesta de Hécate sellada con su riesgo aceptado y la versión esperada solo en los secretos.
- **Referencias de diseño**: [docs/hecate.md](../../docs/hecate.md) (alta y renovación de una identidad, operación),
  [ARCHITECTURE](../../ARCHITECTURE.md#6-identidad-y-seguridad) (identidad y seguridad) y las desviaciones de Hécate
  frente a step-ca y SPIFFE en [DECISIONS](../../DECISIONS.md).
