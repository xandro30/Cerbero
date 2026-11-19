# Feature Specification: Hermes documentado en un documento de alto nivel y documentos específicos

**Feature Branch**: `docs/hermes-reference`

**Created**: 2026-11-16

**Status**: Draft

**Input**: User description: "SPECIFY_FEATURE_DIRECTORY=specs/010-hermes-reference. Reorganizar la documentación de
Hermes en un documento de alto nivel y documentos específicos, como hizo la 007 con Hécate. Hermes tiene construidas la
feature 008 (su base y chat.send_message con Telegram) y la comprobación del acceso por app de la 009, pero su
documentación vigente es un solo documento, docs/hermes.md, con la plantilla de 14 apartados y unas 500 líneas, y lo
necesario para usarlo está repartido en specs ya cerradas: las órdenes de la CLI con sus salidas y códigos de salida, el
contrato de la API para quien lo llama y los procedimientos de operación. hermes/README.md tiene tres líneas. El
propietario decidió en la 008 consolidar entonces en docs/hermes.md y hacer la reestructuración en un cambio propio, y
en la 009 que fuera la 010. Este cambio reorganiza la documentación. docs/hermes.md explica por qué existe Hermes, qué
resuelve, qué hace y qué no, quién lo usa y cómo encaja, y presenta cada concepto en un párrafo que enlaza a su
documento en docs/hermes/: contratos (la taxonomía de tipos de servicio, las tres capas de datos, el sobre común, el
contrato de una función y chat.send_message); proveedores (las librerías, cómo se añade una, su manifiesto, la batería
de conformidad del tipo de servicio y el proveedor fake); configuración (instancias, plantillas, variables, contactos y
la verificación de canales); seguridad (la autenticación tras Envoy, el acceso por app, los secretos, la auditoría, los
datos personales y los logs); integración, para quien llama a Hermes (el certificado y el token corto, la
Idempotency-Key, los errores y si se reintenta, las capacidades y la prueba de conexión), remitiendo al OpenAPI de
shared/contracts y sin código; CLI y configuración (cómo se lanza, las variables de entorno y cada orden de hermes con
sus argumentos, salidas y códigos de salida); y operación (la puesta en marcha con Hécate, Envoy y el PostgreSQL común,
la salud y un procedimiento para cada situación: conceder el acceso a quien llama, verificar un canal, rotar el token de
un bot, Hécate sellada o caída, PostgreSQL caído y la identidad caducada o revocada). hermes/README.md pasa a explicar
cómo se desarrolla Hermes: las herramientas, la suite con HECATE_BIN y Docker, el E2E, las comprobaciones de calidad,
los ayudantes de pruebas y la organización del paquete. Lo diseñado y sin construir (las tasks, chat.ask, los grupos de
contactos y el cifrado de las copias con wrap/unwrap) se describe como diseño, sin decir que falta: el estado de lo
construido va solo en DECISIONS y en el README, como en la 007. Nada se pierde: todo lo que dice hoy docs/hermes.md
queda en algún documento nuevo o se justifica por qué sobra, y los enlaces de otros documentos a apartados de
docs/hermes.md se actualizan. No cambia código ni contratos. La constitución 1.2.1 ya admite documentos de detalle en
docs/<componente>/. Rama docs/hermes-reference; commit de squash docs:."

## Clarifications

### Session 2026-11-16

- Q: ¿Cómo se reparte el diseño de Hermes en los documentos de `docs/hermes/`? → A: En nueve documentos: además de
  contratos, proveedores, configuración, seguridad, integración, CLI y operación, uno de llamadas (cómo se atiende una
  llamada: síncrona o como task, idempotencia, reintentos seguros, resultado desconocido y timeouts) y otro de
  comunicación (contactos y canales, plantillas y sus variables, mensaje neutral, verificación de canales y `chat.ask`);
  configuración queda para el fichero, su aplicación y las instancias.
