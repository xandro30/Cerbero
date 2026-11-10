# Guía de revisión: Identidades de Hécate

Rama `feature/003-hecate-identities`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene
saber. Lo que cubre, caso por caso, está en [use_cases.md](use_cases.md).

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Siete historias, 34 requisitos y 18 criterios de éxito. Las 18 aclaraciones fijan
   el alcance:
   - la clave de la CA raíz solo existe en la CLI, y la CA no se activa hasta confirmar su copia;
   - solo la emisión gasta un token de alta;
   - cada certificado se renueva una vez, y un segundo intento revoca con urgencia los de esa identidad;
   - las claves de firma de llamadas se publican solo a componentes autenticados;
   - el certificado de Hécate solo admite la revocación urgente;
   - la clave propia de Hécate se guarda en claro para responder sellada (riesgo aceptado).
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md),
   [contratos](contracts/README.md) y el [ADR 0009](../../docs/adr/0009-pila-api-hecate.md)):
   - la pila: axum sobre rustls (solo TLS 1.3), rcgen y x509-parser, jsonwebtoken con EdDSA y CRL X.509 (R1 a R4);
   - las ceremonias por el canal local (R9) y el formato propio de la copia de la clave raíz, HRK1 (R7);
   - los streams `ca`, `identity/<nombre>` y `certificate/<identidad>/<serie>`, y las tablas nuevas (R10);
   - los secretos en memoria (R14).
3. **El código nuevo**, en `hecate/crates/` y en este orden:
   - `domain/src/ca/` e `identity/`: los aggregates de la CA, la identidad y el certificado, el token de alta, la clave
     de firma de llamadas y el freno de altas, con sus eventos en `events.rs`.
   - `application/src/ca/service.rs`: las tres ceremonias (crear, renovar la intermedia, sustituir), el certificado
     propio de Hécate, las CRL y el retiro de intermedias.
   - `application/src/identity/service.rs`: tokens, alta, renovación, revocaciones, deshabilitar, listados y claves
     publicadas. Es el fichero más importante.
   - `infrastructure/src/`: `pki.rs` (rcgen y la clave raíz de la CLI), `backup.rs` (HRK1), `store/ca.rs` y
     `store/identity.rs`, `api/` (servidor, supervisor, autenticación y rutas), `call_token.rs`, `tasks.rs` y las
     operaciones del canal en `control/operations.rs`.
   - `cli/src/lib.rs`: `ca create`, `ca renew-intermediate`, `ca replace`, `tokens`, `identities`, `certificates` y el
     bloque `ca` de `status`.
   - `infrastructure/src/testing.rs` y `examples/component.rs`: el componente de prueba, que hace de componente real
     contra la API.
4. **Lo que toca código de 001, 002 y 005**, que ya está en `main`:
   - `SealingService` gana `unsealed_epoch()` e `is_unseal_passphrase()`;
   - `secrets.create` y `secrets.add_version` rechazan las identidades reservadas `hecate` y `admin` (FR-027): es el
     único cambio de comportamiento de 001 y 002;
   - el registro de accesos gana las columnas `identity` y `certificate` y los métodos `enrollment_token` e `internal`,
     con la migración del 005 ampliada;
   - el lector de PEM pasa a `domain::pem` y la regla de identidad a `identity::names`, sin cambiar su comportamiento;
   - la configuración gana `HECATE_API_ADDR`, `HECATE_CERT_LIFETIME_HOURS` y `HECATE_ENROLL_TOKEN_MINUTES`.

   No se ha quitado ninguna prueba del 005. Las que cambian se amplían a propósito: la paridad del esquema, los
   literales del registro de accesos y el contrato de `status`.
5. **Los tests:** 754 en total. Siguen las 423 del cambio 005 y se suman 331. Lo más crítico:
   - `infrastructure/tests/identity_no_leak.rs`: ningún token, frase de la CA, clave raíz, copia, clave de intermedia
     ni clave del componente en la base, los logs o las respuestas, en ninguna codificación. La clave propia de Hécate
     solo está en `hecate_tls`.
   - `infrastructure/tests/api_*.rs`: la API con el componente de prueba, cada respuesta validada contra el OpenAPI,
     también con un año de datos.
   - `infrastructure/tests/contract_*_ops.rs`: las operaciones nuevas del canal local.
   - `application/tests/identity_renew.rs`, `identity_revoke.rs`, `ca_replace.rs` y `ca_intermediate.rs`: segundos
     intentos, revocaciones, sustitución en una transacción, cortes de luz y retiro de intermedias.
   - `infrastructure/tests/pki_crl.rs` y `store_ca.rs`: webpki rechaza lo revocado, y una clave destruida no queda en
     el fichero.

## Cómo probarlo

```bash
cargo test --manifest-path hecate/Cargo.toml --workspace
```

