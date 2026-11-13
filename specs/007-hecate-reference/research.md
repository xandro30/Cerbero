# Research: Hécate documentada antes de pausarla

Decisiones del cambio 007. Cada una dice qué se elige, por qué y qué se descartó. Parte de `main` tras la feature 006
(`d589bf6`): un solo documento de diseño, `docs/hecate.md`, con la plantilla de 14 apartados, y lo necesario para usar
Hécate repartido en specs cerradas y en el código.

## R0. Decisiones del propietario

- **Specify y clarify (2026-11-12):**
  - el estado de lo diseñado y sin construir vive solo en DECISIONS y en el README; los documentos de diseño no dicen
    nada de su estado, y las referencias de uso describen solo lo que existe;
  - la seguridad tiene su propio documento específico (FR-018);
  - la guía de integración no lleva código: formatos exactos, secuencias con diagramas y el componente de prueba como
    cliente de referencia (FR-005);
  - la documentación no describe ningún gestor de servicios; cómo corren los componentes en el home lab pasa a
    DECISIONS como abierta (FR-019).
- **Plan (2026-11-12):** los documentos de uso se contrastan con comprobaciones automáticas y con un recorrido real con
  el binario release (R11).

## R1. Estructura y nombres

**Decisión:** `docs/hecate.md` es el documento de alto nivel, y `docs/hecate/` tiene nueve documentos específicos, con
nombre en español y sin tildes:

| Documento | Desarrolla |
|---|---|
| `docs/hecate/sellado.md` | La raíz en memoria, la frase y las partes, desbloquear (también, como diseño, desde Ténaro), sellar y el freno de intentos |
| `docs/hecate/secretos.md` | Secret y Key, el cifrado por sobre, el formato cifrado, las versiones y su ciclo de vida, la huella, los metadatos, la rotación de las claves de ámbito y `wrap`/`unwrap` |
| `docs/hecate/identidades.md` | Identity y Certificate, la CA en dos niveles y sus ceremonias, el alta, la renovación única, la revocación, el freno de altas, el certificado propio y las identidades hijas |
| `docs/hecate/acceso.md` | La Access Policy (también la regla de lectura de las identidades hijas), la resolución y sus rechazos, el permiso de estado y los permisos que salen de las declaraciones |
| `docs/hecate/auditoria.md` | El registro de accesos, su retención y la firma de checkpoints |
| `docs/hecate/seguridad.md` | Frente a qué protege Hécate, qué mecanismo cubre cada amenaza y los riesgos aceptados |
| `docs/hecate/integracion.md` | El lado del componente: alta, token corto, renovación, publicaciones, resolución, estado, errores y pruebas |
| `docs/hecate/cli.md` | Cómo se lanza, lo que necesita del sistema, las variables de entorno y cada orden de la CLI |
| `docs/hecate/operacion.md` | La puesta en marcha y el procedimiento de cada situación de operación |

El desarrollo de Hécate va en `hecate/README.md` (R8).

- Los nombres en español siguen al contenido; sin tildes, para que las rutas y los enlaces no dependan de la
  codificación.
- Cada documento específico abre con un párrafo que dice qué cubre y para quién, desarrolla su tema y cierra con
  «Relacionado»: los enlaces al documento de alto nivel y a sus vecinos.

**Alternativas:** nombres en inglés (rompen con el idioma de los docs); una carpeta `hecate/docs/` dentro del componente
(separa el diseño de `docs/`, que la constitución nombra como su sitio); ampliar `docs/hecate.md` (lo contrario de lo que
pide el propietario).

## R2. El documento de alto nivel

**Decisión:** `docs/hecate.md` deja la plantilla de 14 apartados y pasa a este esquema:

1. **Por qué existe**: el problema que resuelve: los secretos de terceros, la identidad entre componentes y la auditoría
   de Cerbero sin confiar en cada componente.
2. **Qué hace y qué no hace**: su frontera.
3. **Quién la usa**: los componentes, el admin y Ténaro, con el diagrama de contexto.
4. **Cómo encaja todo**: una página con la vida de un componente con Hécate (alta, llamadas, secretos, renovación), con
   un diagrama, y una tabla que resume sus cinco aggregates.
5. **Conceptos**: un párrafo y un enlace por concepto: sellado, secretos, identidades, acceso, auditoría y seguridad.
6. **Usarla y desarrollarla**: un párrafo y un enlace por uso: integración, CLI y configuración, operación y desarrollo.
7. **Ante fallos**: qué pasa si Hécate cae, se sella o pierde el disco (principio VI); los procedimientos, en operación.
8. **Decisiones y riesgos**: los ADR, las desviaciones y los riesgos aceptados, con enlaces a DECISIONS.
9. **Preguntas frecuentes**: las de hoy; la que toca un concepto enlaza a su documento.
10. **Referencias**.