- Q: ¿Cómo se llaman los dos documentos que llevarían «configuración» en el nombre? → A: La referencia de la CLI pasa a
  llamarse «CLI y variables de entorno», y «configuración» queda solo para la configuración de Hermes (el fichero, su
  aplicación y las instancias). Así tampoco se confunden las variables de entorno con las variables de plantilla.
- Q: ¿Cómo se escriben los procedimientos de operación, si cómo corre Hermes en el home lab sigue abierto y el único
  entorno completo es el compose de pruebas de `hermes/deploy/`? → A: Cada paso usa las órdenes de `hermes` y de
  `hecate`, que valen en cualquier despliegue, y un apartado explica cómo se ejecutan en el compose de pruebas, sin
  presentarlo como el despliegue de casa.

## User Scenarios & Testing *(mandatory)*

Hermes tiene construida su base y su primera función: la API detrás de Envoy, la identidad y los secretos de Hécate, la
configuración en su base aplicada con su CLI, la verificación de canales y `chat.send_message` con Telegram (feature
008), y la comprobación del acceso por app (feature 009). Su documentación vigente es un solo documento largo con la
plantilla de 14 apartados, y lo necesario para usarlo (las órdenes de la CLI, el contrato para quien lo llama y los
procedimientos de operación) está repartido en specs ya cerradas, que no se actualizan.

Este cambio aplica a Hermes la estructura que la 007 aplicó a Hécate: un documento de alto nivel, que cuenta por qué
existe Hermes y qué hace, y documentos específicos, más pequeños, que desarrollan cada concepto y cada uso. No cambia
código, comportamiento ni contratos: implementar aquí es escribir, reorganizar y contrastar documentación.

En esta spec, «documento de alto nivel» es `docs/hermes.md`; «documento específico» es el que desarrolla un concepto
(contratos, llamadas, comunicación, configuración, proveedores, seguridad) o un uso (integración, CLI y variables de
entorno, operación, desarrollo); y «dato» es todo lo que se puede comprobar contra el código o los contratos publicados:
una orden, un argumento, una salida, un código de salida, una variable de entorno y su valor por defecto, una operación,
un campo, un código de error, un estado HTTP, un límite o un tiempo.

Actores:

- **Propietario**: el admin de Hermes. Lo pone en marcha con Hécate, Envoy y el PostgreSQL común, lo configura y lo
  opera con su CLI.
- **Quien desarrolla un componente que llama a Hermes**: hoy, el cliente de referencia de las pruebas; en producción,
  Hades. Necesita llamar a Hermes sin leer sus specs ni su código.
- **Quien desarrolla una librería de proveedor**: añade un proveedor a un tipo de servicio, como Telegram al de `chat`.
- **Quien retoma el desarrollo de Hermes**: el propietario o el agente de desarrollo. Necesita pasar la suite, las
  comprobaciones de calidad y saber dónde está cada cosa.

### User Story 1 - Entender Hermes y llegar a cada concepto (Priority: P1)

Alguien que no conoce Hermes abre su documento de alto nivel y entiende en pocos minutos por qué existe, qué problema
resuelve, qué hace y qué no hace, quién lo usa y cómo encaja con el resto de Cerbero. Cada concepto y cada uso aparecen
introducidos en un párrafo, con un enlace al documento específico que los desarrolla, y nada de lo que hoy explica
`docs/hermes.md` se ha perdido por el camino.

**Why this priority**: es la puerta de entrada a todo lo demás y el criterio de documentación del propietario: un
documento a alto nivel y, desde él, documentos manejables y específicos.

**Independent Test**: leer solo el documento de alto nivel y responder por qué existe Hermes, qué resuelve, qué hace y
qué no, quién lo usa y cómo se comporta ante fallos; seguir cada enlace y comprobar que lleva al documento que
desarrolla ese concepto o uso; y comprobar que cada apartado y cada dato del documento actual tiene su sitio en la
estructura nueva.

