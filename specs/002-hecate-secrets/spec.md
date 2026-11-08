# Feature Specification: Secretos de Hécate

**Feature Branch**: `feature/002-hecate-secrets`

**Created**: 2026-11-07

**Status**: Draft

**Input**: User description: "Resolver secretos de Hécate: guardar, versionar y leer secretos cifrados por capas con permisos por identidad"

## Clarifications

### Session 2026-11-07

- Q: ¿Esta feature expone la resolución a los componentes? → A: No: solo la lógica de resolución y permisos, con la
  identidad como dato de entrada; el canal autenticado llega con la feature de identidades.
- Q: ¿Puede el admin leer el valor de un secreto? → A: No: solo escribirlo; ve una huella corta de cada versión para
  comprobar que lo escribió bien.
- Q: ¿Destruir una versión exige volver a demostrarlo? → A: Sí: destruir exige la frase actual o dos partes (un fallo
  cuenta en el contador común de intentos); revocar, restaurar y promover no; la destrucción automática por superar el
  máximo de versiones tampoco.
- Q: ¿Qué puede consultar el admin sobre los secretos guardados? → A: Lista las rutas (con filtro opcional por
  identidad) y consulta cada secreto: metadatos y, por versión, número, estado, fecha y huella; nunca valores; cada
  consulta queda en el registro de accesos.
- Q: ¿Un secreto caducado se sigue resolviendo? → A: Sí: la caducidad es informativa; el estado de Hécate expone
  cuántos secretos han caducado o caducan pronto (sin rutas) y la consulta del admin muestra la fecha.

## User Scenarios & Testing *(mandatory)*

Hécate custodia los secretos de terceros de Cerbero (tokens de bots, API keys, credenciales). Cada secreto vive en
una ruta que empieza por la identidad a la que pertenece (`secret://<identidad>/<resto>`), tiene versiones y se
guarda cifrado por capas: la raíz protege una clave de ámbito por identidad, y esa clave protege una clave distinta para cada
versión de cada secreto. Esta feature cubre guardar secretos, darles versiones nuevas, gestionar su ciclo de vida y
que cada identidad lea solo los suyos.

Actores:

- **Admin**: el único usuario privilegiado; guarda secretos y gestiona sus versiones.
- **Identidad**: un componente de Cerbero (p. ej. Hermes o Hades) que necesita leer sus secretos para trabajar.

Todas las operaciones exigen Hécate desbloqueada (feature 001).

### User Story 1 - Guardar un secreto (Priority: P1)

El admin guarda un secreto nuevo en una ruta de una identidad (p. ej. `secret://hermes/telegram/family_bot/bot_token`),
con su tipo de valor. Hécate lo guarda cifrado como versión 1 y nunca lo deja en claro en disco ni en logs.

**Why this priority**: sin secretos guardados no hay nada que resolver; es la base del resto.

**Independent Test**: guardar un secreto y comprobar que existe con su versión 1, sus metadatos y su valor cifrado, y
que el valor no aparece en claro en ningún sitio.

**Acceptance Scenarios**:

1. **Given** Hécate desbloqueada, **When** el admin guarda un secreto en una ruta nueva con un valor válido para su
   tipo, **Then** queda guardado como versión 1 `active`.
2. **Given** un tipo de valor `json` o `pem`, **When** el valor no es válido para ese tipo, **Then** se rechaza sin
   guardar nada.
3. **Given** una ruta mal formada (sin identidad, con segmentos vacíos o caracteres no permitidos), **When** el admin
   intenta guardar, **Then** se rechaza.
4. **Given** Hécate sellada, **When** se intenta guardar un secreto, **Then** se rechaza indicando que está sellada.
5. **Given** secretos guardados de varias identidades, **When** el admin lista los de una identidad y consulta uno,
   **Then** ve las rutas, los metadatos y, por versión, número, estado, fecha y huella, sin ningún valor.

---

### User Story 2 - Una identidad resuelve sus secretos (Priority: P1)

Una identidad pide uno o varios de sus secretos y recibe, por cada uno, el valor, la versión y cuánto tiempo puede
guardarlo en su caché. Si pide un secreto de otra identidad, se le deniega.

**Why this priority**: es para lo que existen los secretos; sin resolverlos, ningún componente puede usar un tercero.

**Independent Test**: guardar secretos de dos identidades y comprobar, con la identidad como dato de entrada de la
resolución, que cada una resuelve los suyos (versión activa o una concreta) y que se le deniegan los de la otra,
quedando registrado. El canal autenticado por el que los componentes harán la petición llega con la feature de
identidades.