Los demás `docs/<componente>.md` mantienen su plantilla (FR-016).

**Alternativas:** mantener los 14 apartados con remisiones a los documentos nuevos (sigue siendo largo y mezcla el alto
nivel con el detalle).

## R3. Qué va a cada documento

**Decisión:** cada apartado del documento actual tiene destino (FR-004, SC-002). A una referencia de uso (integración,
CLI y operación) solo llega lo construido; el diseño de lo que no lo está va a su documento de concepto (FR-013):

| Apartado actual | Destino |
|---|---|
| 1. Propósito y frontera | Alto nivel: «Por qué existe» y «Qué hace y qué no hace» |
| 2. Contexto: diagrama y quién usa qué | Alto nivel: «Quién la usa», con lo que necesita cada componente como diseño; en integración, solo lo que ya puede usar |
| 3. Aggregates | Alto nivel: la tabla resumen; Secret y Key en secretos; Identity, con las identidades hijas, y Certificate en identidades; Access Policy en acceso |
| 3. Jerarquía de claves | Secretos; la raíz, presentada en sellado |
| 3. Formato de un secreto cifrado | Secretos |
| 3. Dónde se guarda | Cada tabla con su concepto: `events`, `secret_material` y `key_material` en secretos; `ca_material`, `hecate_tls` y `enroll_attempts` en identidades; la configuración de sellado en sellado; el registro de accesos en auditoría |
| 3. Huella | Secretos |
| 4. Arranque y desbloqueo | Sellado, también el desbloqueo desde Ténaro como diseño; sus tablas de operaciones del canal local, en la CLI, solo con lo construido |
| 4. Resolver un secreto | Acceso; el lado del componente, en integración |
| 4. Consultar el estado | Acceso; el lado del componente, en integración |
| 4. Alta y renovación de una identidad | Identidades; el lado del componente, en integración |
| 4. Rotar una clave de ámbito | Secretos |
| 4. Ciclo de vida de un secreto | Secretos; los avisos de rotación, en acceso (el estado) |
| 4. Auditoría y checkpoints | Auditoría |
| 5. Contratos | Alto nivel: un párrafo con las superficies; las operaciones construidas de la API, en integración; `wrap`/`unwrap` en secretos y `checkpoints:sign` en auditoría, como diseño; la gestión, en la CLI |
| 6. Configuración y extensión | Los permisos por declaración, en acceso; los metadatos, en secretos; los parámetros, en la CLI y en cada concepto (R12); el adapter de hardware, en sellado |
| 7. Organización interna | `hecate/README.md`, con los crates reales (R8, R12) |
| 8. Datos | Lo que se copia y lo que nunca sale de la memoria, en seguridad; las copias y la restauración, en operación; la retención, en auditoría |
| 9. Seguridad | Seguridad; la regla de que una identidad padre no lee los secretos de sus hijas, en acceso |
| 10. Operación | Alto nivel: «Ante fallos»; cada fila, con su procedimiento, en operación, sin lo que no existe (como el desbloqueo desde Ténaro); la observabilidad, en decisiones y riesgos |
| 11. Pruebas | `hecate/README.md` (cómo se prueba); la Hécate de pruebas para otros componentes, en integración; la restauración probada, en operación |
| 12. Decisiones y riesgos | Alto nivel |
| 13. Preguntas frecuentes | Alto nivel |
| 14. Referencias | Alto nivel |

**Alternativas:** repartir sin un mapa (no habría forma de comprobar que nada se pierde).

## R4. Guía de integración

**Decisión:** `docs/hecate/integracion.md` sigue la vida de un componente, sin código (R0):

1. **Qué tiene y qué guarda un componente:** su clave TLS (P-256), su clave de firma de llamadas (Ed25519), su
   certificado con su cadena y la huella de la CA raíz que trae el token de alta.
2. **El flujo completo:** un diagrama desde el token de alta hasta resolver un secreto y renovar el certificado.
3. **Alta:** cómo se comprueba a Hécate con la huella antes de enviar nada, la petición con la CSR y la clave de firma de
   llamadas, la `Idempotency-Key` y la respuesta.
4. **Cada llamada autenticada:** el certificado de cliente y el token corto. Su formato exacto (cabecera, campos,
   firma), cuánto dura como mucho, que vale una sola vez, a qué certificado queda ligado y qué valida Hécate.