**Acceptance Scenarios**:

1. **Given** el documento de alto nivel, **When** alguien lo lee sin abrir ningún otro, **Then** puede explicar por qué
   existe Hermes, qué resuelve (los servicios de terceros detrás de contratos estándar por tipo de servicio), qué hace y
   qué no hace, quién lo usa y cómo encaja con Hades, Hécate, Envoy y el PostgreSQL común.
2. **Given** cada concepto (contratos, llamadas, comunicación, configuración, proveedores y seguridad) y cada uso
   (integración, CLI y variables de entorno, operación y desarrollo), **When** se busca en el documento de alto nivel,
   **Then** tiene un párrafo que lo introduce y un enlace a su documento.
3. **Given** un documento específico, **When** se lee, **Then** desarrolla su concepto o su uso y, para lo que pertenece
   a otro, enlaza al documento que lo desarrolla en lugar de repetirlo.
4. **Given** cada apartado y cada dato de `docs/hermes.md` antes de este cambio, **When** se busca en la estructura
   nueva, **Then** está en el documento de alto nivel o en uno específico, o la revisión justifica por qué sobra.
5. **Given** DECISIONS y el README del proyecto, **When** alguien busca qué hace ya Hermes y qué piezas de su diseño
   están sin construir, **Then** lo encuentra allí, y en ningún documento de diseño.

---

### User Story 2 - Integrar un componente que llama a Hermes (Priority: P2)

Quien desarrolla un componente que llama a Hermes quiere implementar su lado. Con la guía de integración y el contrato
de la API, sin abrir las specs ni el código de Hermes, sabe qué necesita para llamarlo (su identidad de Hécate y el
acceso que le concede el admin), cómo autentica cada llamada a través de Envoy, cómo usa la `Idempotency-Key`, qué
significa cada error y si puede reintentar, y cómo consulta las capacidades y prueba una instancia.

**Why this priority**: Hades será el primer componente de producción que llame a Hermes, y su cliente aún no existe; sin
esta guía habría que reconstruir el protocolo desde specs cerradas y desde el código.

**Independent Test**: con la guía y el contrato de la API, escribir la secuencia completa de un componente desde que el
admin le concede el acceso hasta enviar un mensaje, repetirlo sin duplicarlo y actuar ante cada error, con los datos que
necesita cada paso; y comprobar cada dato de la guía contra el código y el contrato.

**Acceptance Scenarios**:

1. **Given** un componente con su identidad de Hécate, **When** su desarrollador sigue la guía, **Then** sabe que
   necesita que el admin le conceda el acceso a Hermes en Hécate, a qué dirección llama y qué presenta en cada conexión.
2. **Given** cualquier llamada, **When** el componente la prepara, **Then** la guía dice cómo construye el token corto,
   para qué audiencia, cuánto puede durar y qué contexto de auditoría puede llevar, y que lo que diga el token no
   concede nada.
3. **Given** una llamada con efecto, **When** el componente la envía o la repite, **Then** la guía dice cómo es la
   `Idempotency-Key`, qué devuelve Hermes al repetir la misma petición o al reutilizar la clave con otra, y qué
   significa un resultado desconocido.
4. **Given** cada respuesta de error, **When** el componente la recibe, **Then** la guía dice qué significa, su estado
   HTTP, si puede reintentar y qué hacer, también cuando Hermes no está listo o Hécate no responde.
5. **Given** cada operación de la API, **When** se busca en la guía, **Then** aparece con su propósito, su autenticación
   y su enlace al esquema del contrato, sin duplicar el esquema.
6. **Given** la guía, **When** se busca un cliente de ejemplo, **Then** remite al cliente de referencia de las pruebas,
   completo y probado, en lugar de llevar código.

---

### User Story 3 - Configurar y manejar Hermes desde la CLI (Priority: P3)