La pasada en Linux es la del apartado 3 de [quickstart.md](quickstart.md). Con la VM por defecto de Colima (2 GiB), hay
que añadir `-e CARGO_BUILD_JOBS=1`. El recorrido manual, con el binario release y el ejemplo `component`, está en los
apartados 5 a 9, con `HECATE_KDF_PROFILE=moderate` para no esperar unos 22 s en cada derivación de Argon2id.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, tareas y el ADR 0009;
  - el código y las pruebas;
  - los OpenAPI en `shared/contracts/src/cerbero_contracts/openapi/`;
  - los docs consolidados (`docs/hecate.md`, Identidad y seguridad en `ARCHITECTURE.md`, `GLOSSARY.md`,
    `docs/hermes.md`, `DECISIONS.md` y `shared/contracts/README.md`).

  La fila de la 003 ya no está en «Cambios pausados». La constitución no cambia.
- **Resultados:** 754 pruebas en macOS y las mismas en un contenedor Linux (`rust:1.99.1-bookworm`), con el código de
  la fase 16. El recorrido de los apartados 5 a 9 del quickstart dio lo esperado en sus 42 comprobaciones. Las
  comprobaciones de calidad pasan sin avisos, y el árbol normal no compila TLS 1.2.
- **Tus decisiones durante la implementación:**
  - features mínimas (axum sin las de por defecto, jsonschema solo con `resolve-file`), rustls con `prefer-post-quantum`
    y MIT-0 en la lista de licencias de `deny.toml`;
  - descifrar la clave de la intermedia en cada operación que firma;
  - un supervisor que arranca, cambia o para la API;
  - anular un token que ya no está pendiente se acepta sin eventos;
  - la repetición exacta de un alta deja una fila `repeat`;
  - el `subject` del alta es el SPIFFE ID;
  - una copia de la clave raíz que no se abre aborta con `wrong_passphrase`, sin distinguir una frase mal escrita de
    la copia de otra CA;
  - las filas de los rechazos anteriores al caso de uso las escribe un registrador en el middleware, sin cambiar su
    precedencia (fase 13); en un alta, ese registrador pasa por el freno y puede responder `rate_limited` (fase 14);
  - una ruta que no existe y un método que la ruta no admite responden `not_found` (404) con el sobre, sin fila
    (fase 15).
- **Desviaciones y aclaraciones**, ya anotadas en tareas, contratos, data-model y quickstart:
  - tareas reescritas: T014, T023, T032, T039, T040, T061, T076, T091 y T098;
  - en una CRL, `unspecified` va sin extensión de motivo (RFC 5280 §5.3.1);
  - `reissued` siempre está en `certificates.revoke`, a `null` salvo con el certificado vigente de Hécate;
  - la prueba de `ca.replace.begin` va en `proof`, como en `secrets.destroy`;
  - una deshabilitación urgente deja una fila `certificate_revoke` por serie;
  - una petición que la API rechaza antes de su caso de uso deja la fila de su operación, sin `subject`: con Hécate
    sellada, `rejected_conflict` y `detail: sealed`; con un `X-Request-Id` mal formado, `invalid_input` y
    `detail: request_id`. En un alta, ese `X-Request-Id` pasa por el freno como un `Idempotency-Key` mal formado, y
    un `HEAD` a una ruta `GET` cuenta como esa operación (data-model, fases 13 y 14);
  - el quickstart corrige su §7.2: tras sustituir la CA, un componente que fija la raíz anterior no llega a Hécate.
- **Los tiempos son de un Mac de desarrollo:** con un año de datos (unos 5.400 certificados), `GET /v1/keys` y `GET
  /v1/revocations` responden en menos de 1 s, y un alta tras sustituir la CA también (SC-010). En el servidor de casa
  siguen sin medirse, como en 001 y 002.
- **Fuera de alcance**, como fija la spec:
  - la Access Policy y la resolución por el canal autenticado, ahora en «Abiertas» de DECISIONS;
  - las identidades hijas, `wrap` / `unwrap`, la firma de checkpoints y el desbloqueo desde Ténaro;
  - el adapter de hardware y los avisos;
  - el lado de cada componente, que se prueba con el componente de prueba.

  Orthros no cambia.
- **Visto y sin cambiar:** tras sustituir la CA, `identities get` muestra los certificados de la CA anterior con su
  propio estado, `valid` hasta que caducan, y `renewable no`, sin decir que son de la generación anterior.

## Dónde mirar con más sospecha

- **El alta (`enroll` y `enroll_checked` en `identity/service.rs`):**
  - el orden de las comprobaciones decide qué cuenta para el freno: sellada y frenada no cuentan, y los errores
    internos tampoco;
  - solo la emisión gasta el token;
  - la repetición se reconoce por un resumen del cuerpo, y la `Idempotency-Key` se comprueba dentro de la identidad.
