# Feature Specification: Hécate en Rust: migración con paridad de las features 001 y 002

**Feature Branch**: `refactor/hecate-rust`

**Created**: 2026-11-08

**Status**: Draft

**Input**: User description: "Migrar Hécate de Python a Rust con paridad de comportamiento de lo que ya está hecho: las
features 001 (arranque y sellado: inicialización, desbloqueo, sellado y partes de recuperación) y 002 (secretos: cifrado
por capas, versiones, ciclo de vida y resolución por identidad), como deciden los ADR 0005, 0006 y 0007. El canal local de
administración mantiene sus operaciones, su sobre y sus códigos de error, y se conserva el modelo de datos en SQLite; las
partes de recuperación cambian de formato (Shamir en GF(256)) y los datos de desarrollo se inicializan de nuevo. La
paridad la definen las pruebas y los quickstarts de 001 y 002, que se trasladan caso a caso, y se cumplen las
comprobaciones de calidad del ADR 0006 con la política de licencias del ADR 0008. Al terminar se elimina la Hécate en
Python y su miembro del workspace de uv, y se consolidan los docs que el ADR 0005 asocia a Hécate: el README (árbol y
proyectos en Python), el doc de Hécate (librerías del CryptoProvider y riesgo de memoria) y el riesgo de memoria de
DECISIONS, que se reescribe para Rust. Fuera de alcance: la conversión de Orthros y su README, la feature 003
(identidades, CA y API mTLS), que sigue pausada hasta que se fusione este cambio, y el adapter de hardware. Rama
refactor/hecate-rust; commit de squash refactor:."

## Clarifications

### Session 2026-11-08

- Q: ¿La nueva implementación debe guardar los datos con el mismo formato que la de Python (cifrado por capas, datos
  asociados, huella y derivación de la frase) o basta con que se comporte igual? → A: Mismo formato, comprobado con
  vectores de prueba generados con la versión en Python antes de borrarla y comparando el esquema de SQLite; no hace
  falta abrir bases de datos antiguas.
- Q: ¿Qué tiene que ser idéntico en lo que muestra y devuelve la CLI? → A: Las respuestas del canal y los códigos de
  salida, idénticos; los textos de la CLI, idénticos donde los comprueban las pruebas y los quickstarts. Las órdenes
  `hecate …` no cambian, solo cómo se lanzan (ya no con `uv run`), y los pasos traducidos van en el quickstart de este
  cambio.
- Q: ¿En qué sistemas tiene que pasar la paridad para dar el cambio por terminado? → A: En macOS y, además, con la suite
  completa en un contenedor Linux antes de fusionar; las medidas de tiempo en el servidor siguen pendientes.
- Q: ¿Cómo se da por cumplido que los secretos de paso se borran de la memoria? → A: Por diseño y revisión: cada secreto
  de paso vive en un tipo que se borra al soltarse, la revisión comprueba que ninguno sale de esos tipos y las pruebas
  unitarias comprueban que esos tipos se borran.
- Q: ¿Qué JSON acepta el canal local, al que llegarán peticiones de otras apps, y cómo responde? → A: Solo JSON estándar
  en UTF-8, como dice el contrato de la feature 001, con cadenas de texto válido; lo demás (por ejemplo `NaN`,
  `Infinity`, números fuera de rango, sustitutos sueltos, BOM, UTF-16 o UTF-32) se rechaza con `validation` «request is
  not valid JSON», sin fila en el registro de accesos. Con JSON válido se mantienen los códigos, los mensajes y las filas
  del registro de accesos de la versión en Python. Las respuestas y los logs son JSON con los mismos campos y valores,
  sin imitar los separadores ni los escapes de la versión en Python.
- Q: ¿Qué acepta el tipo de valor `json` de los secretos, que leerán otras apps? → A: JSON estándar, para que pueda
  leerlo cualquier app que reciba el secreto: `NaN` e `Infinity`, que la versión en Python aceptaba, se rechazan. Es un
  cambio a propósito y queda en el mapeo.
