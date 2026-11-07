# Feature Specification: Arranque de Hécate

**Feature Branch**: `feature/001-hecate-bootstrap`

**Created**: 2026-11-06

**Status**: Draft

**Input**: User description: "Arranque de Hécate: inicializar, sellar y desbloquear con la frase del admin, y recuperar con Shamir 2 de 3"

## Clarifications

### Session 2026-11-07

- Q: Con Hécate desbloqueada, ¿cambiar la frase o las partes exige volver a demostrarlo? → A: Sí: la frase
  actual o, si se olvidó, dos partes válidas.
- Q: ¿El contador de intentos fallidos sobrevive a un reinicio y es común a frase y partes? → A: Sí: se
  mantiene tras reiniciar y frase y partes comparten el mismo contador.
- Q: Si se pierden la frase y dos partes, ¿Hécate ofrece una operación para borrarlo todo y reinicializar? → A:
  No: es un procedimiento manual documentado (parar Hécate y borrar sus datos), fuera de esta feature.

## User Scenarios & Testing *(mandatory)*

Hécate custodia los secretos y las claves de Cerbero. Su raíz (la clave que protege todo lo demás) solo existe
descifrada mientras Hécate está desbloqueada. Esta feature cubre su ciclo de arranque: ponerla en marcha por
primera vez, desbloquearla tras cada reinicio, sellarla a mano y recuperarla si el admin olvida la frase.

El actor es el **admin**, el único usuario privilegiado de Cerbero. En esta feature actúa desde el equipo
donde corre Hécate.

### User Story 1 - Inicializar Hécate (Priority: P1)

El admin pone en marcha Hécate por primera vez. Hécate genera su raíz, el admin fija su frase y Hécate genera
tres partes de recuperación, de las que bastan dos para recuperar la raíz. Las partes se muestran una sola vez
y el admin confirma que las ha guardado. Al terminar, Hécate queda sellada.

**Why this priority**: sin inicializar, Hécate no puede hacer nada; es la base de todas las demás historias.

**Independent Test**: sobre una instalación nueva, inicializar y comprobar que Hécate pasa a sellada, que las
partes se mostraron una vez y que ni la frase, ni las partes, ni la raíz quedan guardadas en claro.

**Acceptance Scenarios**:

1. **Given** una Hécate sin inicializar, **When** el admin inicia la inicialización y fija una frase válida,
   **Then** Hécate muestra tres partes de recuperación una sola vez y pide confirmarlas.
2. **Given** las partes mostradas, **When** el admin confirma correctamente dos de ellas, **Then** la
   inicialización termina y Hécate queda sellada.
3. **Given** las partes mostradas, **When** el admin abandona antes de confirmar, **Then** no queda nada
   inicializado y se puede empezar de nuevo.
4. **Given** una Hécate ya inicializada, **When** alguien intenta inicializarla otra vez, **Then** se rechaza
   sin modificar nada.

---

### User Story 2 - Desbloquear con la frase tras un reinicio (Priority: P1)

Tras cualquier reinicio, Hécate arranca sellada: solo permite desbloquearse y consultar su estado. El admin
escribe su frase y Hécate pasa a desbloqueada, lista para servir.

**Why this priority**: es la operación de todos los días; sin ella, cada reinicio deja a Cerbero sin secretos.

**Independent Test**: con una Hécate inicializada y sellada, desbloquear con la frase correcta y comprobar el
cambio de estado; reiniciar y comprobar que vuelve a estar sellada.

**Acceptance Scenarios**:

1. **Given** Hécate sellada, **When** el admin introduce la frase correcta, **Then** pasa a desbloqueada.
2. **Given** Hécate sellada, **When** se introduce una frase incorrecta, **Then** sigue sellada, se registra el
   intento fallido y el siguiente intento tiene que esperar más.
3. **Given** Hécate desbloqueada, **When** se reinicia, **Then** arranca sellada.
4. **Given** Hécate sellada, **When** se pide cualquier operación que no sea desbloquear o consultar el estado,
   **Then** se rechaza indicando que está sellada.

---

### User Story 3 - Sellar a mano (Priority: P2)

Ante una sospecha de intrusión, el admin sella Hécate con una sola acción. La raíz descifrada desaparece y
Hécate deja de servir hasta que se desbloquee de nuevo.

**Why this priority**: es la respuesta de emergencia; importante, pero no impide usar Hécate en el día a día.

**Independent Test**: con Hécate desbloqueada, sellarla y comprobar que vuelve a estado sellado y rechaza
cualquier operación salvo desbloquear o consultar el estado.

**Acceptance Scenarios**:

1. **Given** Hécate desbloqueada, **When** el admin la sella, **Then** pasa a sellada en esa misma acción.
2. **Given** Hécate ya sellada, **When** el admin la sella, **Then** sigue sellada y se le informa con el estado
   actual, sin cambiar nada.

---