El propietario quiere tener en un solo sitio cómo se lanza Hermes, con qué variables de entorno, qué hace cada orden de
su CLI, con sus argumentos, su salida, sus errores y sus códigos de salida, y cómo es el fichero de la configuración que
aplica.

**Why this priority**: hoy el detalle de las órdenes, de sus salidas y del fichero de la configuración está en los
contratos de la 008, una spec cerrada; sin él no se puede operar Hermes sin leer el código.

**Independent Test**: comparar la referencia con el código: cada orden y cada variable de entorno que existe está en la
referencia, con su comportamiento, y la referencia no nombra ninguna que no exista.

**Acceptance Scenarios**:

1. **Given** el equipo de Hermes, **When** el propietario quiere arrancarlo, **Then** la referencia dice cómo se lanza,
   qué necesita (los directorios, los sockets y Envoy delante) y qué variables de entorno admite, con su valor por
   defecto, su efecto y sus límites.
2. **Given** cada orden de la CLI, **When** se busca en la referencia, **Then** aparece con sus argumentos, su salida si
   va bien, sus errores y su código de salida.
3. **Given** el fichero de la configuración, **When** el propietario lo escribe, **Then** encuentra su formato, sus
   reglas y cómo se planifica, se aplica y se exporta.
4. **Given** la orden que pide un dato sensible (el token de alta), **When** se lee su entrada, **Then** la referencia
   dice cómo se pide sin que pase por la línea de órdenes.

---

### User Story 4 - Actuar ante cada situación de operación (Priority: P4)

El propietario quiere un procedimiento escrito, paso a paso, para poner Hermes en marcha y para cada situación de su
operación: conceder o retirar el acceso a quien llama, verificar un canal, rotar el token de un bot, Hécate sellada o
caída, PostgreSQL caído, una instancia que falla y la identidad de Hermes caducada, revocada o de otra CA.

**Why this priority**: la tabla de operación de hoy dice qué pasa en cada caso, pero no qué hacer paso a paso; es lo que
más falta hará al operar Hermes en serio.

**Independent Test**: para la puesta en marcha y para cada situación de la operación, encontrar su procedimiento y
comprobar que cada paso usa una orden de la referencia de la CLI de Hermes o de la de Hécate que existe.

**Acceptance Scenarios**:

1. **Given** un equipo con Hécate desbloqueada y el PostgreSQL común, **When** el propietario sigue el procedimiento de
   puesta en marcha, **Then** prepara en Hécate los secretos y el token de Hermes, lo da de alta, aplica la
   configuración, verifica un canal y concede el acceso al primer componente que lo llama.
2. **Given** cada situación de la operación, **When** se busca su procedimiento, **Then** existe, paso a paso, con las
   órdenes que usa y cómo se comprueba que ha funcionado.
3. **Given** una situación que hoy no tiene un procedimiento completo (p. ej. restaurar una copia, sin la política común
   de copias), **When** se lee su procedimiento, **Then** dice qué se puede hacer hoy y remite a DECISIONS para lo que
   falta.

---

### User Story 5 - Añadir una librería de proveedor (Priority: P5)

Quien desarrolla una librería quiere saber qué tiene que hacer para añadir un proveedor a un tipo de servicio: qué es
una librería, de qué partes consta, qué declara su manifiesto, qué batería de conformidad tiene que pasar y cómo se
prueba con el proveedor fake.

**Why this priority**: Telegram es hoy el único proveedor real, y cada librería nueva tiene que seguir las mismas reglas
para que los Playbooks no dependan del proveedor.

**Independent Test**: con el documento de proveedores, enumerar lo que tiene que contener una librería nueva de `chat` y
lo que tiene que pasar, y comprobar cada dato contra la librería de Telegram, la del fake y la batería de conformidad.

**Acceptance Scenarios**:

1. **Given** el documento de proveedores, **When** alguien prepara una librería, **Then** sabe qué partes tiene y qué
   declara cada campo de su manifiesto.
