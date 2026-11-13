# Feature Specification: Hécate documentada antes de pausarla

**Feature Branch**: `docs/hecate-reference`

**Created**: 2026-11-12

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/007-hecate-reference. Documentar el estado actual de Hécate
antes de pausarla para seguir con otros componentes. Hécate tiene hechas las features 001, 002, 003, 005 y 006: sellado,
secretos, identidades, Access Policy y resolución por la API. Pero su documentación vigente es un solo documento de diseño
(docs/hecate.md) con la plantilla de 14 apartados, y lo necesario para usarla está repartido en specs ya cerradas: las
órdenes de la CLI, el contrato de la API para los componentes y el flujo con el que un componente obtiene y renueva su
certificado. Las variables de entorno no aparecen en ningún doc vigente. Este cambio reorganiza la documentación de Hécate
en un documento de alto nivel y documentos específicos. docs/hecate.md explica por qué existe Hécate, qué resuelve, qué
hace y qué no, y quién la usa. Introduce cada concepto en un párrafo que enlaza a su documento en docs/hecate/: sellado
(raíz, frase, partes, desbloqueo y sellado); secretos (cifrado por sobre, versiones, huella, metadatos, rotación de las
claves de ámbito y almacenamiento); identidades (CA en dos niveles y ceremonias, alta, renovación, revocación y
certificado propio); acceso (Access Policy, resolución y permiso de estado); auditoría (registro de accesos y
checkpoints); integración, para quien desarrolla un componente (el flujo completo desde el token de alta, los
certificados y el token corto desde el lado del componente, las operaciones de la API remitiendo al OpenAPI de
shared/contracts, los errores, Hécate sellada y la caché); CLI y configuración (cómo se lanza, las variables de entorno y
cada orden con sus argumentos, salidas y códigos de salida); y operación (la puesta en marcha y el procedimiento de cada
situación de la tabla de operación, que hoy dice que son procedimientos escritos y no lo son). hecate/README.md pasa a
explicar cómo se desarrolla Hécate: compilar, la suite en macOS y en un contenedor Linux, las comprobaciones de calidad,
el componente de prueba y la organización de los crates. El estado se documenta según las reglas de docs. Lo diseñado y
sin construir pasa a «Abiertas» de DECISIONS como diseñado y no implementado: la firma de checkpoints, wrap/unwrap, las
identidades hijas, los permisos que salen de las declaraciones, la rotación de las claves de ámbito y la gestión y el
desbloqueo desde Ténaro. El README gana en «Estado del Proyecto» lo que ya hace Hécate. La constitución nombra
docs/<componente>.md como la fuente del diseño vigente. Se aclara, con una subida PATCH y su justificación en el
historial, que un componente puede desarrollar sus conceptos en docs/<componente>/. Los docs describen lo que hay, sin
futuro ni roadmap. No se pierde ni se contradice nada de lo que hoy dicen docs/hecate.md, ARCHITECTURE, GLOSSARY ni
DECISIONS. Cada dato (órdenes, variables, operaciones y códigos) se contrasta con el código y el OpenAPI. El resto de docs
de componente mantiene su plantilla. Fuera de alcance: cambiar el código o el comportamiento de Hécate, documentar aparte
el protocolo del canal local (su único cliente es la CLI), modificar las specs de cambios ya fusionados y reestructurar
los docs de otros componentes. Rama docs/hecate-reference, creada desde main; commit de squash docs:."

## Clarifications

### Session 2026-11-12

- Q: ¿Dónde se ve que una pieza diseñada de Hécate aún no está construida? → A: Solo en DECISIONS y en el README del
  proyecto. Los documentos de diseño describen el diseño sin decir nada de su estado, y las referencias de uso
  (integración, CLI y operación) describen solo lo que existe.
- Q: ¿Dónde se desarrolla la seguridad de Hécate (frente a qué protege, qué mecanismo cubre cada amenaza y sus riesgos
  aceptados), que hoy es un apartado transversal del documento de diseño? → A: En su propio documento específico, que
  enlaza a cada concepto; el documento de alto nivel lo introduce como a los demás.
- Q: ¿La guía de integración lleva ejemplos de código? → A: No: formatos exactos, secuencias con diagramas y, como
  cliente de referencia completo y probado, el componente de prueba de Hécate. Cada componente elige su lenguaje.
