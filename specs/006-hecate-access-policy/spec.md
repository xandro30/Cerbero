# Feature Specification: Access Policy de Hécate y resolución de secretos

**Feature Branch**: `feature/006-hecate-access-policy`

**Created**: 2026-11-10

**Status**: Draft

**Input**: User description: "Añadir a Hécate la Access Policy y la resolución de secretos por el canal autenticado, la
decisión abierta en DECISIONS que la feature 003 dejó para el cambio siguiente. Hoy la resolución (feature 002) es un
caso de uso interno sin canal; este cambio la expone en la API mTLS de la 003 (POST /v1/secrets:resolve, como fija
docs/hecate.md): un componente autenticado con su certificado y su token corto resuelve varias referencias secret:// a
la vez, la versión activa o una concreta (@N), y recibe valor, versión y TTL de caché, con las reglas que ya tiene el
caso de uso: todo o nada y un único not_found igual tanto si el secreto no existe como si es de otra identidad o no
tiene versión activa. Ese rechazo no debe distinguir los casos ni por su contenido ni por su tiempo de respuesta, que la
002 dejó sin medir porque no había canal. La Access Policy decide qué rutas y operaciones usa cada identidad: por
defecto cada una solo lee secret://<su identidad>/** y lo demás está denegado; compartir un secreto con otra identidad
es una excepción explícita y auditada que el admin concede y retira por el canal local y la CLI; cada cambio de
política se versiona y queda registrado con quién. Entra también el ámbito status: una identidad con ese permiso
consulta el estado agregado (sellada o no y los recuentos de secretos por rotar o caducar y de certificados por
caducar), nunca rutas ni metadatos de un secreto concreto. Cada resolución, atendida o rechazada, deja fila en el
registro de accesos, sin el valor ni la huella. Se mantienen las garantías del 005 y de la 003: los valores resueltos
son secretos de paso que se borran de la memoria al enviar la respuesta, también si falla (FR-007 del 005, FR-028 de la
003), y las entradas siguen su formato estándar (FR-018 del 005, FR-029 de la 003); la API conserva el sobre común, sus
códigos y su comportamiento con Hécate sellada. Se cumplen las comprobaciones de calidad del ADR 0006 con la política de
licencias del ADR 0008, la suite completa pasa en macOS y en un contenedor Linux antes de fusionar, y la pila es la del
ADR 0009 (si hace falta tecnología nueva, se decide en un ADR antes de usarla). Al cerrar se consolidan docs/hecate.md,
el OpenAPI de shared/contracts y lo que corresponda de ARCHITECTURE y GLOSSARY, revisando la lista contra los docs
actuales, y la decisión abierta sale de «Abiertas» de DECISIONS y pasa a un ADR, como fija la constitución. Fuera de
alcance: la firma de checkpoints de auditoría y su clave de firma de eventos, wrap/unwrap, la rotación de las claves de
ámbito, las identidades hijas, los permisos que salen de las declaraciones de Hermes y Hades (llegan con esos
componentes), la gestión y el desbloqueo desde Ténaro, el adapter de hardware, los avisos, la rotación automática de
secretos, OpenTelemetry (sigue la excepción hasta que DECISIONS cierre el colector y el backend) y el lado de cada
componente (su caché y su refresco), que se prueba con el componente de prueba de la 003; Orthros no cambia. Rama
feature/006-hecate-access-policy, creada desde main; commit de squash feat:"

## Clarifications

### Session 2026-11-10

- Q: ¿La consulta del estado agregado por la API debe responder también con Hécate sellada, aunque el resto de la API
  responda que está sellada? → A: Sí: responde indicando que está sellada, con sus recuentos, que no tocan material
  cifrado, como el estado del canal local; el resto de la API sigue respondiendo que está sellada.
- Q: ¿Una excepción de lectura debe nombrar un secreto concreto que ya exista, o puede cubrir un prefijo de rutas de
  otra identidad? → A: Un secreto concreto que exista al concederla; compartir varios es conceder varias excepciones.