2. **Given** un tipo de servicio, **When** se añade un proveedor, **Then** el documento dice qué batería de conformidad
   tiene que pasar y cómo se usa el proveedor fake en las pruebas.

---

### User Story 6 - Retomar el desarrollo de Hermes (Priority: P6)

Quien retoma el desarrollo de Hermes quiere pasar su suite, con la Hécate real y Docker, el E2E y las comprobaciones de
calidad, usar los ayudantes de pruebas y saber cómo se organiza el paquete, sin reconstruirlo desde las specs.

**Why this priority**: es lo que se necesita el primer día al retomar; hoy `hermes/README.md` son tres líneas.

**Independent Test**: siguiendo solo `hermes/README.md`, ejecutar la suite con la Hécate real y Docker, el E2E y las
comprobaciones de calidad, y localizar cada capa del paquete.

**Acceptance Scenarios**:

1. **Given** un equipo con las herramientas que lista, **When** alguien sigue el README de Hermes, **Then** ejecuta la
   suite, con la Hécate real y Docker, y pasa las comprobaciones de calidad.
2. **Given** los ayudantes de pruebas (el cliente de referencia, la Hécate de pruebas y los dobles), **When** se busca
   para qué sirve cada uno, **Then** el README lo dice.

---

### Edge Cases

- Un dato del documento actual que no coincide con el código: el documento pasa a describir lo que hace el código. Si el
  código parece contradecir la spec fusionada que lo pidió, no se corrige aquí: se avisa al propietario.
- Una pieza diseñada y sin construir (las tasks, `chat.ask`, los grupos de contactos, el cifrado de las copias): los
  documentos de diseño la describen como diseño, sin decir nada de su estado; su estado vive solo en DECISIONS y en el
  README del proyecto.
- Las referencias de uso (integración, CLI y operación) describen solo lo que existe: una operación, orden o ruta
  diseñada y sin construir, como las de las tasks, no aparece en ellas.
- Los enlaces de DECISIONS a apartados de `docs/hermes.md` (las tasks, la comunicación y la configuración) dejan de
  tener destino: pasan a llevar al documento específico que los desarrolla.
- Un enlace de otro documento a `docs/hermes.md` sin apartado sigue funcionando, porque el fichero se mantiene como
  documento de alto nivel.
- Un concepto que toca varios documentos (p. ej. la `Idempotency-Key`, que atañe a las llamadas, a quien llama y a la
  auditoría) se desarrolla en uno y los demás lo enlazan.
- Envoy no es parte de Hermes, pero Hermes depende de lo que hace: la documentación dice qué tiene que hacer el proxy y
  remite a la configuración de pruebas de `hermes/deploy/` como referencia, sin fijar cómo corre en el home lab.
- Ejecutar Hermes en el servidor de casa: no se describe. El compose de `hermes/deploy/` es el entorno de pruebas, y el
  documento de operación explica cómo se ejecuta en él cada orden; cómo corren los componentes en el home lab sigue
  abierto en DECISIONS.
- Ningún documento lleva un secreto, un token real ni la dirección de una persona: los ejemplos usan valores de muestra.
- Las specs de cambios ya fusionados no se modifican: siguen siendo el registro de su fecha, aunque la documentación
  vigente pase a otros ficheros.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Este cambio NO DEBE añadir ni modificar código, configuración, contratos publicados ni manifiestos de
  dependencias; solo documentos: `docs/hermes.md` y `docs/hermes/`, `hermes/README.md`, el README del proyecto,
  DECISIONS, los enlaces de otros documentos que apuntan a Hermes y esta carpeta de `specs/`.
