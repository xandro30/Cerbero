# Feature Specification: Identidades de Hécate

**Feature Branch**: `feature/003-hecate-identities`

**Created**: 2026-11-09

**Status**: Draft

**Input**: User description: "Reiniciar la feature 003 (identidades de Hécate), pausada en DECISIONS hasta que se
fusionara la migración, sobre la Hécate en Rust que deja el cambio 005. La entrada es la spec pausada (commit aa25083 de
la rama feature/003-hecate-identities): se conservan su alcance, sus decisiones y sus 8 aclaraciones del 2026-11-08,
salvo donde los cambios 004 y 005 las afectan. El alcance sigue siendo la CA interna en dos niveles con su ceremonia, los
tokens de alta de un solo uso, los certificados SPIFFE de 3 días que se renuevan una sola vez (un segundo intento revoca
con urgencia los de esa identidad), las claves de firma de llamadas publicadas a componentes autenticados, la revocación
y la lista de revocación, deshabilitar identidades, renovar la intermedia, sustituir la CA y la respuesta de Hécate
sellada con su propio certificado, cuya clave no depende del desbloqueo (riesgo aceptado). De los cambios hay que tener
en cuenta la constitución 1.2.0 y las garantías del 005, que se extienden al material nuevo: la frase de la CA, las
claves de la CA raíz, de la intermedia y del certificado de Hécate y los tokens se borran de la memoria al dejar de
usarse (FR-007 del 005), y las entradas nuevas, como solicitudes y certificados en PEM, JSON y fechas, siguen su formato
estándar (FR-018 del 005). Se cumplen las comprobaciones de calidad del ADR 0006 con la política de licencias del ADR
0008, y la lista de lo que se consolida al cerrar se revisa contra los docs actuales. Fuera de alcance, como en la spec
pausada: la Access Policy y la resolución de secretos por el canal autenticado (cambio siguiente), la consulta del estado
por los componentes, las identidades hijas, wrap/unwrap, la firma de checkpoints de auditoría, el desbloqueo desde
Ténaro, el adapter de hardware, los avisos y el lado de cada componente, que se prueba con un componente de prueba;
Orthros no cambia. La PR que la fusione retira su fila de «Cambios pausados». Rama feature/003-hecate-identities,
recreada desde main; commit de squash feat:"

## Clarifications

Las aclaraciones de la sesión del 2026-11-08 vienen de la spec pausada y siguen vigentes tras los cambios 004 y 005; las
de la sesión del 2026-11-09 las completan.

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

### Session 2026-11-09

- Q: ¿En qué proceso debe existir la clave de la CA raíz durante las ceremonias (crear la CA, renovar la intermedia y
  sustituir la CA)? → A: Solo en la CLI de la ceremonia: genera la CA raíz o descifra su copia con la frase de la CA,
  firma la intermedia que prepara Hécate, entrega la clave protegida con la frase y la borra. El proceso de Hécate nunca
  recibe la clave; la frase de la CA solo le llega al crear o sustituir la CA, para comprobar que no es la de
  desbloqueo.
- Q: ¿Debe esta feature añadir trazas y métricas con OpenTelemetry a Hécate, ahora que tiene llamadas entre
  componentes? → A: No: se mantiene la excepción de las features 001 y 002 y del cambio 005 hasta que DECISIONS cierre
  el colector y el backend; el plan de esta feature la vuelve a justificar, ya solo con ese motivo.
- Q: ¿Una petición a la API entre componentes que no es JSON estándar debe dejar fila en el registro de accesos? → A:
  No, como en el canal local (FR-018 del cambio 005): se rechaza con `validation` sin fila y queda en los logs
  estructurados, sin el contenido de la petición; las peticiones bien formadas que se rechazan sí dejan fila.
- Q: ¿Un alta rechazada debe gastar el token de alta que se presentó con ella? → A: No: el token solo se gasta al
  emitir el certificado (FR-011); cualquier alta rechazada (formato, otra identidad, Hécate sellada) lo deja sin usar
  hasta que caduca o el admin lo anula.
- Q: ¿Debe la suite completa pasar también en un contenedor Linux antes de fusionar esta feature, como en el cambio
  005? → A: Sí: en macOS y en un contenedor Linux antes de fusionar, porque Hécate corre en Linux en el servidor de
  casa y la API añade sockets de red y TLS.
- Q: ¿Qué debe poder hacer el admin con la identidad y el certificado propios de Hécate (`hecate`)? → A: Solo revocar
  su certificado con urgencia: Hécate se emite al momento uno nuevo con clave nueva y el anterior entra en la lista de
  revocación. La identidad `hecate` no admite revocación pasiva ni deshabilitación.
- Q: ¿Debe la ceremonia exigir que el admin demuestre que guardó la copia protegida de la clave de la CA raíz antes de
  dar la CA por creada? → A: Sí: la CLI pide la copia guardada y comprueba que se abre con la frase de la CA antes de
  que Hécate active la CA; hasta entonces no se guarda nada. Igual al sustituir la CA.