- Q: ¿Los cambios de política del admin deben llevar la versión esperada de la política, como los de secretos, o
  funcionar como órdenes al momento, como las de identidades de la feature 003? → A: Órdenes sin versión esperada:
  repetir una que no cambia nada no crea versión, y cada cambio es atómico y queda en el historial con quién lo hizo.
- Q: ¿Las consultas del estado agregado que Hécate atiende deben dejar fila en el registro de accesos, o solo sus
  rechazos? → A: Solo los rechazos, como la lista de revocación y las claves publicadas de la feature 003: el estado
  no lleva rutas ni nombres, y un vigilante que lo consulta a menudo llenaría el registro.
- Q: ¿Cómo se sabe qué secretos deben rotarse, si hoy `rotate_every` es un texto libre que no se puede calcular? → A:
  `rotate_every` pasa a ser una duración ISO 8601 que se valida al guardar el secreto, y un secreto debe rotarse cuando
  su versión activa tiene ese tiempo o más. Es un cambio a propósito de la feature 002 (decidido en el plan).
- Q: ¿Se puede conceder una excepción o el permiso de estado a una identidad que aún no se ha dado de alta? → A: Sí: su
  política se puede preparar antes del alta, como sus secretos (decidido al revisar la checklist, como las tres
  siguientes).
- Q: ¿Qué filas deja una resolución rechazada? → A: Una sola, con la primera referencia que hace fallar la petición,
  como la resolución de la feature 002.
- Q: ¿En qué momento debe rotarse un secreto? → A: Cuando su versión activa cumple la duración: en el instante exacto
  ya cuenta.
- Q: ¿Se limita el tamaño de una respuesta de resolución? → A: No: bastan el máximo de 100 referencias y el de 64 KiB
  por valor.

## User Scenarios & Testing *(mandatory)*

Hécate guarda los secretos de terceros de Cerbero desde la feature 002 y da identidad a los componentes desde la 003,
pero ningún componente puede leer todavía sus secretos: la resolución existe solo dentro de Hécate. Esta feature la
abre a los componentes autenticados y añade la Access Policy, que decide qué puede usar cada identidad: por defecto,
solo sus propios secretos; además, los que el admin le comparta de forma explícita y, si se le concede, el estado
agregado de Hécate.

En esta spec, «resolver» es entregar a un componente el valor de uno o varios secretos; una «referencia» es
`secret://<identidad>/<resto>`, opcionalmente con una versión concreta (`@N`); una «versión resoluble» es la que FR-002
deja resolver; una «excepción» es el permiso que el admin concede a una identidad para leer un secreto concreto de
otra; y el «permiso de estado» deja consultar los recuentos agregados de Hécate.

Actores:

- **Admin**: el único usuario privilegiado; guarda los secretos (feature 002), concede y retira excepciones y el permiso
  de estado, y consulta las políticas, siempre desde el equipo de Hécate, por el canal local y la CLI.
- **Componente**: una pieza de Cerbero (p. ej. Hermes o Hades) dada de alta en Hécate (feature 003) que resuelve sus
  secretos al ejecutar y, si tiene el permiso, consulta el estado.

### User Story 1 - Resolver sus secretos (Priority: P1)

Un componente (p. ej. Hermes) necesita en ejecución los secretos que el admin guardó para él, como el token de su bot o
una API key. Se identifica ante Hécate con su certificado y un token corto, como en la feature 003, y pide en una sola
petición todas las referencias que necesita, cada una a la versión activa o a una concreta. Hécate comprueba que puede
leer cada una y le devuelve todos los valores o ninguno, con su versión y cuánto tiempo puede guardarlos en caché.

**Why this priority**: es para lo que existen los secretos; hasta ahora ningún componente podía leerlos.

**Independent Test**: guardar secretos para un componente de prueba dado de alta, resolverlos y comprobar los valores,
las versiones y los tiempos de caché; comprobar que una referencia ajena o inexistente rechaza la petición entera con la
misma respuesta y sin diferencia de tiempo, que cada resolución deja fila sin el valor y que el valor no queda en la
memoria de Hécate al terminar.