### User Story 4 - Recuperar con 2 de 3 partes (Priority: P2)

El admin ha olvidado la frase. Introduce dos de las tres partes de recuperación, Hécate se desbloquea y le
permite fijar una frase nueva.

**Why this priority**: sin recuperación, olvidar la frase supone perder todos los secretos; no es el uso
diario, pero evita un desastre.

**Independent Test**: con Hécate inicializada y sellada, desbloquear con dos partes cualesquiera, fijar una
frase nueva y comprobar que la frase anterior ya no sirve y la nueva sí.

**Acceptance Scenarios**:

1. **Given** Hécate sellada, **When** el admin introduce dos partes válidas, **Then** pasa a desbloqueada y se
   le recomienda fijar una frase nueva.
2. **Given** Hécate desbloqueada por partes, **When** el admin fija una frase nueva, **Then** la nueva frase
   desbloquea en el siguiente reinicio y la anterior no.
3. **Given** Hécate sellada, **When** se introduce una sola parte, **Then** sigue sellada.
4. **Given** Hécate sellada, **When** se introduce una parte mal copiada, **Then** se avisa de que esa parte no
   es válida, sin gastar un intento de desbloqueo.

---

### User Story 5 - Cambiar la frase o repartir de nuevo las partes (Priority: P3)

Con Hécate desbloqueada, el admin cambia su frase o genera un juego nuevo de partes (por ejemplo, si cree que
una se ha perdido o está expuesta). Los secretos no cambian; solo cambia cómo se protege la raíz.

**Why this priority**: es mantenimiento; se puede vivir sin ello hasta que haga falta.

**Independent Test**: cambiar la frase y comprobar que solo la nueva desbloquea; repartir de nuevo y comprobar
que las partes antiguas ya no recuperan y las nuevas sí.

**Acceptance Scenarios**:

1. **Given** Hécate desbloqueada, **When** el admin cambia la frase introduciendo la actual (o dos partes),
   **Then** solo la nueva frase desbloquea.
2. **Given** Hécate desbloqueada, **When** el admin genera un juego nuevo de partes, demostrándolo con la frase
   actual o dos partes, y lo confirma, **Then** las partes antiguas dejan de servir y las nuevas recuperan.
3. **Given** Hécate desbloqueada, **When** alguien intenta cambiar la frase o las partes sin la frase actual ni
   dos partes válidas, **Then** se rechaza, no cambia nada y cuenta como intento fallido.

---

### Edge Cases

- Se va la luz durante la inicialización: al arrancar, Hécate sigue sin inicializar y se puede empezar de nuevo.
- Se va la luz durante un cambio de frase o un reparto de partes: la raíz sigue protegida por la protección
  anterior completa; nunca queda a medias.
- Intentos fallidos repetidos de desbloqueo: la espera entre intentos crece y cada intento queda registrado.
- Reiniciar Hécate para saltarse la espera: el contador y la espera pendiente siguen tras el reinicio.
- Alternar entre frase y partes para tener más intentos: todos los fallos suman en el mismo contador.
- Se pierden la frase y dos partes: los datos no se pueden recuperar; es el comportamiento esperado y Hécate lo
  indica antes de inicializar. Volver a empezar es un procedimiento manual (parar Hécate y borrar sus datos);
  Hécate no ofrece ninguna operación para borrarlo todo.
- Dos personas intentan desbloquear a la vez: solo se acepta un desbloqueo; el otro recibe el estado actual.
- Se pide desbloquear una Hécate ya desbloqueada: no cambia nada y se informa del estado.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hécate MUST tener tres estados: sin inicializar, sellada y desbloqueada.
- **FR-002**: Hécate MUST permitir inicializarse una sola vez; un segundo intento se rechaza sin cambios.
  Hécate MUST NOT ofrecer ninguna operación que borre sus datos para volver a inicializar.
- **FR-003**: Al inicializar, Hécate MUST generar una raíz aleatoria, pedir al admin una frase y generar tres
  partes de recuperación de las que dos bastan para reconstruir la raíz.
- **FR-004**: Hécate MUST mostrar las partes una sola vez y no dar la inicialización por terminada hasta que el
  admin confirme dos de ellas; si se abandona antes, no queda nada inicializado.
- **FR-005**: Hécate MUST rechazar frases de menos de 12 caracteres.
- **FR-006**: Hécate MUST NOT arrancar nunca desbloqueada: tras cualquier arranque, también inesperado, está sin
  inicializar o sellada.