- Q: ¿La documentación explica cómo dejar Hécate corriendo como servicio en el servidor de casa? → A: No. La referencia
  dice cómo se lanza y qué necesita del sistema para arrancar, comprobado con el código. Cómo corren los componentes en el
  home lab (gestor de servicios o contenedores) es una decisión común a todos: pasa a «Abiertas» de DECISIONS, y su
  procedimiento se escribirá al desplegar.

## User Scenarios & Testing *(mandatory)*

El 2026-11-12, tras fusionar la feature 006, el propietario decidió pausar Hécate para seguir con otros componentes. Su
núcleo está construido: el sellado (001), los secretos (002, portados a Rust en el 005), las identidades (003) y la Access
Policy con la resolución por la API (006). Lo que falta de su diseño depende casi todo de componentes que aún no existen.

Antes de pausarla, este cambio deja su documentación en un estado que permita usarla y retomarla sin reconstruir nada
desde las specs. Hoy hay un solo documento de diseño largo, y lo necesario para usar Hécate está repartido en specs ya
cerradas, que no se actualizan. La documentación pasa a tener un documento de alto nivel, que cuenta por qué existe Hécate
y qué hace, y documentos específicos, más pequeños, que desarrollan cada concepto y cada uso. No cambia código ni
comportamiento: implementar aquí es escribir, reorganizar y contrastar documentación.

En esta spec, «documento de alto nivel» es `docs/hecate.md`; «documento específico» es el que desarrolla un concepto
(sellado, secretos, identidades, acceso, auditoría, seguridad) o un uso (integración, CLI y configuración, operación); y
«dato» es
todo lo que se puede comprobar contra el código o el contrato de la API: una orden, un argumento, una salida, un código
de salida, una variable de entorno y su valor por defecto, una operación, un campo, un código de error, un límite o un
tiempo.

Actores:

- **Propietario**: el admin de Hécate. La pone en marcha, la opera desde la CLI y la retomará cuando un componente necesite
  la siguiente pieza.
- **Quien desarrolla otro componente**: por ejemplo, quien construya Hermes. Necesita implementar el lado del componente
  (darse de alta, renovar su certificado, autenticar cada llamada, resolver sus secretos y consultar el estado) sin leer
  las specs ni el código de Hécate.
- **Quien retoma el desarrollo de Hécate**: el propietario o el agente de desarrollo, meses después. Necesita compilarla,
  probarla y saber dónde está cada cosa.

### User Story 1 - Entender Hécate y llegar a cada concepto (Priority: P1)

Alguien que no conoce Hécate abre su documento de alto nivel y entiende en pocos minutos por qué existe, qué problema
resuelve, qué hace y qué no hace, y quién la usa. Cada concepto aparece introducido en un párrafo, con un enlace al
documento específico que lo desarrolla, y nada de lo que hoy explica el documento de diseño se ha perdido por el camino.

**Why this priority**: es la puerta de entrada a todo lo demás y el criterio de documentación que pide el propietario: un
documento a alto nivel y, desde él, documentos manejables y específicos.

**Independent Test**: leer solo el documento de alto nivel y responder por qué existe Hécate, qué resuelve, qué hace y qué
no, quién la usa y cómo se comporta ante fallos; seguir cada enlace y comprobar que lleva al documento que desarrolla ese
concepto; y comprobar que cada apartado del documento de diseño actual tiene su sitio en la estructura nueva.

**Acceptance Scenarios**:

1. **Given** el documento de alto nivel, **When** alguien lo lee sin abrir ningún otro, **Then** puede explicar por qué
   existe Hécate, qué resuelve (los secretos de terceros, la identidad de los componentes y la auditoría de Cerbero), qué
   hace y qué no hace, y quién la usa.
2. **Given** cada concepto (sellado, secretos, identidades, acceso, auditoría y seguridad) y cada uso (integración, CLI y
   configuración, operación y desarrollo), **When** se busca en el documento de alto nivel, **Then** tiene un párrafo que
   lo introduce y un enlace a su documento.
3. **Given** un documento específico, **When** se lee, **Then** desarrolla su concepto o su uso y, para lo que pertenece a
   otro, enlaza al documento que lo desarrolla en lugar de repetirlo.