**Acceptance Scenarios**:

1. **Given** un componente dado de alta con secretos en `secret://<su identidad>/...`, **When** pide varias referencias
   sin versión con su certificado y un token corto válidos, **Then** recibe por cada una el valor de su versión activa,
   el número de versión y el tiempo de caché (5 minutos blando y 1 hora duro).
2. **Given** un secreto con una versión `deprecated` dentro de su periodo de gracia, **When** el componente la pide de
   forma explícita (`@N`), **Then** la recibe; sin indicar versión recibe la activa; pasada la gracia, o si la versión
   está `revoked` o `destroyed`, la petición se rechaza.
3. **Given** una petición con una referencia que no existe, una de otra identidad sin excepción o una sin versión
   resoluble, **When** se resuelve, **Then** se rechaza entera con el mismo `not_found`, sin devolver ningún valor y sin
   indicar el motivo.
4. **Given** muchas peticiones rechazadas de cada uno de esos tres tipos, **When** se miden sus tiempos de respuesta,
   **Then** no permiten distinguir de qué tipo es cada una.
5. **Given** una resolución atendida o rechazada, **When** se revisa el registro de accesos, **Then** tiene una fila por
   referencia resuelta, o una con la referencia que hizo fallar la petición, con la identidad y su certificado, sin el
   valor ni la huella.
6. **Given** Hécate sellada, **When** un componente intenta resolver, **Then** Hécate responde que está sellada y no
   devuelve nada.
7. **Given** una petición sin certificado válido o sin token corto válido, **When** se intenta resolver, **Then** se
   rechaza con `auth` y queda registrada, como cualquier otra operación de la API.
8. **Given** una resolución que termina, con éxito o con error, **When** se revisa la memoria que controla Hécate,
   **Then** los valores resueltos ya no están en ella.
9. **Given** una petición que no sigue su formato estándar (una referencia mal formada, una versión que no es un entero
   positivo de 1 a 9 cifras sin ceros a la izquierda, más de 100 referencias o un cuerpo que no es JSON estándar),
   **When** se envía, **Then** se rechaza con `validation` sin resolver nada.

---

### User Story 2 - Compartir un secreto con otra identidad (Priority: P2)

A veces un componente necesita un secreto que pertenece a otro, como una credencial común a dos integraciones. El admin
concede a esa identidad la lectura de ese secreto concreto, como una excepción explícita, y la retira cuando ya no hace
falta. Cada cambio crea una versión de la política de la identidad con quién lo hizo, y el admin puede consultar qué
puede leer cada identidad y desde cuándo.

**Why this priority**: sin excepciones, un secreto que necesitan dos componentes tendría que duplicarse, con dos copias
que rotar a la vez; va detrás de la resolución porque amplía quién puede leer.

**Independent Test**: conceder a un componente de prueba un secreto de otra identidad, resolverlo, retirar la excepción
y comprobar que la siguiente petición se rechaza, que cada cambio crea una versión de la política con quién y cuándo, y
que el historial muestra qué podía leer en cada momento.

**Acceptance Scenarios**:

1. **Given** un secreto de `hades` y la identidad `hermes` sin excepción, **When** `hermes` lo pide, **Then** se rechaza
   con el mismo `not_found` que un secreto inexistente.
2. **Given** que el admin concede a `hermes` la lectura de ese secreto, **When** `hermes` lo pide, **Then** lo recibe
   con las mismas reglas de versiones que su dueño, y los demás secretos de `hades` le siguen denegados.
3. **Given** una excepción concedida, **When** el admin la retira, **Then** la siguiente petición de `hermes` a ese
   secreto se rechaza.
4. **Given** cambios en la política de una identidad, **When** el admin la consulta, **Then** ve las excepciones
   vigentes y si tiene el permiso de estado, y en su historial cada versión con el cambio, quién lo hizo y cuándo.
5. **Given** una excepción para una identidad reservada (`hecate`, `admin`), para un secreto que no existe o para un
   secreto de la propia identidad, **When** el admin la pide, **Then** se rechaza sin cambiar nada.