- **FR-007**: Sellada, Hécate MUST aceptar solo dos operaciones: desbloquear y consultar su estado.
- **FR-008**: Hécate MUST desbloquearse con la frase del admin o con dos partes de recuperación.
- **FR-009**: Hécate MUST detectar una parte mal copiada antes de usarla y avisar sin contar un intento fallido.
- **FR-010**: Hécate MUST permitir al admin sellarla con una sola acción, y eliminar la raíz descifrada al sellar.
- **FR-011**: Tras desbloquearse con partes, Hécate MUST recomendar al admin fijar una frase nueva.
- **FR-012**: Desbloqueada, Hécate MUST permitir al admin cambiar la frase y generar un juego nuevo de partes,
  siempre que lo demuestre de nuevo con la frase actual o con dos partes válidas; la protección anterior deja de
  servir solo cuando la nueva está completa y confirmada.
- **FR-013**: Cambiar la frase o las partes MUST NOT modificar ni volver a cifrar los secretos guardados.
- **FR-014**: Hécate MUST NOT guardar en ningún sitio la raíz descifrada, la frase ni las partes; la raíz
  descifrada solo existe en memoria mientras está desbloqueada.
- **FR-015**: Hécate MUST registrar en el registro de accesos cada intento de inicializar, desbloquear o sellar (momento, vía, resultado),
  sin incluir nunca la frase ni las partes.
- **FR-016**: Tras tres intentos fallidos seguidos de desbloqueo, Hécate MUST duplicar la espera antes del
  siguiente intento, desde 1 minuto hasta un máximo de 15 minutos; un desbloqueo correcto reinicia la cuenta.
  El contador y la espera se mantienen tras un reinicio, y los fallos con la frase y con las partes suman en el
  mismo contador.
- **FR-017**: Hécate MUST informar de su estado (sin inicializar, sellada o desbloqueada) sin exponer secretos.
- **FR-018**: Antes de inicializar, Hécate MUST advertir de que perder la frase y dos partes hace imposible
  recuperar los datos.
- **FR-019**: La inicialización MUST NOT crear la CA raíz offline; la crea la feature de identidades, junto con
  la CA intermedia y con Hécate ya desbloqueada.

### Key Entities *(include if feature involves data)*

- **Raíz**: clave aleatoria que protege todas las demás claves de Hécate. Solo existe descifrada en memoria.
- **Protección por frase**: la raíz cifrada con una clave derivada de la frase del admin, de forma costosa para
  frenar ataques por fuerza bruta.
- **Partes de recuperación**: tres fragmentos de los que dos reconstruyen la raíz. Cada una incluye una
  comprobación para detectar errores al copiarla.
- **Protección por partes**: la raíz cifrada de forma que se recupera con dos partes.
- **Estado de sellado**: sin inicializar, sellada o desbloqueada, con el contador de intentos fallidos.
- **Registro de accesos**: cada intento de inicializar, desbloquear o sellar, con momento, vía y resultado.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: El admin completa la inicialización, guardando las partes, en menos de 10 minutos.
- **SC-002**: El admin desbloquea Hécate con la frase en menos de 1 minuto desde que la ve sellada.
- **SC-003**: Tras el 100 % de los reinicios, Hécate está sellada.
- **SC-004**: Cualquier combinación de dos partes de las tres desbloquea; una sola parte nunca lo hace.
- **SC-005**: El 100 % de las partes con un carácter mal copiado se detectan antes de contar como intento.
- **SC-006**: Ninguna inspección de lo que Hécate guarda o registra encuentra la raíz descifrada, la frase ni
  las partes.
- **SC-007**: Un atacante con acceso local no puede hacer más de 10 intentos de desbloqueo por hora tras los
  primeros fallos, aunque reinicie Hécate o alterne entre frase y partes.
- **SC-008**: Un corte de luz en cualquier punto de la inicialización, del cambio de frase o del reparto de
  partes deja siempre a Hécate en un estado desde el que se puede continuar o desbloquear.

## Assumptions

- **Solo desde el equipo de Hécate**: el desbloqueo desde Ténaro queda fuera de esta feature, porque Ténaro aún
  no existe. El admin actúa desde el equipo donde corre Hécate, y tener acceso local a ese equipo identifica al
  admin.
- **Fuera de alcance:** el procedimiento manual para volver a empezar tras perder la frase y dos partes (se
  documenta con la operación de Hécate), resolver secretos, la CA raíz, la CA intermedia y los certificados, rotar la raíz, el adapter de
  hardware (TPM o YubiKey) y la firma de checkpoints de auditoría. Son features propias.
- **Registro de accesos** local de Hécate; encadenarlo y firmarlo pertenece a la feature de auditoría.
- **Avisos**: avisar de que Hécate está sellada lo hará un cronjob de Hades consultando su estado; aquí solo se
  expone el estado.
- **Frase mínima de 12 caracteres** y espera creciente de 1 a 15 minutos, como valores razonables para uso
  doméstico.
- **Referencias de diseño**: [docs/hecate.md](../../docs/hecate.md) (arranque y desbloqueo) y las desviaciones de
  Hécate en [DECISIONS](../../DECISIONS.md). Al consolidar esta feature, la inicialización de docs/hecate.md
  deja de incluir la CA raíz offline.