5. **Renovación:** a los dos tercios de la vida del certificado, una sola vez, con claves nuevas; la repetición exacta y
   el segundo intento.
6. **Lo que publica Hécate:** la lista de revocación y las claves de firma de llamadas, para qué sirven y cada cuánto se
   refrescan.
7. **Resolver secretos:** la petición, la respuesta, la caché blanda y la dura, y el rechazo único.
8. **Consultar el estado:** con el permiso de estado, también sellada.
9. **Errores y Hécate sellada:** el sobre común, cada código con su estado HTTP y si se puede reintentar, `Retry-After`
   y `X-Request-Id`.
10. **Límites:** el tamaño del cuerpo, las referencias por petición y los tiempos.
11. **Operaciones:** una tabla con cada operación, su autenticación y su enlace al OpenAPI de `shared/contracts`.
12. **Probar un componente contra Hécate:** una Hécate con su propio directorio de datos y su trust domain, y el
    componente de prueba como cliente de referencia.

**Fuentes:** el OpenAPI, los contratos de las specs 003 y 006 (el formato del token corto, en `formats.md`), y en el
código `api/auth.rs`, `call_token.rs`, `api/routes.rs`, `api/envelope.rs` y el componente de prueba (`testing.rs` y el
ejemplo `component`).

**Alternativas:** dos documentos, contrato y flujos (se solapan casi entero; el contrato de máquina ya es el OpenAPI).

## R5. Referencia de la CLI y la configuración

**Decisión:** `docs/hecate/cli.md` tiene:

1. **Lanzar Hécate:** `hecate serve`, que arranca sellada.
2. **Lo que necesita del sistema:** la memoria bloqueada, los volcados de memoria desactivados, el directorio de datos y
   el socket solo para su usuario y quién puede usar el canal local, comprobado con `process.rs`, `config.rs` y
   `control/server.rs`.
3. **Variables de entorno:** las seis que existen (`HECATE_DATA_DIR`, `HECATE_SOCKET_PATH`, `HECATE_KDF_PROFILE`,
   `HECATE_API_ADDR`, `HECATE_CERT_LIFETIME_HOURS` y `HECATE_ENROLL_TOKEN_MINUTES`), con su valor por defecto, su efecto
   y sus límites.
4. **Convenciones:** la salida, el formato de los errores (`error (<código>): <mensaje>`), los códigos de salida (0, 1 y
   2) y cómo se piden los datos sensibles: sin eco o desde un fichero, nunca en la línea de órdenes.
5. **Las órdenes:** las 13 de primer nivel y sus 32 órdenes finales, agrupadas por concepto:
   - sellado: `init`, `unseal`, `seal`, `status`, `passphrase change` y `shares rekey`;
   - secretos: `create`, `list`, `get`, `put`, `revoke`, `restore`, `promote` y `destroy`;
   - CA: `create`, `renew-intermediate` y `replace`;
   - tokens: `create`, `list` y `annul`;
   - identidades: `list`, `get`, `disable` y `enable`;
   - certificados: `revoke`;
   - políticas: `grant-read`, `revoke-read`, `grant-status`, `revoke-status`, `list` y `get`;
   - `serve`.

   Cada una con sus argumentos, su salida si va bien, sus errores y su código de salida.

**Fuentes:** `cli/src/lib.rs` y la ayuda de la CLI; los contratos `cli.md` de las specs 003, 005 y 006 y
`control-protocol.md` de las 001 y 002.

**Alternativas:** documentar también el protocolo del canal local (fuera de alcance: su único cliente es la CLI).

## R6. Procedimientos de operación

**Decisión:** `docs/hecate/operacion.md` tiene un procedimiento por situación, cada uno con «Cuándo», «Qué necesitas»,
«Pasos» (con las órdenes de la CLI) y «Cómo comprobarlo». Son las nueve situaciones de la tabla de operación del
documento actual, más la puesta en marcha y el arranque tras un reinicio o un corte, que el documento actual describe
aparte:

| Procedimiento | Notas |
|---|---|
| Puesta en marcha | Inicializar, desbloquear, crear la CA, dar de alta el primer componente y conceder sus permisos |
| Arrancar y desbloquear tras un reinicio o un corte | |
| Hécate caída o sellada | Qué siguen haciendo los componentes y cómo se vuelve |
| Se pierde el disco | Hécate no tiene copias propias: una copia en frío del directorio de datos y su restauración; la política común de copias, en DECISIONS |
| Se olvida la frase | Desbloquear con dos partes y fijar una frase nueva |
| Se pierden la frase y las partes | Inicializar de nuevo, volver a dar de alta las identidades y volver a introducir los secretos |
| CA intermedia comprometida, o clave de la CA raíz o su frase perdidas | Sustituir la CA |
| La intermedia va a caducar | Renovarla con la copia de la clave de la CA raíz |
| Clave de un componente comprometida | Revocación urgente o deshabilitar la identidad |
| Clave propia de Hécate comprometida | Revocación urgente de su certificado |
| Clave de ámbito comprometida | Las claves de ámbito no se rotan: lo que sí se puede hacer; la rotación de las claves de ámbito, en DECISIONS |