6. **Given** una excepción que ya existe, **When** el admin la concede otra vez, o retira una que no existe, **Then** se
   acepta sin crear una versión nueva.
7. **Given** Hécate sellada, **When** el admin cambia o consulta una política, **Then** se rechaza indicando que está
   sellada.

---

### User Story 3 - Consultar el estado agregado (Priority: P3)

Un componente encargado de vigilar Cerbero (p. ej. un cronjob de Hades que emite avisos) necesita saber si Hécate está
sellada y cuántos secretos y certificados necesitan atención, sin ver ninguno en concreto. El admin le concede el
permiso de estado, y el componente consulta los recuentos, también con Hécate sellada.

**Why this priority**: es la base de los avisos de rotación y caducidad que emitirá Hades; va al final porque esos
avisos aún no existen.

**Independent Test**: conceder el permiso de estado a un componente de prueba, consultar el estado con Hécate
desbloqueada y sellada, comprobar los recuentos y que ninguna respuesta lleva rutas ni nombres, y que sin el permiso la
consulta se rechaza.

**Acceptance Scenarios**:

1. **Given** un componente con el permiso de estado, **When** consulta el estado, **Then** recibe si Hécate está sellada
   y cuántos secretos han caducado, caducan en 7 días o deben rotarse, cuántas identidades tienen su certificado más
   reciente a menos de 24 horas de caducar, cuántos intentos de renovación sospechosos hubo en 24 horas, los días que le
   quedan a la intermedia si son menos de 30 y si las altas están frenadas.
2. **Given** Hécate sellada, **When** ese componente consulta el estado, **Then** lo recibe igualmente, indicando que
   está sellada.
3. **Given** un componente sin el permiso de estado, **When** lo consulta, **Then** se rechaza y queda registrado.
4. **Given** cualquier respuesta del estado, **When** se inspecciona, **Then** no contiene rutas de secretos, nombres de
   identidades ni metadatos de un secreto concreto.
5. **Given** un secreto que indica cada cuánto rotarlo y cuya versión activa ya tiene ese tiempo o más, **When** se
   consulta el estado, por la API o por el canal local, **Then** cuenta entre los secretos que deben rotarse.
6. **Given** una consulta del estado atendida, **When** se revisa el registro de accesos, **Then** no ha dejado fila; si
   se rechazó, sí.

---

### Edge Cases

- Una petición mezcla secretos propios y compartidos: se resuelve todo o nada, como cualquier otra.
- La política de una identidad cambia mientras resuelve: la resolución usa la política de antes o la de después, nunca
  una mezcla.
- Se deshabilita la identidad dueña de un secreto compartido: la excepción sigue y la otra identidad lo sigue
  resolviendo, porque los secretos no dependen del alta de su dueño (feature 002).
- Se deshabilita la identidad que recibió una excepción, o se sustituye la CA: no puede autenticarse hasta darse de alta
  de nuevo, y su política se conserva.
- Un componente prueba rutas ajenas una tras otra: todas reciben el mismo `not_found`, sin diferencia de tiempo, y cada
  intento deja fila atribuida a su identidad; no hay freno, porque está autenticado.
- Se revoca o se destruye una versión de un secreto compartido: la otra identidad deja de resolverla igual que su dueño.
- Un componente guardó un valor en caché y después se retira la excepción o se revoca la versión: Hécate no puede borrar
  esa copia; deja de refrescarse y caduca como mucho al cumplirse el tiempo duro de caché (1 hora).
- Hécate sellada: la resolución responde que está sellada, el estado responde con sus recuentos y la gestión de
  políticas se rechaza.
- Se va la luz a mitad de un cambio de política: queda la política anterior o la nueva, nunca a medias.
- Un secreto pasa su fecha de caducidad: se sigue resolviendo y cuenta en el estado entre los caducados (feature 002).
- Un secreto guardado antes de esta feature con un `rotate_every` que no es una duración: se sigue leyendo y
  resolviendo, y no cuenta entre los que deben rotarse.