4. **Given** cada apartado y cada dato del documento de diseño actual, **When** se busca en la estructura nueva, **Then**
   está en el documento de alto nivel o en uno específico.
5. **Given** la constitución, **When** alguien busca dónde vive el diseño vigente de un componente, **Then** encuentra que
   es su `docs/<componente>.md` y, si los tiene, sus documentos de detalle en `docs/<componente>/`.

---

### User Story 2 - Integrar un componente con Hécate (Priority: P2)

Quien desarrolla otro componente quiere implementar su lado de la integración con Hécate. Con la guía de integración y el
contrato de la API, sin abrir las specs ni el código de Hécate, sabe qué genera y guarda el componente, cómo se da de alta
con su token, cómo comprueba que habla con Hécate, cuándo y cómo renueva su certificado, cómo autentica cada llamada, qué
debe refrescar y cada cuánto, cómo resuelve y guarda en caché sus secretos, cómo consulta el estado y qué hace ante cada
error o con Hécate sellada.

**Why this priority**: el siguiente componente que se construya será el primero que use Hécate, y su cliente aún no
existe; sin esta guía habría que reconstruir el protocolo desde specs cerradas y desde el código en Rust.

**Independent Test**: con la guía y el contrato de la API, escribir la secuencia completa de peticiones desde que el
componente recibe su token de alta hasta que resuelve un secreto y renueva su certificado, con los datos que necesita cada
una; y comprobar cada dato de la guía contra el código y el contrato.

**Acceptance Scenarios**:

1. **Given** un componente con un token de alta, **When** su desarrollador sigue la guía, **Then** sabe qué claves genera
   y guarda, cómo comprueba la identidad de Hécate antes de enviar nada y qué recibe al darse de alta.
2. **Given** un componente dado de alta, **When** llega el momento de renovar, **Then** la guía dice cuándo renovar, cómo
   se hace, qué pasa si se repite la misma petición y qué pasa si se intenta renovar dos veces el mismo certificado.
3. **Given** cualquier llamada autenticada, **When** el componente la prepara, **Then** la guía dice cómo construye el
   token corto, cuánto puede durar, a qué queda ligado y qué valida Hécate.
4. **Given** las publicaciones de Hécate (la lista de revocación y las claves de firma de llamadas), **When** el
   componente las usa, **Then** la guía dice para qué sirven y cada cuánto refrescarlas.
5. **Given** una resolución de secretos, **When** el componente recibe la respuesta, **Then** la guía dice cuánto tiempo
   puede usar cada valor sin refrescarlo y hasta cuándo puede seguir usándolo si Hécate no responde.
6. **Given** cada respuesta de error y Hécate sellada, **When** el componente la recibe, **Then** la guía dice qué
   significa, si puede reintentar y qué hacer.
7. **Given** cada operación de la API, **When** se busca en la guía, **Then** aparece con su propósito, su autenticación y
   su enlace al esquema del contrato, sin duplicar el esquema.

---

### User Story 3 - Lanzar y manejar Hécate desde la CLI (Priority: P3)

El propietario quiere tener en un solo sitio cómo se lanza Hécate, con qué variables de entorno, y qué hace cada orden de
la CLI, con sus argumentos, su salida, sus errores y sus códigos de salida.

**Why this priority**: hoy las órdenes están repartidas en los contratos de cinco specs cerradas y las variables de
entorno no aparecen en ningún documento vigente; tras meses de pausa, no se podría operar Hécate sin leer el código.

**Independent Test**: comparar la referencia con el código: cada orden y cada variable de entorno que existe está en la
referencia, con su comportamiento, y la referencia no nombra ninguna que no exista.

**Acceptance Scenarios**:

1. **Given** el equipo de Hécate, **When** el propietario quiere arrancarla, **Then** la referencia dice cómo se lanza y
   qué variables de entorno admite, con su valor por defecto, su efecto y sus límites.
2. **Given** cada orden de la CLI, **When** se busca en la referencia, **Then** aparece con sus argumentos, su salida si va
   bien, sus errores y su código de salida.
3. **Given** una orden que pide datos sensibles (la frase, las partes, el valor de un secreto), **When** se lee su
   entrada, **Then** la referencia dice cómo se piden sin que pasen por la línea de órdenes.

---