- Q: ¿Deben dejar fila en el registro de accesos las consultas que los componentes hacen a la lista de revocación y a
  las claves publicadas cuando se atienden? → A: No: las consultas atendidas no dejan fila y sus rechazos sí; las
  consultas del admin siguen dejando fila, como en la feature 002.
- Q: ¿Debe Hécate frenar las altas cuando acumula muchos intentos fallidos seguidos? → A: Sí, con un contador propio,
  aparte del común: tras 10 fallos seguidos rechaza las altas con esperas de 1, 2, 4 y 8 minutos y después de 15;
  registra cada tanda frenada en lugar de cada intento, el estado indica que las altas están frenadas y durante la
  espera también se rechaza un token válido.
- Q: ¿Qué debe pasar con la clave de la intermedia anterior cuando se renueva la intermedia o se sustituye la CA? → A:
  Se destruye en cuanto deja de hacer falta: al renovar, como mucho cuando caduca el último certificado que firmó (3
  días); al sustituir la CA, al momento.

## User Scenarios & Testing *(mandatory)*

Cada componente de Cerbero necesita una identidad para hablar con los demás: un certificado de corta duración cuyo
único nombre es su identidad (`spiffe://<trust domain>/<componente>`), emitido por la autoridad de certificados propia
de Cerbero (la CA interna). La CA tiene dos niveles: una CA raíz que vive fuera de Hécate y solo se usa en ceremonias, y
una intermedia que Hécate custodia y con la que firma. Además del certificado, cada llamada entre componentes lleva un
token corto firmado por quien llama con una clave propia, distinta de la del certificado (constitución, principio V);
Hécate registra y publica esas claves. Esta feature cubre crear la CA, dar de alta componentes con un token de un solo
uso, renovar sus certificados, publicar sus claves de firma de llamadas, revocar, y renovar o sustituir la CA.

La feature se reinicia sobre la Hécate que deja el cambio 005, con el comportamiento de las features 001 y 002.
Conserva el alcance y las decisiones de la spec pausada del 2026-11-08 y añade lo que traen los cambios 004 y 005: los
secretos de paso nuevos se borran de la memoria al dejar de usarse (FR-028), las entradas nuevas siguen su formato
estándar (FR-029) y lo que ya hace Hécate no cambia, salvo el rechazo de FR-027 (FR-030).

En esta spec, «token» a secas es el token de alta; el que acompaña a cada llamada es siempre el «token corto». «CA
raíz» es la autoridad raíz de la CA de Cerbero, y «raíz de cifrado», la clave raíz de Hécate de la feature 001. Un
«secreto de paso» es, como en el cambio 005, un secreto que Hécate solo tiene en memoria mientras lo usa.

Actores:

- **Admin**: el único usuario privilegiado; crea, renueva y sustituye la CA, genera los tokens de alta, revoca
  certificados y deshabilita identidades.
- **Componente**: una pieza de Cerbero (p. ej. Hermes o Hades) que necesita identidad. Genera sus claves privadas en su
  propio equipo, se da de alta con un token y renueva su certificado antes de que caduque.

Todas las operaciones exigen Hécate desbloqueada (feature 001), salvo los recuentos del estado.

### User Story 1 - Crear la CA de Cerbero (Priority: P1)

El admin crea la CA en una ceremonia, en el equipo de Hécate, y elige el trust domain del entorno (p. ej.
`cerbero.home`) y una frase propia de la CA. La CLI de la ceremonia genera la CA raíz, que firma la intermedia que
prepara Hécate. La clave de la CA raíz solo existe en la CLI mientras dura la ceremonia, nunca en el proceso de Hécate,
y sale de ella protegida con esa frase, para que el admin la guarde fuera de Hécate en dos soportes distintos. La CA
no queda creada hasta que el admin devuelve la copia guardada y la CLI comprueba que se abre con la frase; al terminar,
ni esa clave ni la frase quedan en memoria. Hécate se queda con el certificado de la CA raíz y con la intermedia, cuya
clave protege su raíz de cifrado.

**Why this priority**: sin CA no se puede emitir ninguna identidad; es la base del resto.

**Independent Test**: hacer la ceremonia y comprobar que la intermedia está firmada por la CA raíz, que la clave de la
CA raíz solo sale protegida con la frase de la CA, que sin devolver la copia guardada no se crea la CA, que el proceso
de Hécate nunca recibe esa clave ni la guarda, que ni ella ni la frase quedan en memoria al terminar y que la clave de
la intermedia nunca está en claro en disco.

**Acceptance Scenarios**:

1. **Given** Hécate desbloqueada y sin CA, **When** el admin hace la ceremonia con un trust domain válido y una frase de
   la CA válida, **Then** quedan creadas la CA raíz y la intermedia, Hécate se identifica desde ese momento con un
   certificado de su identidad emitido por la intermedia, y el admin recibe la clave de la CA raíz protegida con esa
   frase y la huella de la CA raíz.
2. **Given** una frase de la CA de menos de 12 caracteres o igual a la frase de desbloqueo, **When** el admin hace la
   ceremonia, **Then** se rechaza sin crear nada.