- Una referencia pide una versión que no existe: se rechaza con el mismo `not_found` que un secreto inexistente.
- Una referencia aparece dos veces en la misma petición: se resuelve otra vez, con el mismo resultado.
- Un secreto tiene su versión activa sin el material guardado (datos inconsistentes): se trata como sin versión
  resoluble, con `not_found`, como en la feature 002.
- Un secreto compartido se queda sin versiones resolubles (todas revocadas o destruidas): la excepción sigue en la
  política y la resolución da `not_found`, como a su dueño.
- Una petición que no es JSON estándar: se rechaza con `validation` sin fila en el registro de accesos y queda en los
  logs estructurados, como en la feature 003; el resto de peticiones mal formadas sí dejan fila.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Un componente autenticado con un certificado válido y un token corto válido, como en la feature 003,
  DEBE poder resolver en una sola petición una o varias referencias a secretos, cada una a la versión activa o a una
  versión concreta, y recibir por cada una el valor, el número de versión y el tiempo de caché (5 minutos de caché
  blanda y 1 hora dura, como fija el FR-011 de la feature 002).
- **FR-002**: La resolución DEBE seguir las reglas de versiones de la feature 002: sin indicar versión se resuelve la
  `active`; una `deprecated` solo se resuelve pedida de forma explícita y durante su periodo de gracia; una `revoked` o
  `destroyed` no se resuelve nunca; un secreto caducado se sigue resolviendo (FR-007, FR-014 y FR-022 de la 002).
- **FR-003**: Si alguna referencia de una petición no existe, no está permitida para la identidad que la pide o no tiene
  una versión resoluble, la petición entera DEBE rechazarse sin devolver ningún valor y con un único `not_found`, con el
  mismo contenido en los tres casos (FR-013 de la 002).
- **FR-004**: Los rechazos de FR-003 NO DEBEN poder distinguirse por su tiempo de respuesta, sea cual sea el número de
  referencias de la petición o la posición de la que falla: medirlo no debe revelar si un secreto existe, si es de otra
  identidad o si le falta una versión resoluble. Ningún dato de la respuesta DEBE delatarlo, tampoco la duración que
  indica el sobre. Quien mide es un componente autenticado, desde la red interna y con tantas peticiones como quiera.
- **FR-005**: Por defecto, una identidad DEBE poder resolver solo `secret://<su identidad>/**`; cualquier otra ruta está
  denegada salvo una excepción vigente de su Access Policy (FR-006). Esa regla por defecto no necesita ninguna política.
- **FR-006**: El admin DEBE poder conceder a una identidad la lectura de un secreto concreto de otra identidad, y
  retirarla, por el canal local y la CLI. La excepción nombra un secreto concreto, nunca un prefijo de rutas, y vale
  solo para ese secreto, con las reglas de versiones de FR-002; compartir varios es conceder varias. Se rechaza sin
  cambiar nada una excepción para una identidad reservada (`hecate`, `admin`) o para un secreto de la propia identidad
  (`validation`), y para un secreto que no existe (`not_found`). La identidad que la recibe no tiene que estar dada de
  alta: su política se puede preparar antes, como sus secretos (FR-027 de la feature 003).
- **FR-007**: El admin DEBE poder conceder a una identidad el permiso de estado (FR-011), y retirarlo, por el canal
  local y la CLI. Las identidades reservadas no pueden recibirlo, y la identidad tampoco tiene que estar dada de alta.
- **FR-008**: La Access Policy de cada identidad DEBE ser versionada: cada cambio crea una versión nueva, se aplica de
  forma atómica y queda registrado con quién lo hizo y cuándo. Conceder una excepción o un permiso que ya existen, o
  retirar uno que no existe, se acepta sin crear una versión. Los cambios no exigen la versión esperada: son órdenes
  cuyo resultado no depende de lo que el admin leyó antes, como las de identidades de la feature 003 (FR-026).
- **FR-009**: El admin DEBE poder listar las identidades cuya política ha cambiado alguna vez, también las que ya no
  tienen nada concedido, y consultar la política vigente de cada una con su historial: cada versión, con el cambio,
  quién lo hizo y cuándo. Una identidad sin política se consulta como una política vacía, en la versión 0, porque vale
  la regla por defecto.