### User Story 4 - Actuar ante cada situación de operación (Priority: P4)

El propietario quiere un procedimiento escrito, paso a paso, para poner Hécate en marcha por primera vez y para cada
situación de la tabla de operación: desbloquearla tras un reinicio, renovar la intermedia, sustituir la CA, responder a
una clave comprometida, recuperar una frase olvidada y el resto.

**Why this priority**: el documento de diseño dice que cada fila de su tabla de operación es un procedimiento escrito, y
no existe ninguno; son lo que más falta hará al retomar Hécate tras una pausa larga.

**Independent Test**: para cada situación de la tabla de operación y para la puesta en marcha, encontrar su procedimiento
y comprobar que cada paso usa una orden de la referencia de la CLI que existe.

**Acceptance Scenarios**:

1. **Given** un equipo sin Hécate, **When** el propietario sigue el procedimiento de puesta en marcha, **Then** la
   inicializa, crea la CA, la desbloquea y da de alta el primer componente con sus permisos.
2. **Given** cada situación de la tabla de operación, **When** se busca su procedimiento, **Then** existe, paso a paso,
   con las órdenes que usa.
3. **Given** una situación que hoy no tiene un procedimiento completo (p. ej. restaurar una copia, sin la política común
   de copias), **When** se lee su procedimiento, **Then** dice qué se puede hacer hoy y remite a DECISIONS para lo que
   falta.

---

### User Story 5 - Retomar el desarrollo de Hécate (Priority: P5)

Quien retoma el desarrollo de Hécate quiere compilarla, pasar la suite en macOS y en un contenedor Linux, pasar las
comprobaciones de calidad, usar el componente de prueba y saber cómo se organizan los crates, sin reconstruirlo desde las
specs.

**Why this priority**: es lo que se necesita el primer día al retomar; hoy `hecate/README.md` es una sola línea.

**Independent Test**: siguiendo solo `hecate/README.md`, compilar, ejecutar la suite en las dos plataformas y las
comprobaciones de calidad, y localizar cada crate.

**Acceptance Scenarios**:

1. **Given** un equipo con las herramientas que lista, **When** alguien sigue el README de Hécate, **Then** compila,
   ejecuta la suite en macOS y en el contenedor Linux y pasa las comprobaciones de calidad.
2. **Given** el componente de prueba, **When** se busca cómo usarlo, **Then** el README dice para qué sirve y cómo se
   ejecuta su ejemplo.

---

### User Story 6 - Saber qué está hecho y qué no al pausar (Priority: P6)

El propietario quiere que, mientras Hécate está pausada, se vea qué hace ya y qué piezas de su diseño están sin
construir, sin que los documentos de diseño se conviertan en un roadmap.

**Why this priority**: al elegir el siguiente componente, o al retomar Hécate, hay que saber qué pieza falta y a quién le
hace falta.

**Independent Test**: en DECISIONS, cada pieza diseñada y sin construir aparece entre las abiertas; en el README del
proyecto, el estado dice lo que Hécate ya hace.

**Acceptance Scenarios**:

1. **Given** DECISIONS, **When** se leen las abiertas, **Then** cada pieza de Hécate diseñada y sin construir aparece como
   diseñada y no implementada, sin repetir las que ya figuran.
2. **Given** el README del proyecto, **When** se lee su estado, **Then** dice lo que Hécate ya hace y enlaza al documento
   de alto nivel.
3. **Given** DECISIONS, **When** se busca cómo corre Hécate en el servidor de casa, **Then** aparece entre las abiertas
   como una decisión común a todos los componentes.

---

### Edge Cases

- Un dato del documento de diseño actual que no coincide con el código: el documento pasa a describir lo que hace el
  código. Si el código parece contradecir la spec fusionada que lo pidió, no se corrige aquí: se avisa al propietario.
- Una pieza diseñada y sin construir (p. ej. la firma de checkpoints o `wrap`/`unwrap`): los documentos de diseño la
  describen como diseño, sin decir nada de su estado; su estado vive solo en DECISIONS y en el README del proyecto.
- Las referencias de uso (integración, CLI y operación) describen solo lo que existe: una operación u orden diseñada y
  sin construir no aparece en ellas.