3. **Given** la ceremonia terminada, **When** se inspecciona lo que Hécate guarda y registra, **Then** no aparecen la
   clave de la CA raíz ni la frase de la CA, y la clave de la intermedia solo aparece cifrada.
4. **Given** una ceremonia interrumpida antes de terminar (p. ej. por un corte de luz), **When** Hécate vuelve,
   **Then** no queda nada guardado y la ceremonia se puede repetir.
5. **Given** una CA ya creada, **When** el admin intenta crearla otra vez, **Then** se rechaza: la CA solo cambia
   renovando la intermedia (User Story 6) o sustituyéndola (User Story 7).
6. **Given** Hécate sellada, **When** se intenta la ceremonia, **Then** se rechaza indicando que está sellada.
7. **Given** una ceremonia que termina, con éxito o con error, **When** se revisa lo que han recibido y conservan la
   CLI y el proceso de Hécate, **Then** la clave de la CA raíz nunca llegó al proceso de Hécate y ninguno de los dos
   conserva esa clave ni la frase de la CA (FR-002, FR-028).
8. **Given** una ceremonia en la que el admin no devuelve la copia guardada, o la devuelve y no se abre con la frase de
   la CA, **When** la CLI la comprueba, **Then** la CA no se crea, no queda nada guardado y la ceremonia se puede
   repetir.

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
3. **Given** un token para `hermes`, **When** la solicitud pide otra identidad, **Then** se rechaza sin emitir nada y el
   token sigue sin usar.
4. **Given** un token recién generado, **When** el admin lo recibe, **Then** es la única vez que se muestra: Hécate no
   puede volver a mostrarlo y su valor no queda en su memoria.
5. **Given** un componente con su token, **When** comprueba a Hécate antes de enviarlo, **Then** la identidad que
   presenta Hécate encadena con la CA raíz cuya huella lleva el token.
6. **Given** una identidad deshabilitada, **When** el admin pide un token para ella, **Then** se rechaza.
7. **Given** tokens pendientes, **When** el admin los lista, **Then** ve la identidad y la caducidad de cada uno, sin
   su valor, y puede anular cualquiera; un token anulado ya no sirve.
8. **Given** una identidad que ya tiene certificados, **When** se da de alta otra vez con un token nuevo, **Then**
   recibe un certificado nuevo y los anteriores ya no se renuevan.
9. **Given** un token válido, **When** el componente envía una solicitud o una clave pública que no sigue su formato
   estándar (p. ej. un PEM con dos bloques), **Then** se rechaza con `validation` sin emitir nada y el token sigue sin
   usar.
10. **Given** 10 altas fallidas seguidas, **When** llega otra, aunque traiga un token válido, **Then** se rechaza
    mientras dura la espera (1, 2, 4 y 8 minutos y después 15), queda una fila por tanda y no por intento, y el estado
    indica que las altas están frenadas; el desbloqueo y las operaciones del admin no se ven afectados.

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
6. **Given** un componente que refresca las claves publicadas o la lista de revocación, **When** Hécate atiende la
   consulta, **Then** no deja fila en el registro de accesos; si la rechaza, sí.

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
7. **Given** el certificado vigente de Hécate, **When** el admin lo revoca con urgencia, **Then** Hécate se identifica
   al momento con un certificado nuevo y otra clave, y el anterior entra en la lista de revocación; revocarlo de forma
   pasiva o deshabilitar la identidad `hecate` se rechaza.

---

### User Story 6 - Renovar la CA intermedia (Priority: P3)

Antes de que caduque la intermedia, el admin hace otra ceremonia con una copia de la clave de la CA raíz y la frase de
la CA: la CLI descifra la copia y firma con ella una intermedia nueva que prepara Hécate, con la que Hécate emite desde
ese momento. Ni la copia ni la frase llegan al proceso de Hécate. Los certificados emitidos con la anterior siguen
valiendo hasta caducar.

**Why this priority**: pasa una vez al año, pero si se olvida, Hécate deja de poder emitir; el estado avisa con
antelación.

**Independent Test**: renovar la intermedia y comprobar que las altas y renovaciones nuevas usan la nueva, que los
certificados emitidos con la anterior siguen siendo válidos hasta caducar, que el estado avisaba de la caducidad y que
la copia de la clave de la CA raíz y la frase de la CA no llegan al proceso de Hécate ni quedan en memoria al terminar.

**Acceptance Scenarios**:

1. **Given** una CA creada, una copia de la clave de la CA raíz y la frase de la CA, **When** el admin renueva la
   intermedia, **Then** las emisiones siguientes usan la intermedia nueva y la anterior deja de emitir.
2. **Given** certificados emitidos con la intermedia anterior, **When** se renueva la intermedia, **Then** siguen
   siendo válidos hasta caducar y se pueden renovar, obteniendo uno de la intermedia nueva.
3. **Given** una clave de CA raíz que no corresponde a la CA de Hécate, o una frase de la CA incorrecta, **When** se
   intenta renovar la intermedia, **Then** se rechaza sin cambiar nada.