No describe ningún gestor de servicios ni nada que no exista, como el desbloqueo desde Ténaro (FR-007, FR-013,
FR-019). Lo que no existe se dice como un hecho y remite a DECISIONS, sin palabras de tiempo («hoy», «todavía», «por
ahora»).

**Alternativas:** procedimientos dentro de la referencia de la CLI (mezcla qué hace cada orden con qué hacer ante cada
situación).

## R7. Documento de seguridad

**Decisión:** `docs/hecate/seguridad.md` tiene:

1. **Frente a qué protege:** el robo del disco o de una copia, un componente comprometido, alguien en la red interna,
   un componente que prueba rutas ajenas, un error del admin y una reescritura de la auditoría.
2. **Qué lo cubre:** una tabla amenaza → mecanismo → documento que lo desarrolla (sellado, cifrado por sobre, mTLS y
   token corto, Access Policy y rechazos iguales, borrado de la memoria, registro de accesos y checkpoints).
3. **Principios:** denegar por defecto, nada secreto en eventos, logs ni filas, y los secretos de paso se borran.
4. **Lo que no cubre y los riesgos aceptados:** con enlaces a DECISIONS.
5. **Datos:** qué se guarda y se copia, qué nunca sale de la memoria y los datos personales.

**Fuentes:** el apartado 9 actual, el 8 (datos), los riesgos y desviaciones de DECISIONS y el ADR 0010.

## R8. README de desarrollo

**Decisión:** `hecate/README.md` explica:

- qué es Hécate, con el enlace al documento de alto nivel;
- qué herramientas necesita (la versión de Rust del workspace, cargo-deny y, para Linux, Docker con Colima);
- cómo se compila;
- la suite en macOS y en el contenedor Linux, con `CARGO_BUILD_JOBS=1` y la memoria bloqueada;
- las comprobaciones de calidad (fmt, clippy con los avisos como errores y cargo-deny);
- el componente de prueba (la feature `test-support` y el ejemplo `component`);
- la organización real de los crates y los patrones (Event Sourcing, ports y adapters, cifrado por sobre);
- dónde está la spec de cada feature.

**Fuentes:** el quickstart de la feature 006 (apartados 2 y 3), los `Cargo.toml` del workspace y los crates.

## R9. El estado: DECISIONS y README

**Decisión:**

- **DECISIONS, «Abiertas»:** siete filas nuevas, con el formato de las que ya mezclan lo diseñado sin implementar (como
  el coche escoba de Caronte): «<pieza> | Diseñado, no implementado: <qué es>; <quién lo necesita>».
  - firma de checkpoints de Hécate (con la clave de firma de eventos y la verificación periódica);
  - `wrap`/`unwrap` de Hécate;
  - identidades hijas;
  - permisos que salen de las declaraciones;
  - rotación de las claves de ámbito;
  - gestión y desbloqueo de Hécate desde Ténaro;
  - despliegue de los componentes en el home lab (gestor de servicios o contenedores, usuarios y arranque), común a
    todos y sin diseñar (FR-019).

  No se repiten las que ya están: el adapter de hardware, la rotación automática de secretos, el anclaje externo de los
  checkpoints, la observabilidad y la política de copias.
- **README, «Estado del Proyecto»:** una línea con lo que Hécate ya hace (sellado, secretos, identidades, Access Policy
  y resolución por la API) y el enlace a `docs/hecate.md`.

**Alternativas:** una sección nueva en DECISIONS para lo diseñado sin construir (DECISIONS ya lo recoge en «Abiertas»).

## R10. Aclaración de la constitución (1.2.1)

**Decisión:** subida PATCH a la 1.2.1, con `/speckit-constitution`:

- «Fuentes de verdad»: «`docs/<componente>.md` y, si los tiene, sus documentos de detalle en `docs/<componente>/`, con
  ARCHITECTURE, describen el diseño vigente, sin MVP, roadmap ni futuro».