- Una situación de la tabla de operación que hoy no tiene un procedimiento completo (p. ej. restaurar una copia, sin la
  política común de copias): su procedimiento dice qué se puede hacer hoy y remite a DECISIONS para lo que falta.
- Un enlace de otro documento a `docs/hecate.md` sigue funcionando, porque el fichero se mantiene como documento de alto
  nivel; nadie enlaza hoy a un apartado concreto suyo.
- Las specs de cambios ya fusionados no se modifican: siguen siendo el registro de su fecha, aunque la documentación
  vigente pase a otros ficheros.
- Un concepto que toca varios documentos (p. ej. el borrado de los secretos resueltos de la memoria, que atañe al acceso,
  a la seguridad y a la integración) se desarrolla en uno y los demás lo enlazan.
- Las decisiones abiertas que ya están en DECISIONS (el adapter de hardware, la rotación automática de secretos, el
  anclaje externo de los checkpoints, la observabilidad, la política de copias) no se duplican.
- Ejecutar Hécate como servicio en el servidor de casa: no se describe. La referencia da lo que necesita para arrancar, y
  cómo corren los componentes en el home lab queda abierto en DECISIONS (FR-019).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Este cambio NO DEBE añadir ni modificar código, configuración de compilación ni manifiestos de dependencias;
  solo documentos: `docs/hecate.md` y `docs/hecate/`, `hecate/README.md`, el README del proyecto, DECISIONS, la
  constitución, los enlaces de otros documentos que apuntan a Hécate y esta carpeta de `specs/`.
- **FR-002**: `docs/hecate.md` DEBE ser el documento de alto nivel: por qué existe Hécate y qué resuelve; qué hace y qué no
  hace; quién la usa (los componentes, el admin y Ténaro); un párrafo que introduce cada concepto (sellado, secretos,
  identidades, acceso, auditoría y seguridad) y cada uso (integración, CLI y configuración, operación y desarrollo), con
  el enlace a su documento; su comportamiento ante fallos (principio VI de la constitución); y sus decisiones y riesgos, con enlaces a
  los ADR y a DECISIONS.
- **FR-003**: Cada concepto y cada uso de FR-002 DEBE tener un documento específico que lo desarrolla: los conceptos y los
  usos de integración, CLI y configuración y operación en `docs/hecate/`, y el desarrollo en `hecate/README.md`. Un
  documento específico NO DEBE repetir lo que desarrolla otro: lo enlaza.
- **FR-004**: Ningún apartado ni dato del documento de diseño actual DEBE perderse: cada uno DEBE quedar en el documento de
  alto nivel o en uno específico. Un dato que no coincide con el código DEBE pasar a describir lo que hace el código.
- **FR-005**: La guía de integración DEBE permitir a quien desarrolla otro componente implementar su lado sin leer las
  specs ni el código de Hécate. DEBE recoger:
  - el flujo completo desde que el componente recibe su token de alta: qué claves genera y guarda, cómo comprueba la
    identidad de Hécate antes de enviar nada y qué recibe al darse de alta;
  - la renovación: cuándo renovar, cómo, qué pasa al repetir la misma petición y qué pasa en un segundo intento con el
    mismo certificado;
  - el token corto de cada llamada: cómo se construye y se firma, cuánto puede durar, a qué queda ligado y qué valida
    Hécate;
  - la lista de revocación y las claves de firma de llamadas publicadas: para qué sirven y cada cuánto se refrescan;
  - la resolución de secretos y su caché (tiempo blando y tiempo duro), y la consulta del estado con su permiso;
  - el sobre común, cada código de error con su estado HTTP y si se puede reintentar, el comportamiento con Hécate sellada
    y los límites de las peticiones;
  - cada operación de la API con su propósito y su autenticación, remitiendo al contrato de `shared/contracts` para los
    esquemas, sin duplicarlos.

  La guía NO DEBE llevar código en ningún lenguaje: da los formatos exactos y las secuencias con diagramas, y remite al
  componente de prueba de Hécate como cliente de referencia completo y probado.
- **FR-006**: La referencia de la CLI y la configuración DEBE recoger cómo se lanza Hécate, lo que necesita del sistema
  para arrancar y protegerse (p. ej. la memoria bloqueada y los permisos del directorio de datos y del socket) y cada
  variable de entorno, con su valor por defecto, su efecto y sus límites, y cada orden de la CLI con sus argumentos, su salida si va bien, sus
  errores y su código de salida, incluido cómo pide los datos sensibles sin que pasen por la línea de órdenes. DEBE cubrir
  todas las órdenes y variables que existen y ninguna que no exista.