- **FR-010**: Retirar una excepción o el permiso de estado DEBE tener efecto en Hécate desde ese momento: la siguiente
  petición de esa identidad que dependa de él se rechaza.
- **FR-011**: Una identidad con el permiso de estado DEBE poder consultar el estado agregado de Hécate: si está sellada;
  cuántos secretos han caducado, caducan en 7 días o deben rotarse; cuántas identidades habilitadas tienen su
  certificado más reciente a menos de 24 horas de caducar; cuántos intentos de renovación sospechosos hubo en las
  últimas 24 horas; los días que le quedan a la intermedia cuando son menos de 30; y si las altas están frenadas. La
  respuesta NO DEBE contener rutas de secretos, nombres de identidades ni metadatos de un secreto concreto. Sin el
  permiso, la consulta se rechaza con `auth`.
- **FR-012**: La consulta del estado DEBE responder también con Hécate sellada, con la misma autenticación que
  desbloqueada e indicando que lo está, como el estado del canal local: sus recuentos no tocan material cifrado. El
  resto de operaciones de la API siguen respondiendo que está sellada (FR-023 de la feature 003).
- **FR-013**: El estado del canal local DEBE contar también los secretos que deben rotarse: los que indican cada cuánto
  rotarlos con una duración (FR-023) y cuya versión activa ya tiene esa duración o más; un secreto sin versión activa
  no cuenta.
- **FR-014**: Cambiar o consultar políticas DEBE exigir Hécate desbloqueada, como el resto de la gestión del admin.
- **FR-015**: Hécate DEBE registrar en el registro de accesos cada resolución, porque entrega secretos: si se atiende,
  una fila por referencia resuelta, y si se rechaza, una fila con la primera referencia que hace fallar la petición,
  siempre con la identidad y su certificado. También cada cambio y cada consulta de políticas del admin, y cada consulta
  del estado rechazada. Las consultas del estado atendidas no dejan fila, como las de la lista de revocación y las
  claves publicadas de la feature 003, y una petición que no es JSON estándar tampoco (FR-029 de la 003). Nunca un
  valor, una huella, una clave privada ni un token.
- **FR-016**: Los valores resueltos son secretos de paso: DEBEN borrarse de la memoria al terminar de enviar la
  respuesta, también si la operación falla, como fijan el FR-007 del cambio 005 y el FR-028 de la feature 003. Ningún
  valor resuelto ni su huella DEBEN quedar en lo que Hécate guarda, en sus eventos ni en sus logs. Las copias que hacen
  las librerías al cifrar el TLS y el sistema operativo quedan en el riesgo residual ya aceptado en DECISIONS.
- **FR-017**: Las entradas nuevas DEBEN seguir su formato estándar, como fijan el FR-018 del cambio 005 y el FR-029 de
  la feature 003; lo demás se rechaza con `validation`, sin efectos: las referencias siguen la regla de rutas de la
  feature 002, con una versión opcional que es un entero positivo de 1 a 9 cifras, sin ceros a la izquierda, hasta 100
  referencias por petición, y las peticiones son JSON estándar.
- **FR-018**: Las operaciones nuevas de la API DEBEN seguir el contrato común entre componentes y las convenciones de la
  feature 003: el mismo sobre y los mismos códigos, la versión mayor en la ruta, su lista de operaciones, la
  autenticación por certificado y token corto, y una fila por cada rechazo, también los que ocurren antes de llegar a la
  operación, salvo la petición que no es JSON estándar (FR-015).
- **FR-019**: Cada cambio de esta feature DEBE aplicarse de forma atómica: un corte deja el estado anterior o el nuevo.
- **FR-020**: Las features 001, 002 y 003 DEBEN seguir cumpliéndose con sus pruebas, salvo el formato de `rotate_every`
  que FR-023 cambia a propósito. Las pruebas que fijan el esquema
  exacto de la base, la respuesta exacta del estado y la lista de operaciones de la API se amplían con lo que añade esta
  feature, sin dejar de comprobar lo que ya comprobaban. Este cambio NO DEBE modificar Orthros.