**Acceptance Scenarios**:

1. **Given** un secreto de `hermes`, **When** `hermes` lo resuelve sin indicar versión, **Then** recibe el valor de la
   versión `active`, su número de versión y el tiempo de caché.
2. **Given** varias referencias de `hermes`, **When** las resuelve en una sola petición, **Then** recibe todas, y si
   una no existe o no está permitida, la petición entera se rechaza sin devolver ningún valor.
3. **Given** un secreto de `hades`, **When** `hermes` lo pide, **Then** se le deniega y se registra el rechazo.
4. **Given** un secreto con varias versiones, **When** `hermes` pide una versión concreta (`@n`) que sigue siendo
   resoluble, **Then** recibe esa versión.
5. **Given** una petición de resolución, **When** se evalúan los permisos, **Then** la identidad que resuelve es la
   que aporta el canal autenticado, nunca un dato que la propia petición pueda elegir.

---

### User Story 3 - Rotar un secreto con una versión nueva (Priority: P2)

El admin añade una versión nueva cuando rota la credencial en el proveedor. La versión anterior deja de ser la
activa, pero sigue pudiéndose resolver de forma explícita durante un periodo de gracia, por si los componentes
tardan en recoger la nueva.

**Why this priority**: rotar credenciales es habitual; sin versiones, cambiar un secreto rompería a quien lo usa.

**Independent Test**: añadir una versión y comprobar que la nueva es la activa, que la anterior se resuelve pidiéndola
explícitamente durante la gracia y que deja de resolverse al terminar.

**Acceptance Scenarios**:

1. **Given** un secreto con la versión 1 `active`, **When** el admin añade un valor nuevo, **Then** la versión 2 queda
   `active` y la 1 pasa a `deprecated`.
2. **Given** la versión 1 `deprecated` dentro del periodo de gracia, **When** se pide `@1`, **Then** se resuelve.
3. **Given** el periodo de gracia terminado, **When** se pide `@1`, **Then** ya no se resuelve (queda `revoked`).
4. **Given** un secreto que supera su número máximo de versiones, **When** se añade otra, **Then** la versión más
   antigua se destruye.

---

### User Story 4 - Revocar, restaurar o volver a la anterior (Priority: P2)

Ante una fuga, el admin revoca una versión al instante, sin periodo de gracia. Si las credenciales nuevas no
funcionan, puede volver a activar la anterior.

**Why this priority**: la revocación es la respuesta a una fuga; volver atrás evita dejar sin servicio a un componente.

**Independent Test**: revocar la versión activa y comprobar que deja de resolverse al momento; restaurar una revocada
y promover una anterior, comprobando qué se resuelve en cada paso.

**Acceptance Scenarios**:

1. **Given** una versión `active` o `deprecated`, **When** el admin la revoca, **Then** deja de resolverse al instante.
2. **Given** una versión `revoked` no destruida, **When** el admin la restaura, **Then** vuelve a `deprecated`.
3. **Given** una versión `deprecated`, **When** el admin la promueve, **Then** pasa a `active` y la que era activa pasa
   a `deprecated`.
4. **Given** un secreto sin ninguna versión `active` (p. ej. tras revocar la activa), **When** se resuelve sin indicar
   versión, **Then** se rechaza indicando que no hay versión activa.

---

### User Story 5 - Destruir una versión (Priority: P3)

El admin destruye una versión revocada. Su valor queda irrecuperable para siempre, aunque el historial de lo que pasó
se conserva.

**Why this priority**: es el cierre del ciclo de vida; importante para no acumular credenciales antiguas.

**Independent Test**: destruir una versión revocada y comprobar que no se puede resolver ni restaurar, y que su
historial sigue presente.

**Acceptance Scenarios**:

1. **Given** una versión `revoked`, **When** el admin la destruye demostrándolo con la frase actual o dos partes,
   **Then** pasa a `destroyed` y su valor no se puede recuperar por ningún medio, ni con la raíz.
2. **Given** una versión que no está `revoked`, **When** el admin intenta destruirla, **Then** se rechaza.
3. **Given** una versión `destroyed`, **When** se intenta resolver o restaurar, **Then** se rechaza.
4. **Given** una versión `revoked`, **When** el admin intenta destruirla sin la frase actual ni dos partes válidas,
   **Then** se rechaza, la versión sigue `revoked` y cuenta como intento fallido en el contador común.

---