- «Consolidación»: lo que modifica el diseño se lleva a `docs/<componente>.md` o a sus documentos de detalle.
- Historial:
  - **Qué:** el diseño de un componente puede tener documentos de detalle.
  - **Por qué:** los docs de componente crecen, y el propietario prefiere un documento de alto nivel que introduce cada
    concepto y documentos específicos que lo desarrollan.
  - **Alternativas:** un documento único (difícil de manejar); una carpeta dentro del componente (separa el diseño de
    `docs/`); no aclararlo (la constitución dejaría fuera los documentos de detalle).
  - **Afecta:** `docs/hecate.md` y `docs/hecate/`, y los demás componentes cuando se documenten.
- Es PATCH porque aclara dónde vive el diseño sin cambiar ninguna regla.

## R11. Verificación: comprobaciones y recorrido

**Decisión:** las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas: se escriben antes que los
documentos, fallan sobre `main` y pasan al terminar.

- **Comprobaciones automáticas:**
  - cada orden final de la ayuda de la CLI está en la referencia, y ningún documento de Hécate nombra una orden que no
    exista;
  - cada variable de `config.rs` está en la referencia con su valor por defecto y sus límites, y ningún documento nombra
    otra;
  - cada operación del OpenAPI está en la guía de integración, y la guía no nombra otras rutas `/v1/`;
  - los enlaces y las anclas de los documentos del cambio llevan a un sitio que existe;
  - el cambio no toca código, configuración ni manifiestos;
  - DECISIONS, el README y la constitución tienen sus líneas nuevas;
  - el documento de alto nivel no pasa de 2.500 palabras (SC-001) y ningún documento de Hécate usa palabras de tiempo.
- **Recorrido con el binario release**, en un directorio temporal con un socket de ruta corta y un driver para las
  órdenes interactivas (`hecate init`, las ceremonias de la CA, `unseal` con partes y `passphrase change` con su
  prueba), siguiendo los procedimientos tal como están escritos:
  - la puesta en marcha completa, hasta que el componente de prueba resuelve un secreto;
  - reiniciar y desbloquear;
  - la copia en frío y su restauración;
  - desbloquear con dos partes y cambiar la frase;
  - la revocación urgente de un componente y deshabilitarlo;
  - la revocación urgente del certificado propio de Hécate;
  - renovar la intermedia y sustituir la CA.
- Los scripts viven fuera del repositorio, en el espacio de trabajo de la sesión: el cambio no añade código (FR-001).

**Alternativas:** solo comprobaciones y lectura (el propietario prefirió el recorrido: un paso mal escrito solo se vería
en un incidente).

## R12. Datos que no coinciden con el código

**Decisión:** el documento pasa a describir lo que hace el código (FR-004), y la diferencia queda anotada en la guía de
revisión:

- **Los parámetros que el apartado 6 llama configurables** y no tienen variable (el periodo de gracia y los tiempos de
  caché): los documentos dan su valor y no los presentan como configurables. La referencia de la CLI lista solo las seis
  variables.
- **La retención de la auditoría**, que el apartado 8 llama configurable: el registro de accesos no se purga, lo que
  cumple la retención larga; no se presenta como configurable.
- **El árbol del apartado 7**, con módulos que no existen (cadenas, checkpoints y `wrap`): el README de desarrollo
  describe los crates reales. El diseño de esas piezas sigue en sus documentos de concepto, sin estado (R0).
- Lo configurable de una pieza sin construir (el periodo de rotación de las claves de ámbito) sigue como diseño.
- Si al contrastar el código parece contradecir una spec fusionada, no se corrige aquí: se avisa al propietario.

## R13. Enlaces y otros documentos

**Decisión:**

- ARCHITECTURE remite a «Detalle completo en `docs/hecate.md`», que sigue valiendo; si en algún punto remite a un
  detalle concreto de Hécate, pasa a enlazar al documento específico.
- El README y `hecate/README.md` siguen enlazando a `docs/hecate.md`.
- GLOSSARY solo cambia si una entrada remite a un detalle que se mueve.
- Los docs de los demás componentes no cambian, salvo el enlace que haga falta (FR-016).
- Las specs fusionadas enlazan al fichero, que se mantiene, y nadie enlaza a un apartado concreto suyo (FR-017).

## R14. Diagramas y formato

**Decisión:**

- Mermaid, como hoy. Los diagramas actuales se mueven con su contenido: el contexto al alto nivel; la jerarquía de
  claves y el ciclo de vida a secretos; el desbloqueo a sellado; la resolución a acceso; el alta y la renovación a
  identidades; los checkpoints a auditoría.
- La guía de integración gana los suyos: el flujo completo, la llamada autenticada y la renovación.
- Tablas para las referencias (órdenes, variables, operaciones y códigos).