- Q: ¿El JSON que se guarda en SQLite (eventos y protecciones del sellado) debe ser igual byte a byte que el de Python?
  → A: Igual en contenido, con las mismas claves y valores; los vectores comparan el JSON leído, no sus bytes. El
  esquema de SQLite y los datos asociados del cifrado siguen siendo idénticos byte a byte.

### Session 2026-11-09

- Q: ¿«JSON estándar» significa lo mismo en el canal y en el tipo `json`? → A: Sí: JSON con cadenas de texto válido,
  números representables en doble precisión y como mucho 127 niveles de anidamiento; lo demás se rechaza en los dos
  sitios.
- Q: ¿El criterio de no imitar las tolerancias de Python se aplica también a los PEM y a `expires_at`? → A: Sí: `pem`
  acepta solo espacios ASCII y bloques con contenido, y `expires_at`, ISO 8601 extendido; la tabla de caracteres
  imprimibles de Unicode 17 se mantiene.

## User Scenarios & Testing *(mandatory)*

Las features 001 y 002 dejaron Hécate funcionando en Python: se inicializa, se desbloquea con la frase del admin o con
dos partes de recuperación y se sella, y guarda secretos cifrados por capas, con versiones, ciclo de vida y resolución por
identidad. El [ADR 0005](../../docs/adr/0005-rust-para-orthros-y-hecate.md) la pasa a Rust para que los secretos de paso
no se queden en memoria y para que su futura API pueda leer el certificado del cliente. Este cambio reescribe Hécate en
Rust sin cambiar lo que hace: ni el admin ni las pruebas deben notar la diferencia, salvo el formato de las partes de
recuperación y el rechazo de las entradas que no siguen su formato estándar. Al terminar, la versión en Python
desaparece y los docs describen la nueva.

Actores:

- **Admin**: inicializa, desbloquea y sella Hécate y gestiona los secretos con la CLI, por el canal local de
  administración.
- **Identidad**: un componente que resuelve sus propios secretos; hoy es un caso de uso interno, sin canal, que llega
  con la feature 003.
- **Propietario**: revisa cada fase y decide lo que cambia a propósito.
- **Agente de desarrollo**: traslada las pruebas y el código y demuestra la paridad.

### User Story 1 - El admin arranca, desbloquea y sella Hécate como hasta ahora (Priority: P1)

El admin quiere inicializar Hécate, desbloquearla con su frase o con dos partes, sellarla, consultar su estado y cambiar
la frase o las partes exactamente como con la versión en Python (feature 001): con las mismas órdenes, respuestas,
esperas y registros.

**Why this priority**: es lo que protege la raíz de todo Cerbero; sin arranque y sellado no hay secretos, y una
regresión aquí bloquea todo lo demás.

**Independent Test**: los casos de prueba de la feature 001, trasladados, pasan contra la nueva implementación, y el
quickstart de 001 se completa de principio a fin con la nueva CLI.

**Acceptance Scenarios**:

1. **Given** cualquier escenario de aceptación de la feature 001, **When** se ejecuta con la nueva implementación,
   **Then** el resultado (estado, respuesta, código de error, espera y registro) es el mismo que con la versión en Python.
2. **Given** una inicialización, **When** termina, **Then** las tres partes tienen el formato nuevo, dos cualesquiera
   desbloquean y una sola nunca lo hace.
3. **Given** una parte del formato anterior, **When** el admin intenta desbloquear con ella, **Then** se rechaza como mal
   copiada, sin contar como intento fallido.
4. **Given** tres intentos fallidos seguidos de desbloqueo, **When** el admin sigue intentándolo, **Then** las esperas son
   las mismas que en la versión en Python y el contador sobrevive a un reinicio.
5. **Given** los vectores de prueba de la derivación de la frase y de la protección de la raíz generados con la versión
   en Python, **When** la nueva implementación los procesa, **Then** obtiene exactamente los mismos bytes.

---

### User Story 2 - El admin gestiona secretos y una identidad los resuelve como hasta ahora (Priority: P1)