### Edge Cases

- Dos cambios simultáneos sobre el mismo secreto (p. ej. dos versiones nuevas a la vez): solo uno se aplica; el otro
  se rechaza y debe repetirse sobre el estado nuevo.
- Se va la luz a mitad de añadir una versión o de destruirla: el secreto queda en el estado anterior o en el nuevo,
  nunca a medias.
- Se pide un secreto que no existe y otro que es de otra identidad: ambos se rechazan sin revelar cuál de los dos
  casos es (no se puede averiguar qué secretos tiene otra identidad).
- Se reinicia Hécate: los secretos y sus estados siguen; para resolverlos hay que desbloquearla.
- El periodo de gracia vence mientras Hécate está sellada: al desbloquearla, la versión ya no se resuelve.
- Se pide una versión concreta que no existe: se rechaza igual que un secreto inexistente.
- Un secreto pasa su fecha de caducidad: se sigue resolviendo; el estado de Hécate lo cuenta entre los caducados.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hécate MUST identificar cada secreto por una ruta `secret://<identidad>/<resto>`, donde `<identidad>`
  es el nombre de la identidad dueña y `<resto>` uno o más segmentos de letras minúsculas, dígitos, `-` y `_`.
- **FR-002**: El admin MUST poder guardar un secreto nuevo indicando su ruta, su tipo de valor (`string`, `json` o
  `pem`) y su valor; Hécate MUST validar el valor contra el tipo antes de guardar.
- **FR-003**: Hécate MUST guardar cada versión de cada secreto cifrada con una clave propia de esa versión, protegida a
  su vez por la clave de ámbito de la identidad dueña, protegida por la raíz.
- **FR-004**: El valor cifrado MUST quedar ligado a su ruta, su versión y su identidad: moverlo a otra ruta, versión o
  identidad impide descifrarlo.
- **FR-005**: Hécate MUST NOT guardar ni registrar en ningún sitio el valor en claro de un secreto ni las claves que lo
  protegen sin cifrar.
- **FR-006**: El admin MUST poder añadir una versión nueva a un secreto existente; la nueva pasa a `active` y la
  anterior `active` pasa a `deprecated`.
- **FR-007**: Una versión `deprecated` MUST seguir resolviéndose cuando se pide explícitamente durante un periodo de
  gracia (24 h por defecto, configurable) y pasar a `revoked` al terminarlo.
- **FR-008**: El admin MUST poder revocar una versión `active` o `deprecated` sin periodo de gracia, restaurar una
  `revoked` no destruida a `deprecated`, y promover una `deprecated` a `active`.
- **FR-009**: El admin MUST poder destruir una versión `revoked`, siempre que lo demuestre de nuevo con la frase actual
  o con dos partes (un fallo cuenta en el contador común de intentos de la feature 001); destruirla elimina la clave
  propia de esa versión, de modo que su valor queda irrecuperable, y conserva su historial. Revocar, restaurar y
  promover no exigen esa prueba, ni la destrucción automática de FR-010.
- **FR-010**: Cada secreto MUST tener un máximo de versiones (10 por defecto); al superarlo, la versión más antigua se
  destruye.
- **FR-011**: Una identidad MUST poder resolver uno o varios de sus secretos en una sola petición, indicando o no la
  versión; por cada uno recibe el valor, el número de versión y el tiempo de caché (5 min de caché blanda y 1 h dura).
- **FR-012**: Una identidad MUST NOT poder resolver secretos fuera de `secret://<su identidad>/**`; todo lo demás está
  denegado.
- **FR-013**: Si alguna referencia de una petición de resolución no existe, no es resoluble o no está permitida, la
  petición entera MUST rechazarse sin devolver ningún valor, y la respuesta MUST NOT distinguir "no existe" de "no
  permitido".
- **FR-014**: Sin indicar versión, se resuelve la versión `active`; si no hay ninguna, la resolución MUST rechazarse
  con el mismo motivo que un secreto inexistente (FR-013).
- **FR-015**: Todas las operaciones sobre secretos MUST exigir Hécate desbloqueada, salvo los recuentos de caducidad
  de `status` (FR-022), que no tocan material cifrado.
- **FR-016**: Hécate MUST registrar en el registro de accesos cada escritura, cambio de estado, consulta, resolución y
  rechazo, con quién, qué ruta y qué versión, nunca con el valor.
- **FR-017**: Los cambios sobre un secreto MUST aplicarse de forma atómica y llevar la versión esperada del secreto;
  si otro cambio se adelantó, el cambio se rechaza.
