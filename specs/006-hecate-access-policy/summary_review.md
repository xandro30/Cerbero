# Guía de revisión: Access Policy de Hécate y resolución de secretos

Rama `feature/006-hecate-access-policy`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene
saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Tres historias (resolver sus secretos, compartir un secreto, consultar el estado),
   23 requisitos y 10 criterios de éxito. Las 9 aclaraciones fijan el alcance:
   - el estado responde también con Hécate sellada;
   - una excepción nombra un secreto concreto que exista al concederla, sin prefijos;
   - los cambios de política son órdenes sin versión esperada;
   - solo los rechazos del estado dejan fila;
   - `rotate_every` pasa a ser una duración ISO 8601, el único cambio de comportamiento de la 002 (FR-023);
   - una resolución rechazada deja una fila, con la primera referencia que falla.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md),
   [contratos](contracts/README.md) y el [ADR 0010](../../docs/adr/0010-access-policy-hecate.md)):
   - la resolución por la API y dónde vive (R1);
   - los tiempos de los rechazos: dos fases y un mínimo de 250 ms para `not_found` (R2);
   - el aggregate de la política en `policy/<identidad>` (R3) y el estado para los componentes (R4);
   - `rotate_every` como duración (R5) y la memoria de los valores resueltos (R6).
3. **El código nuevo**, en `hecate/crates/` y en este orden:
   - `domain/src/access_policy/policy.rs` y `PolicyEvent` en `events.rs`: la política, sus reglas y su historial;
     `domain/src/secrets/rotation.rs`: `RotationInterval`.
   - `application/src/access_policy/service.rs`: las órdenes y consultas del admin y `policy_of`.
   - `application/src/secrets/service.rs`: `resolve_for`, la resolución en dos fases con el contexto de quien llama, y
     el recuento de secretos por rotar en `expiry_counts`.
   - `application/src/identity/service.rs`: `resolve` y `status` para los componentes. Es el fichero más importante,
     junto con el anterior.
   - `infrastructure/src/api/`: `routes.rs` (la resolución con su mínimo, el estado, `answers_while_sealed` y
     `operation_of`), `envelope.rs` (la respuesta en un búfer que se borra) y `server.rs` (la guarda de sellada por
     método y ruta).
   - `infrastructure/src/control/operations.rs`: las operaciones `policies.*` y `secrets_due_rotation` en `status`.
   - `cli/src/lib.rs`: `hecate policies ...` y la línea `secrets due rotation: <n>` de `hecate status`.
   - `infrastructure/src/testing.rs` y `examples/component.rs`: `resolve` y `status` del componente de prueba.
4. **Lo que toca código de 001, 002, 003 y 005**, que ya está en `main`:
   - `secrets.create` solo acepta un `rotate_every` que sea una duración en días o semanas (FR-023); las pruebas de la
     002 que usaban `"90d"` y `"30d"` pasan a `"P90D"` y `"P30D"`;
   - la resolución de la 002 (`resolve(identidad, referencias)`) pasa a envolver `resolve_for`, con las mismas filas;
   - `ExpiryCounts` gana `secrets_due_rotation`, y el `status` del canal local y `hecate status` lo muestran;
   - `SealedCheck` recibe el método y la ruta, y `routes::sealed_check` la usan `hecate serve` y los ayudantes de prueba;
   - `IdentityService::new` recibe la política y los secretos, e `IdentityOperations` gana `policies`;
   - el registro de accesos gana las operaciones `policy_grant`, `policy_revoke`, `policy_list`, `policy_get` y
     `status_get`, sin migración (la columna no tiene `CHECK`);
   - la lista de `GET /v1/capabilities` gana `secrets:resolve` y `status`.

   No se ha quitado ni rebajado ninguna prueba anterior. Lo que cambia en sus ficheros:
   - se amplían a propósito el contrato de `status`, la salida de `hecate status`, la lista de operaciones de la API y
     los `rotate_every` de la 002;
   - se añaden pruebas nuevas junto a las de siempre: la validación de `rotate_every` y el recuento de los secretos por
     rotar (`secrets_create.rs`, `secrets_list_get.rs`), la resolución en dos fases (`secrets_resolve.rs`), los
     esquemas del OpenAPI (`contract_schema.rs`), el esqueleto de la API (`api_server.rs`), la primera fase
     (`performance.rs`), las operaciones del registro (`store_access_log.rs`), las fugas de la resolución
     (`identity_no_leak.rs`) y el error de `--rotate-every` (`cli_secrets.rs`);
   - cambia cómo se construyen la guarda de sellada y los servicios en los ayudantes de prueba y en `api_server.rs`,
     `api_identity.rs` e `identity_no_leak.rs`, no lo que comprueban;
   - dos comentarios, en `secret_no_leak.rs` y `api_identity.rs`, describen ya la resolución por la API y el estado
     sellado (T056).