El admin quiere guardar, versionar, listar y consultar secretos (sin ver nunca su valor), y revocarlos, restaurarlos o
destruirlos; y una identidad quiere resolver los suyos. Todo, exactamente como con la versión en Python (feature 002).

**Why this priority**: es la función de Hécate para el resto de componentes, y la feature 003 se construye encima.

**Independent Test**: los casos de prueba de la feature 002, trasladados, pasan contra la nueva implementación, y el
quickstart de 002 se completa de principio a fin con la nueva CLI.

**Acceptance Scenarios**:

1. **Given** cualquier escenario de aceptación de la feature 002, **When** se ejecuta con la nueva implementación,
   **Then** el resultado es el mismo que con la versión en Python.
2. **Given** un valor cifrado, **When** se mueve a otra ruta, versión o identidad, **Then** no se puede descifrar.
3. **Given** una versión destruida, **When** se intenta recuperar por cualquier medio, **Then** no se recupera.
4. **Given** una identidad, **When** pide un secreto de otra, **Then** se deniega sin revelar si existe y queda
   registrado.
5. **Given** los vectores de prueba del cifrado por capas, los datos asociados y la huella generados con la versión en
   Python, **When** la nueva implementación los procesa, **Then** obtiene exactamente los mismos bytes.

---

### User Story 3 - Los secretos de paso no se quedan en memoria (Priority: P2)

El propietario quiere que la nueva implementación borre de la memoria los secretos que solo se usan de paso (la frase,
las partes, la clave derivada de la frase, la clave de recuperación, las claves de ámbito y de versión y los valores en
claro) en cuanto termina la operación que los usa. Es lo que justifica el cambio de lenguaje: Python no permite
garantizarlo y es un riesgo aceptado en DECISIONS.

**Why this priority**: sin esto, la migración no aporta su principal ventaja de seguridad; va después de la paridad
porque una regresión de comportamiento es más grave que un riesgo ya aceptado.

**Independent Test**: la revisión comprueba que cada secreto de paso vive en un tipo que se borra al soltarse, sin copias
fuera de él, y las pruebas unitarias comprueban que esos tipos se borran; la raíz solo vive en memoria mientras Hécate
está desbloqueada.

**Acceptance Scenarios**:

1. **Given** un desbloqueo, **When** termina, **Then** la frase o las partes, y la clave derivada de la frase o la clave
   de recuperación, ya no están en memoria; solo queda la raíz.
2. **Given** una escritura o una resolución de secretos, **When** termina, **Then** las claves de ámbito y de versión y
   los valores en claro ya no están en memoria.
3. **Given** Hécate desbloqueada, **When** el admin la sella, **Then** la raíz se borra de la memoria.
4. **Given** una operación que falla a medias, **When** se aborta, **Then** los secretos de paso que llegó a usar también
   se borran.

---

### User Story 4 - La versión en Python desaparece y los docs describen la nueva (Priority: P3)

El propietario quiere que, al terminar, la Hécate en Python no quede en el repositorio ni en el workspace de uv, que pasen
las comprobaciones de calidad y que los docs que describían la implementación en Python describan la de Rust.

**Why this priority**: dos implementaciones a la vez se desincronizan, y los docs vigentes deben describir lo que hay;
pero solo tiene sentido cuando la nueva ya cumple la paridad.

**Independent Test**: el repositorio no contiene la versión en Python ni el workspace de uv la incluye; las
comprobaciones de calidad pasan; el README, el doc de Hécate y DECISIONS describen la versión en Rust.

**Acceptance Scenarios**:

1. **Given** el repositorio al terminar, **When** se busca la Hécate en Python (código, pruebas, manifiesto y miembro del
   workspace de uv), **Then** no aparece.
2. **Given** las comprobaciones de calidad del [ADR 0006](../../docs/adr/0006-pruebas-rust-cargo-proptest.md), **When**
   se ejecutan, **Then** pasan sin avisos y sin dependencias fuera de la política de licencias del
   [ADR 0008](../../docs/adr/0008-excepcion-openssl-agpl.md).