- **FR-002**: `docs/hermes.md` DEBE ser el documento de alto nivel: por qué existe Hermes y qué resuelve; qué hace y qué
  no hace; quién lo usa y cómo encaja (los componentes a través de Envoy, el admin, Hécate, el PostgreSQL común y los
  proveedores); un párrafo que introduce cada concepto (contratos, llamadas, comunicación, configuración, proveedores y
  seguridad) y cada uso (integración, CLI y variables de entorno, operación y desarrollo), con el enlace a su documento;
  su comportamiento ante fallos (principio VI de la constitución); y sus decisiones y riesgos, con enlaces a los ADR y a
  DECISIONS.
- **FR-003**: Cada concepto y cada uso de FR-002 DEBE tener un documento específico que lo desarrolla: los conceptos y
  los usos de integración, CLI y variables de entorno, y operación en `docs/hermes/`, y el desarrollo en
  `hermes/README.md`. Un documento específico NO DEBE repetir lo que desarrolla otro: lo enlaza.
- **FR-004**: Ningún apartado ni dato de `docs/hermes.md` antes de este cambio DEBE perderse: cada uno DEBE quedar en el
  documento de alto nivel o en uno específico, o la revisión DEBE justificar por qué sobra. Un dato que no coincide con
  el código DEBE pasar a describir lo que hace el código.
- **FR-005**: El documento de contratos DEBE recoger la taxonomía (categoría, tipo de servicio, función, proveedor e
  instancia), las tres capas de datos, el sobre común con sus códigos de error y sus estados HTTP, el contrato de una
  función y `chat.send_message` con el orden de sus comprobaciones. El documento de llamadas DEBE recoger cómo se
  atiende una llamada: síncrona o como task, con su idempotencia, sus reintentos seguros, el resultado desconocido y sus
  timeouts.
- **FR-006**: La guía de integración DEBE permitir a quien desarrolla un componente implementar su lado sin leer las
  specs ni el código de Hermes. DEBE recoger:
  - lo que necesita antes de la primera llamada: su identidad de Hécate y el acceso a Hermes que le concede el admin;
  - cada llamada: la conexión a través de Envoy con su certificado, y el token corto (cómo se construye, su audiencia,
    su vida, su `jti` y el contexto de auditoría que puede llevar), con lo que comprueba Hermes y en qué orden;
  - la `Idempotency-Key`: su formato, qué devuelve Hermes al repetir la misma petición, al reutilizarla con otra y tras
    un corte;
  - el sobre común, cada código de error con su estado HTTP, si se puede reintentar y qué hacer, el comportamiento
    cuando Hermes no está listo o Hécate no responde, y los límites de las peticiones;
  - cada operación de la API con su propósito y su autenticación, remitiendo al contrato de `shared/contracts` para los
    esquemas, sin duplicarlos.

  La guía NO DEBE llevar código en ningún lenguaje: da los formatos exactos y las secuencias con diagramas, y remite al
  cliente de referencia de las pruebas de Hermes como cliente completo y probado.
- **FR-007**: La referencia de la CLI y las variables de entorno DEBE recoger cómo se lanza Hermes y lo que necesita
  para arrancar (los directorios con sus permisos, los dos sockets y Envoy delante), cada variable de entorno con su
  valor por defecto, su efecto y sus límites, y cada orden de la CLI con sus argumentos, su salida si va bien, sus
  errores y su código de salida, incluido cómo pide el token de alta sin que pase por la línea de órdenes. DEBE cubrir
  todas las órdenes y variables que existen y ninguna que no exista.
- **FR-008**: El documento de configuración DEBE recoger las instancias, el formato y las reglas del fichero de la
  configuración, y cómo se planifica, se aplica y se exporta. El documento de comunicación DEBE recoger los contactos y
  sus canales, las plantillas con sus versiones y sus variables, el modelo neutral del mensaje y cómo lo dibuja cada
  proveedor, la verificación de un canal con su código y `chat.ask`.