5. **Los tests:** 840 en total; siguen las 754 de la feature 003 y se suman 86. Lo más crítico:
   - `infrastructure/tests/api_resolve_timing.rs`: SC-003, 1.000 peticiones por grupo intercaladas; los tres rechazos
     y los de 10 referencias no se distinguen por su mediana.
   - `infrastructure/tests/performance.rs`: la primera fase con 100 referencias, por debajo de 50 ms sobre SQLite.
   - `infrastructure/tests/api_resolve.rs` y `api_status.rs`: la API con el componente de prueba, cada respuesta
     validada contra el OpenAPI, también las de error y la del estado sellada.
   - `infrastructure/tests/identity_no_leak.rs`: ningún valor resuelto ni su huella en las filas, los logs ni las
     respuestas.
   - `application/tests/identity_resolve.rs`: todo o nada, las filas, la política leída una vez y Hécate sellada entre
     las dos fases.
   - `application/tests/access_policy.rs`, `identity_status.rs` y `infrastructure/tests/contract_policy_ops.rs`: las
     órdenes, sus filas, una carrera entre dos órdenes, un corte de luz y el permiso de estado.

## Cómo probarlo

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

La pasada en Linux es la del apartado 3 de [quickstart.md](quickstart.md), con `-e CARGO_BUILD_JOBS=1` en la VM por
defecto de Colima. El recorrido manual, con el binario release y el ejemplo `component`, está en los apartados 5 a 7,
con `HECATE_KDF_PROFILE=moderate`. `hecate init` pide escribir `yes` y confirmar dos de las partes, como en la 001.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, tareas, checklists y el ADR 0010;
  - el código y las pruebas;
  - el OpenAPI de Hécate en `shared/contracts/src/cerbero_contracts/openapi/hecate.v1.json`;
  - los docs consolidados: `docs/hecate.md`, `GLOSSARY.md`, la línea de acceso de Hécate en `ARCHITECTURE.md`,
    `DECISIONS.md` (la decisión sale de «Abiertas», pasa a «Tomadas» con el ADR 0010 y gana una desviación frente a
    OpenBao / Vault), el índice de ADR, la fila de `hecate.v1.json` en `shared/contracts/README.md` y la línea de
    avisos de `docs/hades.md`.
- **Resultados:** 840 pruebas en macOS y las mismas en un contenedor Linux (`rust:1.99.1-bookworm`), también las de
  tiempos y rendimiento, con el código final de la fase 9. Las comprobaciones de calidad (fmt, clippy y cargo-deny)
  pasan sin avisos y sin dependencias nuevas. El recorrido de los apartados 5 a 7 del quickstart dio lo
  esperado, y la orden del apartado 9 no imprime nada.
- **Tus decisiones durante la implementación:**
  - la lista de políticas muestra las identidades cuya política ha cambiado alguna vez, también sin nada concedido;
  - el estado de la API llama `enrollment_throttled` al freno de altas, como el canal local;
  - la línea de la CLI es `secrets due rotation: <n>`, con dos puntos.
- **Decisiones mías durante la implementación:**
  - el OpenAPI gana las respuestas `SecretNotFound` y `StatusAuth`, para que no cambien los textos de las de la 003, y
    los tiempos de caché van solo en las descripciones;
  - cada secreto resuelto lleva su referencia tal como se pidió, y la primera fase carga también la clave de ámbito
    envuelta del dueño;
  - `grant_read` comprueba que el secreto existe solo cuando la orden cambiaría algo;
  - solo las operaciones `policies.*` rechazan los argumentos que faltan o sobran;
  - la prueba de fugas busca las huellas en las filas, los logs y las respuestas, no en los ficheros de la base, porque
    los eventos de cada secreto las guardan desde la 002; a cambio comprueba que las resoluciones no escriben ningún
    evento (T051).
- **Lo que encontró la fase 6:**
  - **memoria (T040):** la caché de claves de ámbito de la resolución podía crecer con secretos de varios dueños y dejar
    copias de las claves en la memoria que libera. Ahora tiene sitio para una clave por referencia y no crece. No tiene
    prueba propia: verlo pide un asignador propio, y el código no admite `unsafe`;
  - **recorrido (T045):** con Hécate sellada, las filas de `policies.*` no llevaban la identidad ni la ruta, aunque el
    contrato pide la identidad en todas. Ahora la llevan, con pruebas en `access_policy.rs` y `contract_policy_ops.rs`.
