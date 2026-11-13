---

description: "Tareas del cambio 007: Hécate documentada antes de pausarla"
---

# Tasks: Hécate documentada antes de pausarla

**Input**: Design documents from `/specs/007-hecate-reference/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md),
[quickstart.md](quickstart.md)

**Tests**: las comprobaciones de [quickstart.md](quickstart.md) hacen de pruebas (principio IV, research R11). Fallan
sobre `main`, y cada historia termina pasando las suyas. El recorrido de los procedimientos (apartado 8) se ejecuta con
el binario release.

**Organization**: las tareas se agrupan por historia de usuario (US1 a US6 de la spec), en el orden de sus prioridades.
US4 se apoya en la referencia de la CLI de US3; el resto de historias son independientes entre sí.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: se puede hacer en paralelo (otro fichero, sin dependencias pendientes)
- **[Story]**: historia de usuario de la spec (US1-US6)
- Rutas relativas a la raíz del repositorio

## Convenciones para todas las tareas

- **Solo documentación (FR-001):** no cambia ningún fichero de código, configuración de compilación ni manifiesto. Los
  scripts de comprobación y del recorrido viven fuera del repositorio.
- **Estilo:** español, con los términos técnicos consolidados en inglés, y el de los docs actuales (tablas, viñetas,
  líneas de hasta 120 caracteres). Los diagramas, en Mermaid (research R14).
- **Marco:** cada documento específico sigue el de [data-model.md](data-model.md#el-marco-de-un-documento-específico):
  un párrafo de qué cubre y para quién, el desarrollo y «Relacionado».
- **Estado (FR-013):** los documentos de diseño no dicen nada del estado de lo que describen; las referencias de uso
  (integración, CLI y operación) describen solo lo que existe.
- **Sin palabras de tiempo:** lo que no existe se dice como un hecho («Hécate no tiene copias propias») y remite a
  DECISIONS, sin «hoy», «todavía» ni «por ahora» (apartado 10 de quickstart.md).
- **Un solo sitio (FR-003):** cada tema se desarrolla en un documento y los demás lo enlazan.
- **Datos (FR-012):** cada dato se contrasta con el código y el OpenAPI. Un dato del documento actual que no coincide
  pasa a describir el código y se anota para la guía de revisión (research R12). Si el código parece contradecir una
  spec fusionada, se avisa al propietario y no se corrige aquí.
- **Lo que no se toca:** las specs fusionadas, los ADR y la plantilla de los docs de otros componentes (FR-016,
  FR-017).
- **Constitución:** solo se enmienda con `/speckit-constitution` (research R10).
- **Git:** el propietario hace el commit, el push y la PR; ninguna tarea ejecuta comandos de git que cambien el estado.

---

## Phase 1: Setup (Shared Infrastructure)

- [X] T001 Comprobar que la rama activa es `docs/hecate-reference` (`git branch --show-current`) y que `.specify/feature.json` apunta a `specs/007-hecate-reference`
- [X] T002 Ejecutar la línea base de `specs/007-hecate-reference/quickstart.md` (apartados 1 a 6 y 10) y comprobar que coincide con lo esperado sobre `main`: faltan las 32 órdenes, las 6 variables y las 8 operaciones; los enlaces de los documentos actuales están bien; no hay cambios fuera de Markdown; DECISIONS, el README y la constitución no tienen sus líneas nuevas; y `docs/hecate.md` pasa de las 2.500 palabras
- [X] T003 Preparar fuera del repositorio el recorrido del apartado 8 de `specs/007-hecate-reference/quickstart.md`: el binario release y el ejemplo `component`, un directorio temporal de ruta corta y un driver para las órdenes interactivas del recorrido: `hecate init` (la confirmación y las partes), `ca create`, `ca renew-intermediate` y `ca replace` (la frase de la CA y la copia de la clave raíz), `unseal` con partes y `passphrase change` con su prueba. Los scripts de la feature 006 ya no existen: se escriben de nuevo

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: que todos los documentos existan desde el principio, para que cada historia pueda enlazar a los demás.

- [X] T004 Crear `docs/hecate/` con los nueve documentos de research R1 (`sellado.md`, `secretos.md`, `identidades.md`, `acceso.md`, `auditoria.md`, `seguridad.md`, `integracion.md`, `cli.md` y `operacion.md`), cada uno solo con su marco de data-model.md: título, párrafo de qué cubre y para quién, y «Relacionado»

**Checkpoint**: los nueve ficheros existen y los enlaces de sus «Relacionado» llevan a sitios que existen.

---

## Phase 3: User Story 1 - Entender Hécate y llegar a cada concepto (Priority: P1) 🎯 MVP

**Goal**: `docs/hecate.md` es el documento de alto nivel; los documentos de concepto están completos y nada del
documento actual se ha perdido.

**Independent Test**: leer solo el documento de alto nivel y responder por qué existe Hécate, qué resuelve, qué hace y
qué no, quién la usa y cómo se comporta ante fallos; seguir cada enlace hasta su documento; y pasar el apartado 7 de
quickstart.md.

### Tests for User Story 1

- [X] T005 [US1] Preparar la comprobación «nada se pierde» del apartado 7 de `specs/007-hecate-reference/quickstart.md`: una lista, fuera del repositorio, con cada apartado y cada dato del `docs/hecate.md` de `main` (`git show main:docs/hecate.md`) y su destino según research R3; sobre `main` todos están pendientes

### Implementation for User Story 1

- [X] T006 [P] [US1] Escribir `docs/hecate/sellado.md`: la raíz en memoria, la frase y las partes, desbloquear y sellar con su diagrama de estados, el freno de intentos, la configuración de sellado que se guarda, y como diseño el desbloqueo desde Ténaro y el adapter de hardware (research R1 y R3). Contrastar los tiempos del freno, el mínimo de la frase y los perfiles de Argon2id con `hecate/crates/domain/src/sealing/` y `hecate/crates/application/src/sealing/`
- [X] T007 [P] [US1] Escribir `docs/hecate/secretos.md`: Secret y Key, la jerarquía de claves con su diagrama, el formato cifrado, `events`, `secret_material` y `key_material`, la huella, las versiones y su ciclo de vida con su diagrama, los metadatos, y como diseño la rotación de las claves de ámbito y `wrap`/`unwrap` (research R1 y R3). Contrastar la gracia (fija, research R12), `max_versions`, el formato de la huella y los límites de `rotate_every` con `hecate/crates/domain/src/secrets/` y `hecate/crates/application/src/secrets/`
- [X] T008 [P] [US1] Escribir `docs/hecate/identidades.md`: Identity y Certificate, la CA en dos niveles y sus ceremonias, el alta y la renovación única con su diagrama, la revocación, deshabilitar una identidad, el freno de altas, el certificado propio, la intermedia, las tablas `ca_material`, `hecate_tls` y `enroll_attempts`, y como diseño las identidades hijas (research R1 y R3). Contrastar las duraciones (token, certificado e intermedia), el freno y los avisos con `hecate/crates/domain/src/identity/`, `hecate/crates/domain/src/ca/`, `hecate/crates/application/src/ca/` y `hecate/crates/application/src/identity/`
- [X] T009 [P] [US1] Escribir `docs/hecate/acceso.md`: la Access Policy (también la regla de que una identidad padre no lee los secretos de sus hijas), la resolución en dos fases con su diagrama y su tiempo mínimo, el permiso de estado y sus campos, los avisos y los permisos que salen de las declaraciones como diseño (research R3). Contrastar el mínimo de 250 ms, las 100 referencias y los campos del estado con `hecate/crates/application/src/access_policy/`, `hecate/crates/application/src/identity/service.rs` y `docs/adr/0010-access-policy-hecate.md`
- [X] T010 [P] [US1] Escribir `docs/hecate/auditoria.md`: el registro de accesos (qué deja fila y qué no, nunca valores ni huellas), su retención (no se purga, research R12) y la firma de checkpoints con su diagrama como diseño. Contrastar las operaciones y los métodos del registro con `hecate/crates/domain/src/sealing/access_log.rs`
- [X] T011 [P] [US1] Escribir `docs/hecate/seguridad.md` según research R7 y FR-018: frente a qué protege, la tabla amenaza → mecanismo → documento, los principios, los datos (qué se guarda y se copia, qué nunca sale de la memoria y los datos personales) y los riesgos aceptados con enlaces a DECISIONS, sin repetir el desarrollo de cada mecanismo
- [X] T012 [US1] Reescribir `docs/hecate.md` como documento de alto nivel según research R2 y FR-002: por qué existe, qué hace y qué no, quién la usa con su diagrama, cómo encaja todo con la tabla de los aggregates, un párrafo y un enlace por concepto y por uso (también `docs/hecate/integracion.md`, `docs/hecate/cli.md`, `docs/hecate/operacion.md` y `hecate/README.md`), ante fallos, decisiones y riesgos, preguntas frecuentes y referencias. Sin la plantilla de 14 apartados ni el detalle que ya está en los documentos específicos, y sin pasar de 2.500 palabras, unos 10 minutos de lectura (SC-001, apartado 10 de quickstart.md)
- [X] T013 [US1] Aclarar la constitución con `/speckit-constitution` según research R10 y FR-011, en `.specify/memory/constitution.md`: «Fuentes de verdad» y «Consolidación» nombran los documentos de detalle en `docs/<componente>/`, la versión pasa a 1.2.1 con su entrada en el historial (qué, por qué, alternativas y a qué afecta) y el Sync Impact Report se quita antes del commit
- [X] T014 [US1] Pasar la comprobación de T005 (apartado 7 de `specs/007-hecate-reference/quickstart.md`, FR-004): cada apartado y cada dato del documento de `main` está en su destino, y cada dato que cambia por no coincidir con el código queda anotado para la guía de revisión

**Checkpoint**: el documento de alto nivel y los seis documentos de concepto están completos, la constitución está en
la 1.2.1 y el apartado 7 de quickstart.md pasa.

---

## Phase 4: User Story 2 - Integrar un componente con Hécate (Priority: P2)

**Goal**: quien desarrolla otro componente implementa su lado de la integración con la guía y el OpenAPI, sin abrir las
specs ni el código.

**Independent Test**: con la guía y el OpenAPI, escribir la secuencia completa desde el token de alta hasta resolver un
secreto y renovar; y pasar el apartado 3 de quickstart.md.

### Implementation for User Story 2

- [X] T015 [US2] Escribir `docs/hecate/integracion.md` según research R4 y FR-005, sin código. Debe cubrir qué tiene y guarda un componente, el flujo completo, el alta, la llamada autenticada con el formato exacto del token corto, la renovación, lo que publica Hécate, la resolución y su caché, el estado, los errores y Hécate sellada, los límites, la tabla de operaciones con enlaces al OpenAPI y cómo probar un componente contra Hécate, con el componente de prueba como cliente de referencia. Solo lo construido: las operaciones diseñadas sin construir (`wrap`/`unwrap` y `checkpoints:sign`) no aparecen (research R3). Lleva los diagramas del flujo completo, la llamada autenticada y la renovación. Contrastar cada dato con `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`, `hecate/crates/infrastructure/src/api/`, `hecate/crates/infrastructure/src/call_token.rs` y `hecate/crates/infrastructure/src/testing.rs`
- [X] T016 [US2] Pasar el apartado 3 de `specs/007-hecate-reference/quickstart.md` (cada operación del OpenAPI está en la guía y no sobra ninguna ruta) y repasar, con la guía y el OpenAPI y sin abrir las specs ni el código, la secuencia completa desde el token de alta hasta resolver un secreto y renovar (SC-005)

**Checkpoint**: la guía de integración está completa y el apartado 3 de quickstart.md pasa.

---

## Phase 5: User Story 3 - Lanzar y manejar Hécate desde la CLI (Priority: P3)

**Goal**: cómo se lanza Hécate, sus variables y cada orden de la CLI, en un solo documento.

**Independent Test**: pasar los apartados 1 y 2 de quickstart.md.

### Implementation for User Story 3

- [X] T017 [US3] Escribir `docs/hecate/cli.md` según research R5 y FR-006. Debe cubrir cómo se lanza (`hecate serve`), lo que necesita del sistema, las seis variables con su valor por defecto, su efecto y sus límites, las convenciones (la salida, los errores, los códigos de salida 0, 1 y 2, y los datos sensibles sin eco o desde un fichero) y las 32 órdenes finales con sus argumentos, su salida, sus errores y su código de salida. Solo el canal local: el desbloqueo desde Ténaro no aparece (research R3). Contrastar con `hecate/crates/cli/src/lib.rs`, la ayuda de la CLI, `hecate/crates/infrastructure/src/config.rs`, `hecate/crates/infrastructure/src/process.rs` y `hecate/crates/infrastructure/src/control/server.rs`
- [X] T018 [US3] Pasar los apartados 1 y 2 de `specs/007-hecate-reference/quickstart.md` en los dos sentidos (cada orden y cada variable que existen están en la referencia, y ningún documento nombra otras) y comparar a mano los valores por defecto y los rangos de las variables con `hecate/crates/infrastructure/src/config.rs`

**Checkpoint**: la referencia de la CLI está completa y los apartados 1 y 2 de quickstart.md pasan.

---

## Phase 6: User Story 4 - Actuar ante cada situación de operación (Priority: P4)

**Goal**: un procedimiento escrito, y comprobado con el binario, para la puesta en marcha y cada situación de operación.

**Independent Test**: cada situación tiene su procedimiento, y el recorrido del apartado 8 de quickstart.md pasa.

### Implementation for User Story 4

- [X] T019 [US4] Escribir `docs/hecate/operacion.md` según research R6 y FR-007: los 11 procedimientos con «Cuándo», «Qué necesitas», «Pasos» (con las órdenes de `docs/hecate/cli.md`) y «Cómo comprobarlo». La pérdida del disco y la clave de ámbito comprometida dicen, como un hecho, lo que Hécate no hace y lo que sí se puede hacer, y remiten a DECISIONS. Ni gestor de servicios, que remite a la decisión abierta del despliegue (FR-019), ni Ténaro: los procedimientos usan solo lo que existe (research R3)
- [X] T020 [US4] Ejecutar el recorrido del apartado 8 de `specs/007-hecate-reference/quickstart.md` siguiendo `docs/hecate/operacion.md` tal como está escrito, corregir los pasos que no funcionen y anotar el resultado para la guía de revisión

**Checkpoint**: los procedimientos están escritos y el recorrido pasa.

---

## Phase 7: User Story 5 - Retomar el desarrollo de Hécate (Priority: P5)

**Goal**: `hecate/README.md` explica cómo se compila, se prueba y se organiza Hécate.

**Independent Test**: siguiendo solo el README, compilar, pasar la suite en macOS y en el contenedor Linux y las
comprobaciones de calidad (apartado 9 de quickstart.md).

### Implementation for User Story 5

- [X] T021 [P] [US5] Escribir `hecate/README.md` según research R8 y FR-008: qué es, con el enlace a `docs/hecate.md`; las herramientas; cómo se compila; la suite en macOS y en el contenedor Linux; las comprobaciones de calidad; el componente de prueba; la organización real de los crates y sus patrones; y dónde está la spec de cada feature
- [X] T022 [US5] Seguir solo `hecate/README.md` (apartado 9 de `specs/007-hecate-reference/quickstart.md`): compilar, pasar la suite en macOS y en el contenedor Linux y pasar las comprobaciones de calidad. El resultado es el de la feature 006: 840 pruebas

**Checkpoint**: el README de desarrollo está completo y lo que explica funciona.

---

## Phase 8: User Story 6 - Saber qué está hecho y qué no al pausar (Priority: P6)

**Goal**: DECISIONS recoge lo diseñado y sin construir, y el despliegue en el home lab; el README dice lo que Hécate ya
hace.

**Independent Test**: pasar el apartado 6 de quickstart.md.

### Implementation for User Story 6

- [X] T023 [P] [US6] Añadir a «Abiertas» de `DECISIONS.md` las siete filas de research R9 (FR-009 y FR-019) con el formato de data-model.md: las seis piezas de Hécate como diseñadas y no implementadas, y el despliegue en el home lab como decisión común a todos los componentes, sin repetir las que ya están
- [X] T024 [P] [US6] Añadir a «Estado del Proyecto» de `README.md` la línea con lo que Hécate ya hace y el enlace a `docs/hecate.md` (research R9, FR-010)
- [X] T025 [US6] Pasar el apartado 6 de `specs/007-hecate-reference/quickstart.md`

**Checkpoint**: el estado está en DECISIONS y en el README, y el apartado 6 de quickstart.md pasa.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [X] T026 Revisar los enlaces de `ARCHITECTURE.md`, `GLOSSARY.md` y los docs de otros componentes que remiten a un detalle de Hécate y llevarlos a su documento específico (research R13, FR-014), sin tocar nada más de ellos (FR-016)
- [X] T027 Pasar los apartados 4, 5 y 10 de `specs/007-hecate-reference/quickstart.md` sobre todos los documentos del cambio: enlaces y anclas (FR-015), ningún cambio fuera de Markdown (FR-001), y el documento de alto nivel sin pasar de 2.500 palabras y sin palabras de tiempo en ningún documento (SC-001, FR-013)
- [X] T028 Leer de una vez `docs/hecate.md`, `docs/hecate/`, `hecate/README.md` y las líneas nuevas de `DECISIONS.md`, `README.md` y la constitución, buscando contradicciones entre ellos y con ARCHITECTURE, GLOSSARY y DECISIONS (SC-009), frases de estado en los documentos de diseño o de futuro en cualquiera (FR-013) y temas desarrollados en dos sitios (FR-003)
- [X] T029 Proponer al propietario el mensaje del commit de squash y la descripción de la PR del cambio (`specs/007-hecate-reference/`):
  - **commit:** `docs:`, en inglés, con una línea por bloque con su tipo y sin `Co-Authored-By`;
  - **PR:** en español y sin línea de atribución, con las diferencias con el código que se encontraron (research R12) y el resultado del recorrido;
  - deben recoger `docs/`, `hecate/README.md`, `README.md`, `DECISIONS.md`, `.specify/memory/constitution.md`, `ARCHITECTURE.md` si cambia y `specs/007-hecate-reference/`;
  - el propietario hace el commit, el push y la PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: sin dependencias.
- **Foundational (Phase 2)**: depende del Setup; crea los ficheros que enlazan las historias.
- **US1 (Phase 3)**: depende de la fase fundacional.
- **US2, US3, US5 y US6 (Phases 4, 5, 7 y 8)**: dependen de la fase fundacional y no entre sí; conviene hacerlas tras US1,
  para que los documentos de concepto ya existan cuando las referencias de uso los enlacen.
- **US4 (Phase 6)**: depende de US3, porque cada paso usa una orden de `docs/hecate/cli.md`.
- **Polish (Phase 9)**: al final.

### Within Each User Story

- La comprobación se prepara o se ejecuta primero sobre el estado de partida; después se escribe; al terminar, pasa.
- En US1, los seis documentos de concepto (T006 a T011) van antes del de alto nivel (T012), que resume lo que ellos
  desarrollan.
- Las tareas que tocan el mismo fichero van en orden.

### Parallel Opportunities

- US1: los seis documentos de concepto, T006 a T011.
- Entre historias: T021 (README de desarrollo), T023 y T024 (estado) con las tareas de US2 y US3.

---

## Parallel Example: User Story 1

```bash
Task: "Escribir docs/hecate/sellado.md (T006)"
Task: "Escribir docs/hecate/secretos.md (T007)"
Task: "Escribir docs/hecate/identidades.md (T008)"
Task: "Escribir docs/hecate/acceso.md (T009)"
Task: "Escribir docs/hecate/auditoria.md (T010)"
Task: "Escribir docs/hecate/seguridad.md (T011)"
```

---

## Implementation Strategy

### Primer incremento (MVP)

1. Setup y fase fundacional: la línea base y los nueve ficheros con su marco.
2. US1: los documentos de concepto, el de alto nivel y la constitución 1.2.1.
3. **Revisión del propietario:** el documento de alto nivel y los de concepto.

### Siguientes incrementos

1. US2: la guía de integración, la que más necesitará el siguiente componente.
2. US3 y US4: la referencia de la CLI y los procedimientos, con el recorrido.
3. US5 y US6: el README de desarrollo y el estado.
4. Polish: enlaces, comprobaciones finales, lectura completa y la propuesta de commit y PR.

El cambio se entrega entero en una PR con un commit de squash `docs:`; los incrementos son puntos de revisión.

## Phase 10: Convergence

- [X] T030 Corregir la regla de repetir con otra petición en `docs/hecate/identidades.md` (la viñeta «Repetición» de «El alta y la renovación») y en `docs/hecate/integracion.md` (la viñeta «Repetición» de «El alta»). Hoy las dos dicen que la misma `Idempotency-Key` con otra petición responde `conflict`, y el código (`enroll_checked` y `renew_checked` en `hecate/crates/application/src/identity/service.rs`) hace otra cosa en los dos casos que más se darán: otra petición con un token de alta ya gastado responde `auth` (`two_enrollments_at_once_with_the_same_token_give_one_certificate`), y otra petición con un certificado ya renovado es un segundo intento, con `auth` y la revocación urgente de los certificados válidos de la identidad, sea cual sea su `Idempotency-Key`; en `identidades.md`, eso contradice la viñeta «Renovación única», justo encima. `conflict` solo llega cuando la `Idempotency-Key` ya la usó otra petición de la misma identidad que obtuvo otro certificado (`an_idempotency_key_reused_with_other_content_is_a_conflict_within_its_identity`), como dice la fila `conflict` de la tabla de errores de la guía, que no cambia, per FR-012 / FR-014 / FR-005 (contradicts)
- [X] T031 Completar el procedimiento «La intermedia va a caducar» de `docs/hecate/operacion.md` para cuando ya caducó, que su «Cuándo» incluye, y recoger el hecho en el diseño, en la viñeta del aviso de los 30 días de `docs/hecate/identidades.md`. Ningún certificado dura más que la intermedia que lo firma, tampoco el de Hécate (`sign_leaf` y `renew_own_certificate_if_due` en `hecate/crates/application/src/ca/service.rs`): al caducar la intermedia caduca el certificado de Hécate, así que la API deja de escuchar también desbloqueada (`with_the_intermediate_expired_the_api_stops_listening_also_unsealed`), y caducan los de los componentes, que ya no pueden renovar. Tras `hecate ca renew-intermediate`, Hécate renueva su certificado al momento y la API vuelve a escuchar (el log dice `the API listens`), y cada componente necesita un token de alta nuevo (`with_the_intermediate_expired_the_renewal_is_the_way_out`). Hoy «Cómo comprobarlo» dice que los componentes conservan sus certificados, lo que solo vale si se renueva antes de que caduque, y los pasos no dan las altas nuevas, per FR-007 / FR-012 / US4-AC2 (partial)
- [X] T032 Reescribir en `docs/hecate/cli.md` («Variables de entorno») la frase «Un valor que no vale para el error al momento», que no se entiende: debe decir que, con un valor que no vale, la orden se detiene antes de hacer nada, escribe `error: <mensaje>` y sale con 2, per FR-006 (partial)
- [X] T033 Recoger en la propuesta de commit y PR de T029 las dos entradas de `GLOSSARY.md` que corrigió T026 («Sealed / Unsealed»: sellada responde el estado agregado; «Grace Period»: 24 h fijas), con su motivo. El plan solo preveía tocar GLOSSARY por un enlace (research R13 y «No cambian» del plan) y T029 no nombra `GLOSSARY.md` entre lo que deben recoger el commit y la PR; las correcciones evitan que GLOSSARY contradiga a los documentos del cambio (FR-014, SC-009) y están anotadas en `summary_review.md`, per research R13 / T029 (unrequested)
- [X] T034 Tras T030 a T032, volver a pasar los apartados 1 a 6 y 10 de `specs/007-hecate-reference/quickstart.md` y las comprobaciones de «nada se pierde» del apartado 7, que leen `identidades.md`, `integracion.md`, `operacion.md` y `cli.md`; y anotar en `specs/007-hecate-reference/summary_review.md` lo que encontró converge y lo que cambió, como hizo la feature 006, per FR-004 / FR-015 / SC-009 (partial)

## Phase 11: Convergence

- [X] T035 Completar el «Cómo comprobarlo» de «Arrancar y desbloquear tras un reinicio o un corte» en `docs/hecate/operacion.md` para cuando la intermedia caducó mientras Hécate estaba parada o sellada. Hoy dice que, si Hécate estuvo sellada más tiempo que la vida de su certificado, lo renueva sola al desbloquearla y el log vuelve a decir `the API listens`; pero con la intermedia caducada no lo renueva (`renew_own_certificate_if_due` en `hecate/crates/application/src/ca/service.rs` no hace nada sin una intermedia vigente) y la API sigue sin escuchar. Debe decir que, en ese caso, `hecate status` muestra `intermediate days left: 0`, y remitir a «La intermedia va a caducar», que completó T031; «Hécate caída o sellada», que se comprueba como tras un reinicio, queda cubierto con ello. Después, volver a pasar los apartados 1, 4 y 10 de `specs/007-hecate-reference/quickstart.md` y las comprobaciones de «nada se pierde», y anotarlo en la entrada de converge de `specs/007-hecate-reference/summary_review.md`, per FR-007 / US4-AC2 (partial)