- **FR-021**: DEBEN pasar las comprobaciones de calidad del ADR 0006 (formato, análisis estático con los avisos como
  errores y revisión de dependencias), con la política de licencias del ADR 0008, y la suite completa DEBE pasar en
  macOS y en un contenedor Linux antes de fusionar.
- **FR-022**: Al cerrar, los docs DEBEN describir lo que implementa esta feature donde hoy lo contradicen o no lo
  recogen:
  - docs/hecate.md: la resolución por el canal autenticado, que hoy figura como caso de uso interno sin canal
    (apartado 4, también la ruta de su diagrama); la Access Policy con la regla por defecto, las excepciones del admin
    y el permiso de estado (apartados 3 y 9); las operaciones nuevas de la API y del canal local (apartados 4, 5 y 9);
    el estado para los componentes, también sellada (apartados 5 y 10); y `rotate_every` como duración (apartado 6);
  - GLOSSARY: la entrada Access Policy, que hoy solo menciona los permisos que salen de las declaraciones, con las
    excepciones del admin y el permiso de estado;
  - el OpenAPI de Hécate en `shared/contracts`, con las operaciones nuevas;
  - ARCHITECTURE, si algo de Identidad y seguridad lo contradice;
  - DECISIONS: la decisión «Access Policy de Hécate y resolución por el canal autenticado» sale de «Abiertas» y pasa a
    un ADR, como fija la constitución, y las desviaciones frente a OpenBao / Vault recogen las excepciones por secreto
    concreto, sin comodines (constitución, principio VII).

  La lista se revisa contra los docs al cerrar.
- **FR-023**: Al guardar un secreto, `rotate_every` DEBE ser una duración ISO 8601 en días o en semanas (p. ej. `P90D` o
  `P12W`); cualquier otro texto se rechaza con `validation`, como fija el FR-018 del cambio 005 para las entradas. Es el
  único cambio de comportamiento de la feature 002: un valor guardado antes que no sea una duración se sigue leyendo y
  no cuenta entre los secretos que deben rotarse.

### Key Entities *(include if feature involves data)*

- **Access Policy**: de una identidad; versionada. Recoge las excepciones de lectura que el admin le ha concedido y si
  tiene el permiso de estado. La regla por defecto (sus propios secretos) no se escribe: una identidad sin política
  solo lee lo suyo.
- **Excepción de lectura**: el permiso de una identidad para resolver un secreto concreto de otra; guarda quién la
  concedió y cuándo, y cuándo se retiró.
- **Permiso de estado**: deja a una identidad consultar el estado agregado de Hécate.
- **Referencia a un secreto**: `secret://<identidad>/<resto>`, opcionalmente con `@N` para una versión concreta.
- **Estado agregado**: si Hécate está sellada y los recuentos de secretos y certificados que necesitan atención, sin
  nombres.
- **Registro de accesos**: el de las features anteriores, ampliado con las resoluciones, las consultas del estado
  rechazadas y los cambios y consultas de políticas.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Con Hécate desbloqueada, una resolución de hasta 10 secretos, de cualquier tamaño permitido, tarda menos
  de 1 segundo desde que el componente envía la petición.
- **SC-002**: El 100 % de las peticiones con una referencia inexistente, de otra identidad sin excepción o sin versión
  resoluble se rechazan sin devolver ningún valor, con la misma respuesta, y quedan registradas.
- **SC-003**: Medidos sobre al menos 1.000 peticiones de cada tipo, la diferencia entre las medianas de los tiempos de
  respuesta de dos tipos de rechazo de SC-002 no supera la que hay entre las dos mitades de un mismo tipo, con el margen
  que fija el plan.
- **SC-004**: Ninguna inspección de lo que Hécate guarda o registra encuentra un valor resuelto ni su huella, y al
  terminar cada resolución, también si falla, el valor no queda en la memoria que controla Hécate.