- **FR-007**: El documento de operación DEBE tener un procedimiento escrito, paso a paso y con las órdenes de FR-006, para
  la puesta en marcha (inicializar, crear la CA, desbloquear, dar de alta el primer componente y conceder sus permisos) y
  para cada situación de la tabla de operación. Una situación que hoy no tenga un procedimiento completo DEBE decir qué se
  puede hacer hoy y remitir a DECISIONS para lo que falta. NO DEBE describir ningún gestor de servicios: cómo corre Hécate
  en el home lab remite a la decisión abierta de FR-019.
- **FR-008**: `hecate/README.md` DEBE explicar cómo se desarrolla Hécate: qué herramientas necesita, cómo se compila, cómo
  se ejecuta la suite en macOS y en un contenedor Linux con los ajustes que necesita, las comprobaciones de calidad, para
  qué sirve el componente de prueba y cómo se ejecuta su ejemplo, y cómo se organizan los crates; y DEBE enlazar al
  documento de alto nivel.
- **FR-009**: DECISIONS DEBE recoger entre las abiertas, como diseñada y no implementada, cada pieza del diseño de Hécate
  sin construir: la firma de checkpoints (con su clave de firma de eventos y la verificación periódica), `wrap`/`unwrap`,
  las identidades hijas, los permisos que salen de las declaraciones, la rotación de las claves de ámbito y la gestión y el
  desbloqueo desde Ténaro. NO DEBE duplicar las que ya figuran.
- **FR-010**: El README del proyecto DEBE recoger en su estado lo que Hécate ya hace (sellado, secretos, identidades,
  Access Policy y resolución por la API) y enlazar al documento de alto nivel.
- **FR-011**: La constitución DEBE aclarar que el diseño vigente de un componente vive en su `docs/<componente>.md` y, si
  los tiene, en sus documentos de detalle en `docs/<componente>/`, y que la consolidación de un cambio va a cualquiera de
  ellos. La enmienda DEBE ser una subida PATCH con qué cambia, por qué, qué alternativas se descartaron y a qué afecta en
  su historial.
- **FR-012**: Cada dato de los documentos de este cambio DEBE coincidir con el código y el contrato de la API vigentes.
- **FR-013**: Los documentos DEBEN describir lo que hay, sin futuro ni roadmap. Las referencias de uso (integración, CLI y
  operación) DEBEN describir solo lo que existe. Los documentos de diseño describen el diseño sin decir nada de su estado:
  lo que no está construido figura solo en DECISIONS (FR-009), y lo que Hécate ya hace, en el README del proyecto
  (FR-010).
- **FR-014**: Ningún documento de este cambio DEBE contradecir a otro ni a ARCHITECTURE, GLOSSARY o DECISIONS. Los enlaces
  de otros documentos que remiten a un detalle de Hécate DEBEN llevar al documento específico que lo desarrolla.
- **FR-015**: Todo enlace de los documentos de este cambio DEBE llevar a un fichero o apartado que existe.
- **FR-016**: Los documentos de los demás componentes DEBEN mantener su plantilla; este cambio solo toca de ellos los
  enlaces a Hécate que haga falta.
- **FR-017**: Las specs de cambios ya fusionados NO DEBEN modificarse.
- **FR-018**: El documento de seguridad DEBE recoger frente a qué protege Hécate, qué mecanismo cubre cada amenaza
  (enlazando al documento del concepto que lo desarrolla) y los riesgos aceptados (enlazando a DECISIONS), sin repetir el
  desarrollo de cada mecanismo.
- **FR-019**: DECISIONS DEBE recoger entre las abiertas cómo corren los componentes en el home lab (gestor de servicios o
  contenedores, usuarios y arranque), una decisión común a todos los componentes.

### Key Entities *(include if feature involves data)*

- **Documento de alto nivel**: `docs/hecate.md`; la entrada a Hécate: por qué existe, qué hace, quién la usa y un párrafo
  por concepto y por uso con su enlace.