3. **Given** el README, el doc de Hécate y DECISIONS, **When** se leen, **Then** describen la implementación en Rust: la
   estructura del repositorio, las librerías y el riesgo de memoria reescrito.
4. **Given** Orthros, su README y la feature 003, **When** se revisan al terminar, **Then** no han cambiado.

---

### Edge Cases

- Una parte del formato anterior se rechaza como mal copiada, sin contar como intento fallido, como cualquier parte
  que no supera la comprobación (FR-009 de la feature 001).
- Los datos creados por la versión en Python no se migran ni se garantiza abrirlos: una instalación de desarrollo se
  inicializa de nuevo antes del primer arranque de la nueva versión.
- Las entradas límite (fechas con zona horaria, caracteres imprimibles, el base64 de los PEM, el orden y la forma de los
  argumentos del canal) se aceptan o se rechazan exactamente igual que en la versión en Python: la paridad incluye la
  validación, salvo lo que fija FR-018.
- Un caso de prueba de Python que comprueba un detalle interno, y no un comportamiento, se traslada como prueba del
  comportamiento equivalente; si no lo hay, el mapeo lo explica.
- Los únicos cambios de comportamiento a propósito son el formato de las partes (FR-005) y el rechazo de las entradas que
  no siguen su formato estándar (FR-018); cualquier otro necesita una decisión del propietario y queda en el mapeo.
- Un corte de luz en cualquier punto deja Hécate y sus secretos con las mismas garantías de atomicidad que en las
  features 001 y 002.
- La fila de la feature 003 en los cambios pausados de DECISIONS se queda: la retira la PR que fusione la 003.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hécate DEBE estar implementada en Rust (edition 2024), como workspace de Cargo en `hecate/` fuera del
  workspace de uv (constitución, «Repositorio» y «Lenguajes»; ADR 0005), con la pila del
  [ADR 0007](../../docs/adr/0007-pila-hecate-rust.md).
- **FR-002**: La nueva implementación DEBE cumplir todos los requisitos funcionales y criterios de éxito de las features
  001 y 002, salvo el cambio de formato de las partes de recuperación (FR-005) y el rechazo de las entradas que no
  siguen su formato estándar (FR-018).
- **FR-003**: El canal local de administración DEBE mantener sus operaciones, su sobre y sus códigos de error, según los
  contratos de las features 001 y 002, con respuestas equivalentes: los mismos campos, valores, códigos y mensajes, y las
  mismas filas en el registro de accesos. La forma de escribir el JSON (separadores y escapes) puede cambiar.
- **FR-004**: La CLI DEBE mantener su nombre, sus órdenes, sus opciones y sus códigos de salida, y sus textos DEBEN ser
  idénticos donde los comprueban las pruebas y los quickstarts de 001 y 002; los textos de ayuda pueden cambiar. La
  configuración DEBE mantener sus variables de entorno (`HECATE_DATA_DIR`, `HECATE_SOCKET_PATH` y `HECATE_KDF_PROFILE`)
  y sus valores por defecto. Solo cambia cómo se lanza la CLI, que ya no es con uv.
- **FR-005**: Las partes de recuperación DEBEN generarse en el formato nuevo del ADR 0007 con las mismas reglas que hoy:
  tres partes, dos cualesquiera desbloquean, una sola nunca, y una parte mal copiada se detecta antes de contar como
  intento. Una parte del formato anterior DEBE rechazarse como mal copiada.
- **FR-006**: La nueva implementación DEBE conservar el modelo de datos de 001 y 002: byte a byte, el esquema de SQLite
  (tablas, eventos y registro de accesos), el formato de lo que guarda cifrado (cifrado por capas, datos asociados y
  huella) y los perfiles de coste para derivar la clave de la frase; en contenido (mismas claves y valores), el JSON que
  guarda, que no tiene que imitar los separadores ni los escapes de la versión en Python. De lo cifrado y lo repartido,
  solo cambian las partes de recuperación (FR-005).
  Se comprueba con vectores de prueba generados con la versión en Python antes de borrarla, que se guardan con las
  pruebas, y comparando el esquema. Los datos creados por la versión en Python no se migran ni hace falta abrirlos: una
  instalación de desarrollo se inicializa de nuevo.