- **SC-005**: Una identidad nunca resuelve un secreto de otra sin una excepción vigente; con ella lo resuelve, y al
  retirarla la siguiente petición se rechaza.
- **SC-006**: Para cualquier momento pasado, el admin puede saber qué secretos ajenos podía leer cada identidad, si
  tenía el permiso de estado y quién hizo cada cambio.
- **SC-007**: Ninguna respuesta del estado contiene una ruta, un nombre de identidad ni metadatos de un secreto
  concreto; responde también con Hécate sellada, y sin el permiso se rechaza siempre.
- **SC-008**: Un corte de luz en cualquier punto de un cambio de política deja la política anterior o la nueva.
- **SC-009**: El 100 % de las entradas que no siguen su formato estándar se rechazan con `validation` sin resolver ni
  cambiar nada.
- **SC-010**: La suite completa pasa en macOS y en un contenedor Linux, incluidas las pruebas de las features 001, 002
  y 003, y las comprobaciones de calidad pasan sin avisos.

## Assumptions

- **Base**: la Hécate que deja la feature 003, con su API entre componentes, su componente de prueba y la resolución
  interna de la feature 002 (FR-011 a FR-014 de la 002), que esta feature expone sin cambiar sus reglas.
- **Cachés de los componentes**: Hécate indica el tiempo de caché; guardar y refrescar los valores es del componente, y
  retirar una excepción no alcanza a lo que ya tenga en caché, que caduca como mucho a la hora.
- **Sin freno para la resolución**: quien resuelve está autenticado, cada intento queda atribuido a su identidad y todos
  los rechazos son iguales.
- **Tamaño de la respuesta**: con 100 referencias de hasta 64 KiB, una respuesta puede rondar los 6,5 MiB; no se limita
  aparte, porque quien resuelve está autenticado y solo recibe lo que puede leer.
- **Recuentos pequeños**: un recuento del estado puede ser 1 y aun así no nombra nada; el permiso de estado se concede a
  quien vigila, y se acepta que sepa cuántos secretos o certificados necesitan atención.
- **Políticas sin cifrar**: son metadatos, como el resto de eventos de Hécate, y no llevan ningún valor; por eso se
  pueden consultar con Hécate sellada.
- **Retención**: las filas del registro de accesos y los eventos de las políticas se conservan como el resto de la
  auditoría; el registro es append-only y esta feature no cambia su retención.
- **El admin opera desde el equipo de Hécate**, por el canal local y la CLI; la gestión desde Ténaro llegará con Ténaro.
- **Una sola instancia de Hécate**, sin alta disponibilidad; decenas de identidades y de excepciones.
- **Observabilidad**: sin OpenTelemetry, como en las features anteriores, hasta que DECISIONS cierre el colector y el
  backend; lo que pasa queda en el registro de accesos y en los logs estructurados.
- **Lo que decide el plan**: las operaciones exactas de la API y del canal local, cómo se consigue que los rechazos no
  se distingan por el tiempo y el modelo de datos de la política. La pila es la del ADR 0009; si hace falta tecnología
  nueva, se decide en un ADR antes de usarla (constitución, «Nueva tecnología»).
- **Fuera de alcance**: la firma de checkpoints de auditoría y su clave de firma de eventos, `wrap`/`unwrap`, la
  rotación de las claves de ámbito, las identidades hijas, los permisos que salen de las declaraciones de Hermes y
  Hades (llegan con esos componentes), la gestión y el desbloqueo desde Ténaro, el adapter de hardware, los avisos (los
  emitirá un cronjob de Hades a partir del estado), la rotación automática de secretos, OpenTelemetry y el lado de cada
  componente (su caché y su refresco), que se prueba con el componente de prueba de la feature 003. Orthros no cambia.
- **Referencias de diseño**: [docs/hecate.md](../../docs/hecate.md) (resolver un secreto, contratos y políticas),
  [DECISIONS](../../DECISIONS.md#abiertas) y las specs de las features
  [002](../002-hecate-secrets/spec.md) y [003](../003-hecate-identities/spec.md).