- **FR-018**: Hécate MUST crear la clave de ámbito de una identidad la primera vez que se guarda un secreto suyo.
- **FR-019**: El admin MUST poder escribir valores pero MUST NOT poder leerlos; para comprobar que escribió bien un
  valor, Hécate le muestra una huella corta de cada versión, que no permite recuperar el valor.
- **FR-020**: Cada secreto MUST guardar sus metadatos: dueño, tipo de valor, máximo de versiones, y opcionalmente
  cada cuánto rotarlo y cuándo caduca.
- **FR-021**: El admin MUST poder listar las rutas de los secretos, con filtro opcional por identidad, y consultar
  cada secreto: sus metadatos y, por versión, número, estado, fecha y huella. Ninguna consulta devuelve un valor, y
  cada una queda en el registro de accesos.
- **FR-022**: La fecha de caducidad de un secreto MUST ser solo informativa: un secreto caducado se sigue resolviendo.
  El estado de Hécate MUST exponer cuántos secretos han caducado y cuántos caducan en los próximos 7 días, sin rutas ni
  ningún otro dato de un secreto concreto, y la consulta del admin MUST mostrar la fecha de cada uno.

### Key Entities *(include if feature involves data)*

- **Secreto**: una ruta con sus metadatos (dueño, tipo de valor, máximo de versiones, rotación y caducidad opcionales)
  y sus versiones.
- **Versión de secreto**: número, estado (`active`, `deprecated`, `revoked`, `destroyed`), fecha, huella corta del valor
  y valor cifrado con su clave propia (que desaparece al destruirla).
- **Clave de ámbito** (una por identidad): protege las claves de las versiones de los secretos de una identidad; está protegida por la
  raíz. Una por identidad.
- **Registro de accesos**: cada escritura, cambio de estado, resolución y rechazo, con quién, ruta, versión y
  resultado, sin valores.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Una identidad resuelve 10 secretos en una sola petición en menos de 1 segundo con Hécate desbloqueada.
- **SC-002**: Ninguna inspección de lo que Hécate guarda o registra encuentra el valor en claro de un secreto ni una
  clave sin cifrar.
- **SC-003**: El 100 % de las peticiones de una identidad sobre secretos de otra se deniegan y quedan registradas.
- **SC-004**: Una versión destruida no se recupera por ningún medio, ni con la raíz ni restaurando una copia posterior
  a la destrucción.
- **SC-005**: Tras una rotación, la versión anterior se resuelve explícitamente durante todo el periodo de gracia y en
  ningún momento después.
- **SC-006**: Un corte de luz en cualquier punto de una escritura o un cambio de estado deja siempre el secreto en su
  estado anterior o en el nuevo.
- **SC-007**: Las respuestas de rechazo no permiten saber si un secreto de otra identidad existe.
- **SC-008**: Ninguna operación del admin devuelve el valor de un secreto; la huella que ve no permite recuperarlo.

## Assumptions

- **Identidades sin registro todavía**: hasta la feature de identidades, una identidad es un nombre (minúsculas,
  dígitos y `-`) que encabeza la ruta; no hay altas ni bajas de identidades en esta feature.
- **Resolución sin canal todavía**: esta feature implementa la resolución y los permisos con la identidad como dato
  de entrada; el canal autenticado entre componentes (mTLS con identidad SPIFFE) llega con la feature de identidades,
  que lo conectará a esta resolución. Hasta entonces ningún componente puede resolver desde fuera de Hécate, y ningún
  canal provisional lo sustituye.
- **Fuera de alcance:** rotar las claves de ámbito (gestión de claves), compartir un secreto entre identidades
  (excepción explícita y auditada), los permisos derivados de las declaraciones de Hermes y Hades, las identidades
  hijas, los avisos de rotación y caducidad (los emite un cronjob de Hades a partir de los recuentos del estado), `wrap`/`unwrap` y
  la caché de los componentes (Hécate solo indica el tiempo de caché).
- **Tamaño máximo de un valor**: 64 KiB, suficiente para tokens, credenciales y certificados PEM.
- **El admin opera desde el equipo de Hécate**, por el canal local de administración de la feature 001; la gestión
  desde Ténaro llegará con Ténaro.
- **Referencias de diseño**: [docs/hecate.md](../../docs/hecate.md) (aggregates, jerarquía de claves, ciclo de vida,
  políticas) y las desviaciones de Hécate en [DECISIONS](../../DECISIONS.md).