- **FR-007**: Los secretos de paso (la frase, las partes, la clave derivada de la frase, la clave de recuperación, las
  claves de ámbito y de versión y los valores en claro) DEBEN borrarse de la memoria al terminar la operación que los
  usa, también si falla; la raíz DEBE estar en memoria solo mientras Hécate está desbloqueada. Se comprueba por diseño y
  revisión: cada secreto de paso vive en un tipo que se borra al soltarse, la revisión comprueba que ninguno sale de
  esos tipos y las pruebas unitarias comprueban que esos tipos se borran.
- **FR-008**: Cada caso de prueba de las features 001 y 002 DEBE tener su equivalente en la nueva implementación,
  relacionado caso a caso en un mapeo, y los cambios a propósito (el formato de las partes y el rechazo de las entradas
  que no siguen su formato estándar) DEBEN figurar en él.
- **FR-009**: Los quickstarts de las features 001 y 002 DEBEN completarse de principio a fin con la nueva CLI. Como los
  quickstarts fusionados no se editan, sus pasos, con las órdenes traducidas a la nueva implementación (cómo se lanza la
  CLI y cómo se ejecutan las pruebas), van en el quickstart de este cambio.
- **FR-010**: DEBEN pasar las comprobaciones de calidad del ADR 0006 (formato, análisis estático con los avisos como
  errores y revisión de dependencias), con la política de licencias del ADR 0008.
- **FR-011**: Al terminar, el repositorio NO DEBE contener la Hécate en Python (código, pruebas y manifiesto) y el
  workspace de uv NO DEBE incluirla.
- **FR-012**: El README DEBE describir la estructura nueva: Hécate como workspace de Cargo en el árbol del repositorio y
  en el párrafo de los proyectos, con la referencia de lenguajes al ADR 0005.
- **FR-013**: El doc de Hécate DEBE describir la implementación en Rust: las librerías del `CryptoProvider` (apartados 1
  y 7), cómo se pide el valor de un secreto (apartado 4) y el riesgo de memoria del resumen de decisiones y riesgos
  (apartado 12).
- **FR-014**: DECISIONS DEBE reescribir el riesgo aceptado de memoria para la versión en Rust (la raíz sigue en memoria
  mientras Hécate está desbloqueada; los secretos de paso se borran al usarse) y actualizar las filas que nombran la
  implementación en Python, como el riesgo de Shamir.
- **FR-015**: Este cambio NO DEBE modificar Orthros ni su README, ni la feature 003 ni su fila en DECISIONS, ni añadir
  funcionalidad nueva a Hécate.
- **FR-016**: La resolución por identidad DEBE seguir siendo un caso de uso interno, sin canal, hasta la feature 003.
- **FR-017**: La suite completa DEBE pasar en macOS, donde se desarrolla, y en un contenedor Linux antes de fusionar,
  para cubrir el código que depende del sistema: la credencial del par en el canal local, el bloqueo de memoria y la
  prohibición de volcados.
- **FR-018**: Las entradas DEBEN seguir su formato estándar, sin las tolerancias de la versión en Python; lo demás se
  rechaza con `validation`:
  - el canal local acepta solo peticiones en JSON estándar en UTF-8; las demás se rechazan con «request is not valid
    JSON» y sin fila en el registro de accesos;
  - el tipo de valor `json` acepta solo JSON estándar;
  - el tipo de valor `pem` acepta solo bloques con espacios ASCII y con contenido;
  - `expires_at` acepta solo una fecha, o una fecha y hora, en ISO 8601 extendido, con desfase opcional (sin desfase,
    UTC).

  JSON estándar es, aquí, JSON con cadenas de texto válido, números representables en doble precisión y como mucho 127
  niveles de anidamiento. Son cambios a propósito respecto a la versión en Python, que aceptaba más.