4. **Given** una intermedia que caduca en menos de 30 días, **When** se consulta el estado de Hécate, **Then** lo
   indica, también con Hécate sellada.
5. **Given** una intermedia renovada, **When** caduca el último certificado que firmó la anterior, **Then** la clave de
   la anterior ya no existe en Hécate.

---

### User Story 7 - Sustituir la CA (Priority: P3)

Si la intermedia se compromete, o se pierden la clave de la CA raíz o la frase de la CA, el admin sustituye la CA entera
en una nueva ceremonia: CA raíz e intermedia nuevas, con una frase de la CA nueva. Como al crearla, la CA nueva no se
activa hasta confirmar la copia de su clave. Desde ese momento Hécate solo acepta la CA nueva y cada componente vuelve a
darse de alta con un token nuevo.

**Why this priority**: es un caso raro y caro (repetir todas las altas), pero sin él una intermedia comprometida
permitiría emitir identidades falsas hasta que caducase, y una CA raíz perdida dejaría a Hécate sin poder renovar la
intermedia.

**Independent Test**: sustituir la CA con componentes dados de alta y comprobar que sus certificados ya no se
renuevan, que los tokens pendientes ya no sirven, que un alta nueva obtiene un certificado de la CA nueva y que sin
la prueba no se sustituye nada.

**Acceptance Scenarios**:

1. **Given** una CA con componentes dados de alta, **When** el admin la sustituye demostrándolo con la frase actual de
   Hécate o con dos partes y una frase de la CA nueva, **Then** quedan una CA raíz y una intermedia nuevas, el admin
   recibe la clave de la CA raíz nueva protegida y su huella, Hécate se identifica con un certificado de la CA nueva y
   la clave de la intermedia anterior queda destruida.
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
- Hécate sin CA: la API entre componentes no responde, porque Hécate aún no tiene certificado con el que identificarse;
  solo funciona el canal local.
- Se roban el disco de Hécate: el admin revoca con urgencia el certificado de Hécate, cuya serie muestra el estado
  (User Story 5, escenario 7); en 5 minutos como mucho, ningún componente que refresque lo publicado acepta ya el
  robado.
- Una avalancha de altas con tokens inventados: tras 10 fallos seguidos, las altas se frenan (FR-034); el registro
  guarda una fila por tanda y el contador común de intentos no cambia.
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
- El admin repite una orden sobre algo que ya está en ese estado (deshabilitar una identidad deshabilitada, habilitar
  una habilitada, revocar un certificado ya revocado): se acepta sin cambiar nada. Pedir urgencia sobre una revocación
  o una deshabilitación pasivas sí las hace urgentes. Una identidad que solo tiene tokens no existe para las órdenes de
  identidades; sus tokens se listan y se anulan como tokens.
- Una solicitud de certificado, un certificado o una clave pública que no sigue su formato estándar (varios bloques
  PEM, espacios fuera de RFC 7468, un bloque vacío o de otro tipo), o una petición que no es JSON estándar: se rechaza
  con `validation` sin emitir ni cambiar nada, y un token de alta presentado con ella sigue sin usar. La que no es JSON
  estándar no deja fila en el registro de accesos; las demás, sí.
- Una operación que falla a medias (una ceremonia interrumpida, una renovación de la intermedia con una frase de la CA
  incorrecta, un alta rechazada): los secretos de paso que usó se borran igualmente de la memoria.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: El admin DEBE poder crear la CA en una ceremonia: una CA raíz que firma una intermedia para Hécate, dentro
  del trust domain que elige. Hécate tiene una sola CA.
- **FR-002**: La ceremonia DEBE hacerse en el equipo de Hécate, con su CLI y por su canal local, y la clave de la CA
  raíz NO DEBE entrar en el proceso de Hécate ni guardarse en Hécate: solo existe en la memoria de la CLI durante la
  ceremonia, que la genera, firma con ella la intermedia que prepara Hécate y la entrega protegida con una frase propia
  de la CA, de al menos 12 caracteres y distinta de la de desbloqueo, para guardarla en dos soportes distintos. La frase
  de la CA solo llega al proceso de Hécate al crear o sustituir la CA, para comprobar que no es la de desbloqueo. La CA
  no se crea hasta que el admin devuelve la copia guardada y la CLI comprueba que se abre con la frase de la CA. Hécate
  guarda el certificado de la CA raíz y su huella, nunca la frase de la CA.
- **FR-003**: La clave de la intermedia DEBE guardarse solo cifrada bajo la raíz de cifrado de Hécate, y solo se usa
  con Hécate desbloqueada.
- **FR-004**: Una ceremonia interrumpida, o sin la copia confirmada (FR-002), NO DEBE dejar nada guardado ni cambiar la
  CA vigente.
- **FR-005**: El admin DEBE poder renovar la intermedia con una copia de la clave de la CA raíz y la frase de la CA, que
  solo usa la CLI, como en FR-002: la intermedia nueva emite desde ese momento y los certificados de la anterior siguen
  valiendo hasta caducar. La clave de la intermedia anterior DEBE destruirse como mucho cuando caduca el último
  certificado que firmó.