- **FR-009**: El documento de proveedores DEBE recoger qué es una librería y qué tipos de servicio implementa, de qué
  partes consta, cada campo de su manifiesto, su versión, la batería de conformidad de su tipo de servicio y el
  proveedor fake, con cuándo se carga.
- **FR-010**: El documento de seguridad DEBE recoger frente a qué protege Hermes, qué mecanismo cubre cada amenaza (la
  autenticación tras Envoy, el acceso por app, los secretos de Hécate, la auditoría, los datos personales y los logs),
  enlazando al documento que lo desarrolla, qué datos guarda y dónde, y los riesgos aceptados, enlazando a DECISIONS.
- **FR-011**: El documento de operación DEBE tener un procedimiento escrito, paso a paso y con las órdenes de FR-007 y
  de la CLI de Hécate, para la puesta en marcha (los secretos y el token de Hermes en Hécate, la base en el PostgreSQL
  común, Envoy, el alta, la configuración, la verificación de un canal y el acceso del primer componente) y para cada
  situación de la operación: conceder y retirar el acceso a quien llama, verificar un canal, rotar el token de un bot,
  Hécate sellada o caída, PostgreSQL caído, una instancia que falla y la identidad de Hermes caducada, revocada o de
  otra CA. Una situación que hoy no tenga un procedimiento completo DEBE decir qué se puede hacer hoy y remitir a
  DECISIONS para lo que falta. Cada paso DEBE usar las órdenes de `hermes` y de `hecate`, y un apartado DEBE explicar
  cómo se ejecutan en el compose de pruebas de `hermes/deploy/`, sin presentarlo como el despliegue del home lab. NO
  DEBE describir cómo corre Hermes en el home lab, que sigue abierto en DECISIONS.
- **FR-012**: `hermes/README.md` DEBE explicar cómo se desarrolla Hermes: qué herramientas necesita, cómo se ejecuta la
  suite (con la Hécate real y Docker, y el E2E), las comprobaciones de calidad, para qué sirve cada ayudante de pruebas
  y cómo se organiza el paquete; y DEBE enlazar al documento de alto nivel.
- **FR-013**: Los documentos de diseño DEBEN describir el diseño sin decir nada de su estado, y las referencias de uso
  (integración, CLI y operación) DEBEN describir solo lo que existe. Lo que Hermes ya hace DEBE estar en el README del
  proyecto, y cada pieza de su diseño sin construir, entre las abiertas de DECISIONS, sin duplicar las que ya figuran.
- **FR-014**: Cada dato de los documentos de este cambio DEBE coincidir con el código y los contratos publicados
  vigentes.
- **FR-015**: Los documentos DEBEN describir lo que hay, sin futuro ni roadmap.
- **FR-016**: Ningún documento de este cambio DEBE contradecir a otro ni a ARCHITECTURE, GLOSSARY, DECISIONS o la
  documentación de Hécate. Los enlaces de otros documentos que remiten a un apartado de `docs/hermes.md` DEBEN llevar al
  documento específico que lo desarrolla.
- **FR-017**: Todo enlace de los documentos de este cambio DEBE llevar a un fichero o apartado que existe.
- **FR-018**: Los documentos de los demás componentes DEBEN mantener su plantilla; este cambio solo toca de ellos los
  enlaces a Hermes que haga falta.
- **FR-019**: Las specs de cambios ya fusionados NO DEBEN modificarse.

### Key Entities *(include if feature involves data)*

- **Documento de alto nivel**: `docs/hermes.md`; la entrada a Hermes: por qué existe, qué hace, quién lo usa y un
  párrafo por concepto y por uso con su enlace.
- **Documento específico**: desarrolla un concepto (contratos, llamadas, comunicación, configuración, proveedores,
  seguridad) o un uso (integración, CLI y variables de entorno, operación, desarrollo); enlaza a los demás en lugar de
  repetirlos.
- **Contrato de la API**: el OpenAPI de Hermes y el sobre común en `shared/contracts`; la fuente de los esquemas
  exactos.