- **Lo que encontró converge (fase 7):** nada sin construir; solo pruebas que faltaban, que pasaron a la primera porque
  el código ya lo cumplía:
  - el estado por la API con renovaciones sospechosas, días de la intermedia, altas frenadas e identidades por caducar
    (T048). Con dos campos cruzados a propósito en `routes.rs`, solo falla esta prueba;
  - un secreto con un `rotate_every` antiguo se sigue resolviendo (T049);
  - el error de `--rotate-every` en la CLI (T050);
  - las resoluciones no escriben eventos (T051). Con un `create` plantado entre las dos lecturas, la comprobación falla.
- **La memoria de los valores resueltos:** el valor sale del descifrado en tipos que se borran, se serializa en un
  búfer de tamaño exacto y se borra cuando hyper suelta el cuerpo, también si la petición falla. Quedan en el riesgo
  residual ya aceptado las copias de rustls al cifrar y las de h2, que copia a su búfer los datos de menos de 256 bytes.
  En HTTP/1.1, hyper encola el cuerpo sin copiarlo.
- **Desviaciones y aclaraciones**, ya anotadas en tareas, contratos y quickstart:
  - las pruebas de la 002 con `"30d"` (`contract_secret_ops.rs` y `cli_secrets.rs`) también pasan a `"P30D"`;
  - `docs/hecate.md` describe el estado con los campos del contrato, sin el resultado de la verificación de la
    auditoría, cuya señal sigue en la verificación periódica.
- **Los tiempos son de un Mac de desarrollo:**
  - la prueba de SC-003 tarda unos 37 s;
  - en el recorrido, los tres rechazos tardan unos 310 ms, con el arranque del componente.

  Con el almacén en memoria, la diferencia de trabajo entre los tipos de rechazo es de menos de 1 ms, así que el
  margen de SC-003 solo detecta fugas mayores. La garantía la dan el mínimo de 250 ms y el límite de la primera fase.
  En el servidor de casa siguen sin medirse, como en las features anteriores.
- **Fuera de alcance**, como fija la spec:
  - la firma de checkpoints, `wrap` / `unwrap`, la rotación de las claves de ámbito y las identidades hijas;
  - los permisos que salen de las declaraciones de Hermes y Hades;
  - Ténaro, el adapter de hardware, los avisos y la rotación automática de secretos;
  - OpenTelemetry, que sigue en su excepción;
  - el lado de cada componente, que se prueba con el componente de prueba.

  Orthros no cambia.

## Dónde mirar con más sospecha

- **La resolución en dos fases (`resolve_checked`, `check` y `open` en `secrets/service.rs`):**
  - la primera fase decide todo `not_found` sin descifrar nada;
  - antes de descifrar se vuelve a pedir la raíz: si Hécate se selló entretanto, responde `unavailable` con una fila;
  - un secreto compartido se descifra con la clave de ámbito de su dueño.
- **El mínimo de los rechazos (`resolve` en `routes.rs`):**
  - solo espera ante `not_found`, desde que la petición llega a la operación;
  - el sobre se construye después de la espera, así que `duration_ms` no delata el trabajo.
- **La respuesta que se borra (`wiped_json` y `completed_owned` en `envelope.rs`):** las dos pasadas tienen que
  escribir lo mismo para que el búfer no crezca.
- **El estado (`status` en `identity/service.rs` y `answers_while_sealed` en `routes.rs`):**
  - es la única ruta que la guarda de sellada deja pasar;
  - autentica antes de mirar el permiso;
  - `operation_of` tiene que seguir a las rutas.
- **Las órdenes del admin (`access_policy/service.rs`):**
  - sin versión esperada, vuelven a leer si otra se adelanta;
  - la existencia del secreto se comprueba dentro de esa lectura;
  - la fila toma la identidad y la ruta de los argumentos.
- **`RotationInterval::parse`:** comprueba los dígitos antes de convertir, porque `u32::from_str` admite un `+`.

## Después de la revisión

El primer `/speckit-converge` no encontró nada sin construir ni nada que contradiga la spec. Añadió la fase 7 (T048 a
T052), que ya está hecha: ver «Lo que encontró converge». El apartado 8 del quickstart ya dice dónde está la huella.

El segundo añadió la fase 8 (T053 y T054), ya hecha: dos docs que la consolidación no recogió, la fila de
`hecate.v1.json` en `shared/contracts/README.md` y el nombre del permiso de estado en `docs/hades.md`.

El tercero añadió la fase 9 (T055), ya hecha: la ayuda de `--rotate-every` en la CLI ponía `90d` de ejemplo y lo
llamaba informativo.

El cuarto añadió la fase 10 (T056), ya hecha: dos comentarios de pruebas anteriores decían que la resolución no tiene
canal y que, sellada, toda la API responde `unavailable`.

El quinto no encontró nada: la implementación cumple la spec, el plan y las tareas. Con T047, el mensaje del commit
de squash y la descripción de la PR, están hechas las 56 tareas.