- **FR-006**: El admin DEBE poder sustituir la CA entera (CA raíz e intermedia nuevas, con una frase de la CA nueva, en
  una ceremonia como la de FR-002) ante una intermedia comprometida o la pérdida de la clave de la CA raíz o de su
  frase, demostrándolo con la frase actual de Hécate o con dos partes (un fallo cuenta en el contador común de
  intentos). Desde ese momento Hécate solo acepta certificados de la CA nueva, anula los tokens pendientes, deja sin
  renovación todos los certificados anteriores y destruye la clave de la intermedia anterior; cada componente vuelve a
  darse de alta con un token nuevo.
- **FR-007**: El admin DEBE poder generar un token de alta para una identidad: de un solo uso, que caduca a los 15
  minutos, ligado a esa identidad y con la huella de la CA raíz. Se muestra una sola vez y Hécate guarda solo una forma
  que no permite recuperarlo. El admin DEBE poder listar los tokens pendientes, sin su valor, y anularlos. Los nombres
  `hecate` (la identidad de Hécate) y `admin` (quien figura en el registro de accesos por las operaciones del admin)
  están reservados: no se generan tokens para ellos.
- **FR-008**: Un componente DEBE poder obtener su primer certificado presentando un token válido, una solicitud
  firmada con la clave de su certificado y la clave pública con la que firmará sus llamadas, distinta de la del
  certificado. Hécate NO DEBE recibir nunca una clave privada de un componente.
- **FR-009**: Cada certificado DEBE llevar como único nombre la identidad `spiffe://<trust domain>/<identidad>` y servir
  al componente para identificarse tanto al llamar como al recibir llamadas. Dura 3 días y nunca más que la intermedia
  que lo firma.
- **FR-010**: La identidad del certificado DEBE decidirla Hécate: la del token en el alta y la del certificado
  presentado en la renovación. Una solicitud que pida otra identidad se rechaza.
- **FR-011**: Un token de alta DEBE quedar usado al emitir el certificado, de forma atómica: dos altas con el mismo
  token nunca obtienen dos certificados. Solo la emisión lo gasta: un alta rechazada por cualquier motivo lo deja sin
  usar hasta que caduca o el admin lo anula.
- **FR-012**: Hécate DEBE identificarse ante los componentes con un certificado de su propia identidad
  (`spiffe://<trust domain>/hecate`) emitido por la CA vigente, de modo que un componente pueda comprobarla con la
  huella de la CA raíz antes de enviar su token. Lo renueva ella sola mientras está desbloqueada, y al desbloquearla si
  caducó estando sellada; si el admin lo revoca con urgencia, DEBE emitirse al momento uno nuevo con clave nueva. La
  clave de ese certificado, y solo esa, DEBE estar disponible sin desbloquear, para que Hécate pueda responder sellada.
- **FR-013**: Una nueva alta de una identidad DEBE dejar sin renovación los certificados anteriores de esa identidad.
- **FR-014**: Un componente DEBE poder renovar su certificado mientras sea válido, aportando una clave de certificado y
  una clave de firma de llamadas nuevas. La renovación DEBE rechazarse si el certificado caducó, está revocado, no lo
  emitió Hécate con la CA raíz vigente, su identidad está deshabilitada o alguna de las claves nuevas repite la actual.
- **FR-015**: Un certificado DEBE renovarse una sola vez. Un segundo intento con el mismo certificado se rechaza, se
  registra como posible robo de su clave y revoca con urgencia todos los certificados válidos de esa identidad,
  incluido el recién emitido; la identidad solo vuelve a tener certificado dándose de alta con un token nuevo. Repetir
  la misma petición de alta (con el mismo token de alta) o de renovación (con un token corto nuevo) DEBE devolver el
  mismo resultado sin efectos nuevos y no cuenta como segundo intento.
- **FR-016**: La renovación, la lista de revocación y las claves publicadas DEBEN exigir, además de un certificado
  válido del componente, un token corto firmado con su clave de firma de llamadas registrada, cuyo emisor es la
  identidad del certificado de la conexión, que caduca en minutos y que no se acepta dos veces.
- **FR-017**: Hécate DEBE publicar para los componentes las claves de firma de llamadas registradas, cada una con su
  identidad: vale para verificar llamadas mientras su certificado es válido y la identidad está habilitada, y después
  solo para verificar firmas pasadas. La clave de un certificado revocado con urgencia o de una identidad deshabilitada
  deja de valer para llamadas nuevas en Hécate en ese momento. Lo publicado DEBE indicar que vale como mucho 5 minutos,
  de modo que todo Cerbero deje de aceptar esa clave en ese plazo.
- **FR-018**: El admin DEBE poder revocar un certificado: por defecto de forma pasiva (ya no se renueva, pero sigue
  siendo válido hasta caducar, igual que su clave de firma de llamadas); con urgencia, deja de ser válido en ese momento
  y entra en la lista de revocación hasta que caduca. El certificado de Hécate solo admite la revocación urgente
  (FR-012).