- **Cliente de referencia**: el de las pruebas de Hermes, que se da de alta en Hécate y llama a Hermes a través de Envoy
  como lo haría un componente.
- **DECISIONS**: las decisiones abiertas, entre ellas las piezas de Hermes diseñadas y sin construir.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Alguien que no conoce Hermes, leyendo solo el documento de alto nivel durante menos de 10 minutos, explica
  por qué existe, qué resuelve, qué hace y qué no, y quién lo usa, y llega desde él a cada concepto y a cada uso en un
  solo paso.
- **SC-002**: El 100 % de los apartados y datos de `docs/hermes.md` antes de este cambio tiene su sitio en la estructura
  nueva, o una justificación de por qué sobra.
- **SC-003**: El 100 % de las órdenes de la CLI y de las variables de entorno que existen aparece en la referencia de la
  CLI, con su comportamiento, y la referencia no nombra ninguna que no exista.
- **SC-004**: El 100 % de las operaciones del contrato de la API aparece en la guía de integración, con su
  autenticación, sus errores y su comportamiento cuando Hermes no está listo, y la guía no nombra ninguna que no exista.
- **SC-005**: Con la guía de integración y el contrato de la API, sin abrir las specs ni el código, se puede escribir la
  secuencia completa de un componente: desde que el admin le concede el acceso hasta enviar un mensaje, repetirlo sin
  duplicarlo y actuar ante cada error.
- **SC-006**: La puesta en marcha y el 100 % de las situaciones de la operación tienen su procedimiento escrito, y cada
  paso usa una orden que existe.
- **SC-007**: Siguiendo solo el README de Hermes, quien retoma su desarrollo pasa la suite, con la Hécate real y Docker,
  y las comprobaciones de calidad.
- **SC-008**: El 100 % de las piezas de Hermes diseñadas y sin construir figura en DECISIONS, el README del proyecto
  dice lo que Hermes ya hace, y ningún documento de diseño habla del estado.

## Assumptions

- **Base**: `main` tras la feature 009 (`8d61269`). Hermes tiene construidas la 008 y la comprobación de la 009.
- **Constitución**: la 1.2.1 ya dice que el diseño vigente de un componente vive en `docs/<componente>.md` y en sus
  documentos de detalle en `docs/<componente>/`, así que este cambio no la enmienda.
- **Estructura**: la de la 007 para Hécate. Los nombres exactos de los ficheros de `docs/hermes/` se fijan en el plan.
- **Reparto**:
  - los nueve documentos de `docs/hermes/` son los de la aclaración del 2026-11-16; la guía de integración cuenta las
    llamadas desde el lado de quien llama y enlaza al documento de llamadas;
  - los datos que guarda Hermes y dónde van en el documento de seguridad, como en Hécate;
  - la organización interna del paquete y las pruebas van en `hermes/README.md`, y la batería de conformidad, además, en
    el documento de proveedores.
- **El estado**: como acordó el propietario en la 007, el estado de lo construido vive solo en DECISIONS y en el README
  del proyecto. Las piezas de Hermes sin construir ya figuran casi todas en DECISIONS (las tasks, `chat.ask`, los grupos
  de contactos y `wrap` / `unwrap`); el plan comprueba si falta alguna.
- **Idioma**: los documentos en español, con los términos técnicos en inglés, como el resto de la documentación.
- **Contraste**: cada dato se comprueba contra el código de `hermes/`, los OpenAPI de `shared/contracts` y la CLI real.
- **Fuera de alcance**: cambiar el código, el comportamiento o los contratos de Hermes; documentar aparte el protocolo
  del canal local, cuyo único cliente es la CLI; cómo corren los componentes en el home lab; modificar las specs de
  cambios ya fusionados; y reestructurar la documentación de otros componentes.
- **Rama**: `docs/hermes-reference`, creada desde `main`; commit de squash `docs:`.