- **Documento específico**: desarrolla un concepto (sellado, secretos, identidades, acceso, auditoría, seguridad) o un
  uso (integración, CLI y configuración, operación, desarrollo); enlaza a los demás en lugar de repetirlos.
- **Contrato de la API**: el OpenAPI de Hécate en `shared/contracts`; la fuente de los esquemas exactos.
- **DECISIONS**: las decisiones abiertas, entre ellas las piezas de Hécate diseñadas y sin construir.
- **Constitución**: fija dónde vive el diseño vigente de un componente; tiene una versión semver y un historial de
  enmiendas.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Alguien que no conoce Hécate, leyendo solo el documento de alto nivel durante menos de 10 minutos, explica
  por qué existe, qué resuelve, qué hace y qué no, y quién la usa, y llega desde él a cada concepto en un solo paso.
- **SC-002**: El 100 % de los apartados y datos del documento de diseño actual tiene su sitio en la estructura nueva.
- **SC-003**: El 100 % de las órdenes de la CLI y de las variables de entorno que existen aparece en la referencia de la
  CLI, con su comportamiento, y la referencia no nombra ninguna que no exista.
- **SC-004**: El 100 % de las operaciones del contrato de la API aparece en la guía de integración, con su autenticación,
  sus errores y su comportamiento con Hécate sellada, y la guía no nombra ninguna que no exista.
- **SC-005**: Con la guía de integración y el contrato de la API, sin abrir las specs ni el código, se puede escribir la
  secuencia completa de peticiones desde el token de alta hasta resolver un secreto y renovar el certificado.
- **SC-006**: El 100 % de las situaciones de la tabla de operación y la puesta en marcha tienen su procedimiento escrito.
- **SC-007**: El 100 % de las piezas de Hécate diseñadas y sin construir figura en DECISIONS, y el README del proyecto dice
  lo que Hécate ya hace.
- **SC-008**: Siguiendo solo el README de Hécate, quien retoma su desarrollo compila, pasa la suite en macOS y en el
  contenedor Linux y pasa las comprobaciones de calidad.
- **SC-009**: Una lectura completa no encuentra contradicciones entre los documentos de este cambio, ni con ARCHITECTURE,
  GLOSSARY o DECISIONS, y el 100 % de sus enlaces lleva a un sitio que existe.
- **SC-010**: Al cerrar la rama, ningún fichero de código, configuración de compilación o manifiesto de dependencias
  difiere de `main`.

## Assumptions

- **Base**: `main` tras la feature 006 (`d589bf6`). Hécate queda pausada; los documentos describen lo fusionado.
- **Nombres concretos**: los ficheros, la CLI, el OpenAPI y las piezas de Hécate aparecen porque son el contenido de la
  documentación, no una elección de implementación de este cambio.
- **Nombres y ubicación**: el plan decide los nombres de los documentos específicos dentro de `docs/hecate/`; la spec fija
  qué conceptos y usos tienen documento.
- **Contrato de la API**: el OpenAPI de `shared/contracts` es la fuente de los esquemas; la guía de integración explica lo
  que el contrato no dice (la autenticación, los tiempos, la caché, el comportamiento sellada) y remite a él para el resto.
- **Cliente de referencia**: el componente de prueba de Hécate (su ayudante de pruebas y su ejemplo `component`) habla
  con la API como lo haría un componente real y lo prueban las suites de Hécate; por eso la guía remite a él en lugar de
  incluir código (FR-005).
- **Canal local**: su protocolo no se documenta aparte; su único cliente es la CLI, cuya referencia cubre lo que necesita
  el admin.
- **Plantilla de los demás componentes**: sus documentos mantienen los 14 apartados hasta que se construyan; adoptar la
  estructura nueva se decidirá entonces.
- **Constitución**: la aclaración va por `/speckit-constitution` dentro de este cambio, como la enmienda del cambio 004.
- **Sin código**: si al contrastar aparece una diferencia entre el código y una spec fusionada, no se corrige aquí; se
  avisa al propietario.
- **Fuera de alcance**: cambiar el código o el comportamiento de Hécate, documentar aparte el protocolo del canal local,
  modificar las specs de cambios ya fusionados y reestructurar los documentos de otros componentes.
- **Rama**: `docs/hecate-reference`, creada desde `main`; commit de squash `docs:`.