- **FR-019**: El admin DEBE poder deshabilitar una identidad (sus certificados ya no se renuevan, sus claves de firma de
  llamadas dejan de valer para llamadas nuevas, sus tokens pendientes quedan anulados, no se generan tokens para ella
  y, con urgencia, sus certificados válidos entran en la lista de revocación) y volver a habilitarla; una identidad
  habilitada de nuevo solo vuelve a tener certificado dándose de alta con un token nuevo. La identidad `hecate` no se
  puede deshabilitar.
- **FR-020**: Hécate DEBE ofrecer a los componentes la lista de los certificados revocados con urgencia que aún no han
  caducado, firmada por la CA. La lista DEBE indicar que vale como mucho 5 minutos, de modo que un certificado revocado
  con urgencia deje de aceptarse en todo Cerbero en ese plazo.
- **FR-021**: Hécate DEBE rechazar en cualquier operación los certificados de otro trust domain o que no encadenen con
  la CA raíz vigente.
- **FR-022**: El admin DEBE poder listar las identidades con su estado y la caducidad de su certificado más reciente, y
  consultar cada una con sus certificados (número de serie, validez y estado), sin claves privadas ni tokens.
- **FR-023**: Todas las operaciones de esta feature DEBEN exigir Hécate desbloqueada, salvo los recuentos del estado
  (FR-025). Sellada, Hécate DEBE responder a los componentes, con su certificado vigente, que está sellada, sin emitir,
  renovar ni ofrecer la lista de revocación ni las claves publicadas.
- **FR-024**: Hécate DEBE registrar en el registro de accesos cada ceremonia, renovación o sustitución de la CA, token
  de alta generado o anulado, alta, renovación, revocación, cambio de estado de una identidad, consulta del admin y
  rechazo, con quién (admin, identidad o la propia Hécate, como en la revocación automática de FR-015 o la renovación
  de su propio certificado), qué identidad y qué certificado. Las consultas atendidas de la lista de revocación y de
  las claves publicadas no dejan fila, y sus rechazos sí; tampoco la deja una petición que no es JSON estándar
  (FR-029). Las altas rechazadas mientras están frenadas dejan una fila por tanda (FR-034). Nunca un token, una clave
  privada ni la frase de la CA.
- **FR-025**: El estado de Hécate DEBE exponer, también sellada y sin nombres de identidades: si la CA existe, la serie
  y la caducidad del certificado vigente de Hécate (para poder revocarlo, FR-012), cuántas identidades habilitadas
  tienen su certificado más reciente a menos de 24 horas de caducar, cuántos intentos de renovación sospechosos
  (FR-015) hubo en las últimas 24 horas, los días que le quedan a la intermedia cuando son menos de 30 y si las altas
  están frenadas (FR-034).
- **FR-026**: Cada cambio de esta feature DEBE aplicarse de forma atómica: un corte deja el estado anterior o el nuevo.
  Los cambios del admin sobre identidades, certificados y tokens DEBEN aplicarse sin exigir la versión esperada: son
  órdenes cuyo resultado no depende de lo que el admin leyó antes, y las identidades cambian solas al renovar.
- **FR-027**: Guardar un secreto o una versión nueva (feature 002) en una ruta cuya identidad está reservada (`hecate`,
  `admin`) DEBE rechazarse. Para el resto de identidades se pueden seguir preparando secretos antes de su alta.
- **FR-028**: Los secretos de paso de esta feature DEBEN borrarse de la memoria al terminar la operación que los usa,
  también si falla, como fija el FR-007 del cambio 005: la frase de la CA (en la CLI y, al crear o sustituir la CA, en
  el proceso de Hécate), la clave de la CA raíz (solo en la CLI) y el valor de cada token de alta. La clave de la
  intermedia DEBE estar en memoria solo mientras Hécate está desbloqueada, como la raíz de cifrado, y la del
  certificado propio de Hécate solo mientras ese certificado está vigente; al sustituirse, o al parar Hécate, se
  borran. Se comprueba como en el cambio 005: por diseño, revisión y pruebas.
- **FR-029**: Las entradas nuevas DEBEN seguir su formato estándar, como fija el FR-018 del cambio 005; lo demás se
  rechaza con `validation`, sin efectos:
  - las solicitudes de certificado, los certificados y las claves públicas van en PEM según RFC 7468, con un solo
    bloque, del tipo que corresponde y con un contenido válido para su estándar;
  - las peticiones de la API entre componentes son JSON estándar, como las del canal local, y las que no lo son se
    rechazan sin fila en el registro de accesos, como fija el FR-018 del cambio 005; quedan en los logs estructurados,
    sin el contenido de la petición;
  - el trust domain y los nombres de identidad siguen sus reglas (ver Assumptions), y las fechas, si las hay, van en
    ISO 8601 extendido.