- **La autenticación (`authenticate`):**
  - la conexión TLS solo comprueba que el cliente posee la clave de su certificado; la cadena la valida la aplicación,
    a la hora del reloj, para que todo rechazo sea `auth` y deje fila (SC-006);
  - jsonwebtoken solo comprueba el algoritmo y la firma; tiempos, `iss`, `aud` y `jti` van contra el reloj;
  - conviene comprobar que ninguna ruta salvo el alta se salta `CallCredentials`;
  - los rechazos anteriores al caso de uso (Hécate sellada, `X-Request-Id` mal formado) llegan al registrador que
    reciben la guarda y `request_context`, que responde lo que diga el freno en un alta; `operation_of`, en
    `routes.rs`, traduce método y ruta a la operación, con `HEAD` como `GET`, así que tiene que seguir a las rutas.
- **La renovación (`renew_checked` y `revoke_identity`):**
  - un segundo intento escribe `SuspiciousRenewalDetected` y revoca con urgencia, en una transacción, todos los
    certificados válidos de la identidad;
  - dos renovaciones a la vez no se bloquean: la que pierde vuelve a leer y se convierte en un segundo intento.
- **La sustitución de la CA (`replace_begin` y `replace_confirm`):**
  - qué se comprueba antes de la prueba, para no gastar intentos;
  - la comparación en tiempo constante de la frase nueva con la prueba;
  - la transacción única que retira las intermedias, borra su material, escribe `CaReplaced` y emite el certificado
    nuevo de Hécate bajo el cerrojo de su renovación.
- **El certificado de Hécate:** `replace_own_certificate` lo revoca y lo reemite en una transacción, y el aviso
  `changed` hace que la API lo presente al momento.
- **El retiro de intermedias (`retire_intermediates` y `next_retirement`):** la tarea de cada minuto destruye la clave
  anterior en el instante en que caduca su último certificado, también sellada.
- **La memoria:**
  - la frase de la CA, la clave raíz, las claves de las intermedias y los tokens viven en tipos que se borran;
  - `hecate tokens create` ahora escribe el token desde un búfer que se borra;
  - las copias dentro de aws-lc-rs y rustls quedan en el riesgo residual ya aceptado.
- **Las ceremonias de la CLI:** la copia se escribe con `0600`, se comprueba antes de confirmar y se borra al abortar.
  La clave raíz nunca cruza el canal.
- **El cierre de la feature:** los docs ya están consolidados; hay que comprobar que el diseño vigente no contradiga lo
  implementado.

## Después de la revisión

`/speckit-converge` se ha pasado una vez y añadió la fase 11 (T122 a T125), ya implementada:
- pruebas de corte de luz para la renovación de la intermedia, la renovación programada del certificado de Hécate y el
  retiro de una intermedia;
- una prueba de que la CRL recoge los certificados revocados por un segundo intento;
- la clave de la intermedia anterior se destruye en el instante en que caduca su último certificado;
- la tabla de errores del contrato recoge el rechazo de una CA raíz de sustitución igual a la vigente.

La segunda pasada añadió la fase 12 (T126), ya implementada: tras un retiro que falla, el temporizador del retiro no se
vuelve a armar al momento, y el siguiente intento queda para la siguiente pasada de la tarea.

La tercera pasada añadió la fase 13 (T127 y T128), ya implementada:
- los rechazos de la API anteriores al caso de uso (Hécate sellada y un `X-Request-Id` mal formado) dejan la fila de
  su operación, como pide FR-024;
- la suite pasa también en el contenedor Linux con el código vigente.

La cuarta pasada añadió la fase 14 (T129 y T130), ya implementada:
- el rechazo de un alta por un `X-Request-Id` mal formado pasa por el freno, como el de un `Idempotency-Key` mal
  formado: cuenta como fallo y, con las altas frenadas, responde `rate_limited` y cuenta en la tanda sin fila propia
  (FR-034, SC-017);
- `operation_of` reconoce las peticiones `HEAD`, que axum atiende con las operaciones `GET` (FR-024);
- la suite pasa en macOS y en el contenedor Linux con el código de la fase 14.

La quinta pasada añadió la fase 15 (T131), ya implementada: una ruta que no existe o un método que la ruta no admite
ya no reciben la respuesta vacía de axum, sino `not_found` con el sobre común, como pide el principio II (la fila
`not_found` de la tabla de errores de contracts/api.md lo recoge).

La sexta pasada recorrió la API capa a capa y añadió la fase 16 (T132), ya implementada: un `kid` que no es UTF-8
válido una vez descodificado (`%FF`) ya no lo rechaza el extractor de axum con un `400` de texto, sino que llega vacío
al caso de uso, que autentica primero y responde `validation` con su fila. Era el último rechazo de la API fuera del
sobre y del caso de uso.

La séptima pasada salió limpia. La propuesta del mensaje del commit de squash y de la descripción de la PR (T121) ya
está hecha; el commit, el push y la PR los haces tú.