### Key Entities *(include if feature involves data)*

- **Hécate**: el servicio, con sus tres estados (sin inicializar, sellada y desbloqueada), su canal local de
  administración y su CLI.
- **Parte de recuperación**: una de las tres partes que reparten la clave de recuperación; es lo único cifrado o
  repartido que cambia de formato.
- **Secreto y versión**: igual que en la feature 002 (ruta, tipo de valor, versiones, estados y metadatos).
- **Registro de accesos**: igual que en las features 001 y 002.
- **Secreto de paso**: dato secreto que solo existe durante una operación: la frase, las partes, la clave derivada de la
  frase, la clave de recuperación, las claves de ámbito y de versión y los valores en claro. Vive siempre en un tipo que
  se borra al soltarse.
- **Mapeo de pruebas**: la relación caso a caso entre las pruebas de 001 y 002 y las de la nueva implementación, con los
  cambios a propósito.
- **Vector de prueba**: una entrada y su salida esperada (derivación de la frase, protección de la raíz, cifrado por
  capas, datos asociados, huella, eventos, instantes, validación de valores, caracteres imprimibles o esquema), generada
  con la versión en Python, que la nueva implementación debe reproducir salvo los cambios a propósito.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: El 100 % de los casos de prueba de las features 001 y 002 (346) tiene su equivalente en el mapeo, y todos
  pasan con la nueva implementación, en macOS y en un contenedor Linux.
- **SC-002**: Los quickstarts de 001 y 002 se completan de principio a fin con la nueva CLI, con los pasos traducidos
  del quickstart de este cambio: las órdenes `hecate …`, las salidas que se comprueban y la configuración no cambian,
  salvo el formato de las partes y la forma de lanzar la CLI.
- **SC-003**: Se siguen cumpliendo los criterios de éxito de 001 y 002; por ejemplo, el admin desbloquea con la frase en
  menos de 1 minuto, una identidad resuelve 10 secretos en menos de 1 segundo y, tras el 100 % de los reinicios, Hécate
  está sellada.
- **SC-004**: El 100 % de los secretos de paso vive en tipos que se borran al soltarse, sin copias fuera de ellos según la
  revisión, y las pruebas unitarias de esos tipos pasan.
- **SC-005**: Al cerrar la rama no queda ningún fichero de la Hécate en Python, y el workspace de uv no la incluye.
- **SC-006**: Las comprobaciones de calidad pasan sin avisos y sin dependencias fuera de la política de licencias.
- **SC-007**: Ningún doc vigente describe la Hécate en Python.
- **SC-008**: La nueva implementación reproduce el 100 % de los vectores de prueba generados con la versión en Python
  (los bytes de lo cifrado, de los datos asociados y de la huella, el contenido del JSON guardado y la validación, salvo
  los cambios a propósito de FR-018), y su esquema de SQLite es idéntico al de la versión en Python.

## Assumptions

- Las decisiones las tomó el propietario en los ADR 0005 a 0008; este cambio las implementa y no las reabre.
- La paridad es del comportamiento observable (canal, CLI, configuración, modelo de datos, registros y esperas), y la
  definen las pruebas y los quickstarts de 001 y 002. La estructura interna del código puede cambiar, dentro del esqueleto
  hexagonal de la constitución (principio III).
- No hace falta abrir una base de datos creada por la versión en Python: los datos de desarrollo se inicializan de nuevo
  (ADR 0005).
- La pila es la del ADR 0007; cualquier dependencia que no esté en él se justifica en el plan con los criterios de la
  constitución («Herramientas»).
- En el servidor de casa siguen pendientes, desde la feature 001, las medidas de tiempo; el comportamiento que depende de
  Linux (credencial del par, bloqueo de memoria y volcados) se comprueba en el contenedor (FR-017).
- La feature 003 sigue pausada hasta que se fusione este cambio; después se replanifica sobre la nueva implementación.
- La rama se llama `refactor/hecate-rust` y su commit de squash es `refactor:`.