- **FR-030**: Las features 001 y 002 DEBEN seguir cumpliéndose tal como las deja el cambio 005, con sus pruebas, salvo
  el rechazo que FR-027 añade a propósito. Las pruebas que fijan el esquema exacto de la base o la respuesta exacta del
  estado se amplían con lo que añade esta feature, sin dejar de comprobar lo que ya comprobaban. Este cambio NO DEBE
  modificar Orthros.
- **FR-031**: DEBEN pasar las comprobaciones de calidad del ADR 0006 (formato, análisis estático con los avisos como
  errores y revisión de dependencias), con la política de licencias del ADR 0008, y la suite completa DEBE pasar en
  macOS y en un contenedor Linux antes de fusionar, como en el cambio 005, porque Hécate corre en Linux en casa.
- **FR-032**: Al cerrar, los docs DEBEN describir lo que implementa esta feature donde hoy lo contradicen o no lo
  recogen:
  - docs/hecate.md: las claves publicadas solo para componentes autenticados y válidas 5 minutos (los apartados 2 y 5
    las dan como públicas); el alta en la tabla de contratos y sin certificado de cliente (el apartado 5 dice «siempre
    por mTLS»); la CA creada en su propia ceremonia (el diagrama de arranque la pone en la inicialización y el texto, en
    el alta de identidades), en el equipo de Hécate, con la clave de la CA raíz solo en la CLI, protegida con su frase
    y guardada fuera;
    sustituir la CA ante una intermedia comprometida (apartado 10); el canal de resolución, que llega con la Access
    Policy y no con las identidades (apartado 4); la versión esperada solo en los secretos (apartado 3); las operaciones
    de identidades en el canal local y una CLI que no solo desbloquea (apartados 4 y 9); el alta con el registro de la
    clave de firma de llamadas; la respuesta de Hécate sellada y su riesgo aceptado (apartados 4 y 12); el
    almacenamiento nuevo, con los streams y las tablas de esta feature (apartados 3 y 8), y las librerías que elija el
    plan (apartados 1 y 7);
  - ARCHITECTURE (Identidad y seguridad): la renovación de la intermedia, que deja de ser una ceremonia offline; el
    token de alta, también para volver a dar de alta una identidad; y las claves de una identidad deshabilitada, que se
    conservan solo para verificar firmas pasadas en lugar de retirarse;
  - GLOSSARY: el token de alta, que no sirve solo para el primer certificado, los términos nuevos (revocación urgente,
    lista de revocación y clave de firma de llamadas) y el código común `unavailable` en Envelope;
  - docs/hermes.md: el código común `unavailable` en su lista de códigos;
  - DECISIONS: las desviaciones de esta feature frente a step-ca y SPIFFE (entre ellas, la renovación única con
    detección de reutilización y las claves publicadas solo para componentes autenticados) y el riesgo aceptado de la
    clave del certificado de Hécate.
- **FR-033**: La PR que fusione este cambio DEBE retirar su fila de «Cambios pausados» en DECISIONS (constitución, «Una
  cosa a la vez»).
- **FR-034**: Hécate DEBE frenar las altas tras 10 fallos seguidos, con un contador propio, distinto del contador común
  de intentos y que sobrevive a los reinicios: las rechaza con esperas de 1, 2, 4 y 8 minutos y después de 15, también
  si traen un token válido, y registra cada tanda frenada en lugar de cada intento. Cuenta como fallo toda petición de
  alta rechazada, salvo por estar Hécate sellada o por el propio frenado, y un alta emitida pone el contador a cero.

### Key Entities *(include if feature involves data)*

- **CA raíz**: la autoridad de confianza de un trust domain. Su certificado y su huella los conocen Hécate y los
  componentes; su clave vive fuera de Hécate, protegida con la frase de la CA, y solo la usa la CLI en las ceremonias.
  Al sustituir la CA, una CA raíz nueva reemplaza a la anterior.
- **CA intermedia**: firmada por la CA raíz y custodiada por Hécate, que firma con ella. Dura 1 año; al renovarla, la
  anterior deja de emitir y su clave se destruye como mucho cuando caduca el último certificado que firmó (al sustituir
  la CA, al momento).
- **Identidad**: un componente dentro de un trust domain (`spiffe://<trust domain>/<nombre>`), con estado `enrolled`
  (dada de alta y habilitada) o `disabled` (deshabilitada). Nace con su primera alta; habilitarla de nuevo la devuelve a
  `enrolled`, pero necesita un token nuevo para volver a tener certificado. Los nombres `hecate` y `admin` están
  reservados.
- **Token de alta**: de un solo uso, ligado a una identidad, con caducidad y estado (pendiente, usado, anulado o
  caducado); solo pasa a usado al emitir un certificado. Hécate no guarda su valor.
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
  emitir nada y quedan registrados (por tandas mientras las altas están frenadas), salvo la repetición exacta de la
  petición con la que se usó, que devuelve el mismo certificado.
- **SC-003**: Ninguna inspección de lo que Hécate guarda o registra encuentra la clave de la CA raíz, la frase de la CA,
  la clave de la intermedia sin cifrar, un token de alta en claro ni una clave privada de un componente, y el proceso de
  Hécate no recibe la clave de la CA raíz en ninguna ceremonia.
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
- **SC-012**: Al terminar cualquier operación de esta feature, también si falla, ninguno de sus secretos de paso
  (FR-028) queda en la memoria que controla Hécate, y con Hécate sellada la clave de la intermedia no está en memoria.
- **SC-013**: El 100 % de las entradas que no siguen su formato estándar se rechazan con `validation` sin emitir,
  cambiar ni gastar nada.
- **SC-014**: La suite completa pasa en macOS y en un contenedor Linux, incluidas todas las pruebas de las features 001
  y 002 con las salvedades de FR-030, y las comprobaciones de calidad pasan sin avisos.
- **SC-015**: Tras revocar con urgencia el certificado de Hécate, Hécate se identifica al momento con uno nuevo, y en 5
  minutos como mucho ningún componente que refresque lo que publica Hécate acepta ya el anterior.
- **SC-016**: Ninguna CA se crea ni se sustituye sin que el admin haya devuelto la copia de la clave de su CA raíz y
  esta se haya abierto con su frase.
- **SC-017**: Una avalancha de altas fallidas, una vez frenada, deja como mucho una fila por tanda en el registro de
  accesos y no bloquea el desbloqueo ni las operaciones del admin.
- **SC-018**: Ninguna clave de una intermedia anterior sigue en Hécate más de 3 días después de renovar la intermedia,
  ni después de sustituir la CA.

## Assumptions

- **Base**: la Hécate que deja el cambio 005, con el comportamiento de las features 001 y 002, su canal local y su CLI.
  Esta feature la amplía; lo único que cambia de lo que ya hace es FR-027.
- **Trust domain**: uno por Hécate, elegido al crear la CA (`cerbero.home` en casa, `cerbero.test` en pruebas);
  sustituir la CA mantiene el trust domain. Su nombre sigue la regla de SPIFFE: minúsculas, dígitos, puntos, guiones y
  guiones bajos.
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
  llevará para otras operaciones se evalúan con la Access Policy (cambio siguiente).
- **El alta no lleva token corto ni certificado de cliente**, porque el componente aún no tiene ninguno: la autentica
  el token de alta de un solo uso. Es la excepción inherente al arranque frente al principio V de la constitución, y
  el plan la justificará.
- **Frase de la CA**: la elige y la guarda el admin; Hécate no la guarda. Si se pierde, la salida es sustituir la CA
  (User Story 7). Un fallo con ella queda registrado, aunque lo detecte la CLI, pero no cuenta en el contador común de
  intentos: no protege la raíz de cifrado de Hécate, sino una copia que el admin guarda fuera.
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
- **Lo que decide el plan**: el stack HTTP y TLS de la API entre componentes y la PKI, que los ADR 0005 y 0007 dejan a
  esta feature. Si hace falta tecnología nueva, se decide en un ADR antes de usarla (constitución, «Nueva tecnología»).
- **Observabilidad**: sin OpenTelemetry por ahora. La excepción de las features 001 y 002 y del cambio 005 sigue hasta
  que DECISIONS cierre el colector y el backend, y el plan la justifica de nuevo: las llamadas entre componentes ya no
  sirven de motivo. Lo que pasa queda en el registro de accesos (FR-024) y en los logs estructurados.
- **El admin opera desde el equipo de Hécate**, por el canal local; la gestión desde Ténaro llegará con Ténaro.
- **Riesgo aceptado**: la clave del certificado propio de Hécate no depende del desbloqueo, para que Hécate pueda
  responder sellada. Quien se lleve el disco podría hacerse pasar por Hécate hasta que el admin revoque con urgencia
  ese certificado (5 minutos como mucho tras revocarlo) o, si no lo revoca, mientras sea válido (como mucho 3 días); se
  registra en DECISIONS (FR-032). El adapter de hardware (fuera de alcance) permitiría protegerla sin
  desbloquear.
- **Resolución de secretos**: sigue siendo un caso de uso interno, sin canal, hasta el cambio de la Access Policy. El
  FR-016 del cambio 005 la dejaba así «hasta la feature 003», pero esta feature no la expone.
- **Fuera de alcance**: la Access Policy y la resolución de secretos por el canal autenticado (cambio siguiente), la
  consulta del estado por los componentes (ámbito `status`, con la Access Policy), las identidades hijas de las
  integraciones de Hades, `wrap`/`unwrap`, la firma de checkpoints de auditoría, el desbloqueo desde Ténaro, el adapter
  de hardware, los avisos (los emite un cronjob de Hades a partir del estado), las trazas y métricas con OpenTelemetry
  (ver Observabilidad) y Orthros.
- **Una sola instancia de Hécate**, sin alta disponibilidad; decenas de identidades.
- **Referencias de diseño**: [docs/hecate.md](../../docs/hecate.md) (alta y renovación de una identidad, operación),
  [ARCHITECTURE](../../ARCHITECTURE.md#6-identidad-y-seguridad) (identidad y seguridad) y las desviaciones de Hécate
  frente a step-ca y SPIFFE en [DECISIONS](../../DECISIONS.md).
