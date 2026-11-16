# Guía de revisión: lista de acceso por app

Rama `feature/009-app-access`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Tres historias (el admin concede y retira, una app consulta su lista, Hermes
   atiende solo a quien tiene acceso), 13 requisitos y 7 criterios de éxito. Las 5 aclaraciones fijan el alcance:
   - el acceso se deniega por defecto y la feature no deja ningún acceso concedido de partida;
   - la lista es entre apps: una identidad hija cuenta como su app, y lo que alcanza dentro es perfilado, fuera;
   - una concesión vale en la llamada siguiente y una retirada en 240 s como mucho;
   - sin ninguna copia de su lista, Hermes no está listo;
   - se puede conceder a una identidad que aún no se ha dado de alta, como en la Access Policy.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md) y
   [contratos](contracts/README.md)). No hay ADR nuevo: la decisión es la del
   [ADR 0013](../../docs/adr/0013-acceso-por-app.md). Lo que decidiste al revisar el plan:
   - la lista de cada app es un aggregate nuevo, App Access, y no una parte de la Access Policy (R1);
   - la regla de las identidades hijas queda como regla de diseño, porque las hijas no están construidas (R10).

   Lo demás: las órdenes del admin (R2), la consulta `GET /v1/callers` (R3), la copia en Hermes (R4), la comprobación
   tras el token y antes del `jti` (R5), las cinco operaciones del registro (R6) y las versiones de los contratos (R7).
3. **El código nuevo de Hécate**, en `hecate/crates/` y en este orden:
   - `domain/src/app_access/access.rs` y `AccessEvent` en `events.rs`: el aggregate, sus reglas, su historial y la
     lista en un instante pasado;
   - `application/src/app_access/service.rs`: las órdenes y consultas del admin, con sus filas, y `access_of`;
   - `application/src/identity/service.rs`: `callers`, la consulta de una app autenticada;
   - `infrastructure/src/api/routes.rs`: la ruta, `operation_of` y `callers` en las capacidades;
   - `infrastructure/src/control/operations.rs`: las operaciones `access.*` del canal local;
   - `cli/src/lib.rs`: `hecate access grant|revoke|list|get`;
   - `infrastructure/src/testing.rs` y `examples/component.rs`: `callers` del componente de prueba.
4. **El código nuevo de Hermes**, en `hermes/src/hermes/`:
   - `application/ports.py`: `AccessList`, dentro de `TrustMaterial`, y la consulta, la copia y la búsqueda en los
     puertos;
   - `application/identity.py`: la copia, que llega con las publicaciones, se exige al arrancar y se vuelve a pedir
     ante quien no está, con una sola petición a la vez. Es el fichero más importante, junto con el siguiente;
   - `application/authentication.py`: `_allowed`, después del token y antes de guardar el `jti`;
   - `application/health.py` y `infrastructure/main.py`: el motivo `access list not loaded`;
   - `infrastructure/hecate/client.py` y `identity_store.py`: `GET /v1/callers` y `access.json`.
5. **Lo que toca código de las features anteriores**, que ya está en `main`:
   - `IdentityService::new` de Hécate recibe `AppAccessService`, e `IdentityOperations` gana `access`;
   - la lista de operaciones de `GET /v1/capabilities` gana `callers`, antes de `capabilities`, el orden del contrato;
   - el registro de accesos de Hécate gana `access_grant`, `access_revoke`, `access_list`, `access_get` y
     `callers_list`, sin migración;
   - `hecate.v1.json` pasa a la 1.1.0 con `listCallers`, y `hermes.v1.json` a la 1.0.1, con las descripciones del 401 y
     del 503;
   - en Hermes, `TrustMaterial` lleva la lista; `material()` no da confianza sin ella, y el arranque y la salud la
     exigen.

   No se ha quitado ni rebajado ninguna prueba anterior. Lo que cambia en sus ficheros:
   - se amplían a propósito la versión de los contratos, la lista de operaciones de las capacidades
     (`contract_schema.rs` y `api_revocations.rs`) y los motivos de la salud de Hermes (`test_health.py`);
   - en Hermes, las pruebas que construyen el autenticador conceden antes el acceso a quien llama (`hades`), y
     `prepare_identity` guarda una copia vacía, como la deja un alta, para que `serve` arranque con Hécate inalcanzable;
   - el E2E concede el acceso a `caller` y a `caller-two` antes de arrancar Hermes, y la prueba del certificado revocado
     espera, en la misma espera, a que también valga un acceso retirado.
6. **Las pruebas:** 875 en Hécate (siguen las 841 de la 008 y se suman 34) y 866 en Hermes (siguen las 835 de la 008 y
   se suman 31). Lo más crítico:
   - `hecate/crates/domain/src/app_access/access.rs`: las reglas y el proptest sobre secuencias de órdenes;
   - `hecate/crates/application/tests/app_access.rs` y `infrastructure/tests/contract_access_ops.rs`: las órdenes, sus
     filas, las repetidas, Hécate sellada, un corte de luz y una carrera entre dos órdenes;
   - `hecate/crates/infrastructure/tests/api_callers.rs`: cada app recibe la suya, nada pide la de otra, y los
     rechazos dejan su fila;
   - `hermes/tests/unit/application/test_authentication.py` y `test_publications.py`: quien está entra sin preguntar,
     quien no está recibe `auth` tras volver a pedir la lista, una concesión vale en la llamada siguiente, las
     búsquedas a la vez comparten una petición y una hora sin Hécate no cambia nada (SC-004);
   - `hermes/tests/integration/api/test_auth_routes.py`: cada ruta responde `auth` a quien no está, sin llegar al
     proveedor, y la línea `not_allowed` lleva el `request_id` de la respuesta;
   - `hermes/tests/e2e/test_flow.py`: `caller-three` recibe `auth` hasta que se le concede, y una retirada vale en el
     siguiente refresco.

## Cómo probarlo

Las órdenes del apartado 1 de [quickstart.md](quickstart.md): la suite de Hécate en macOS y en el contenedor
`rust:1.99.1-bookworm` (con `-e CARGO_BUILD_JOBS=1` en la VM por defecto de Colima), sus comprobaciones de calidad, y en
Hermes ruff, el formato, `mypy --strict`, pip-audit y la suite entera:

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

Los recorridos manuales están en los apartados 2 a 4, con `HECATE_KDF_PROFILE=moderate` para Hécate. El apartado 4
usa el compose de `hermes/deploy/`; para no depender de Telegram, se puede seguir con el fichero de pruebas
(`compose.test.yaml`) y el proveedor fake, como el E2E.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit:
  - spec, plan, tareas, checklist y esta guía;
  - el código y las pruebas de Hécate y de Hermes;
  - los OpenAPI de `shared/contracts/` y la fila de `hecate.v1.json` en `shared/contracts/README.md`;
  - los docs consolidados: `docs/hecate.md`, `docs/hecate/acceso.md`, `integracion.md`, `cli.md` y `auditoria.md`,
    `docs/hermes.md`, `ARCHITECTURE.md`, `GLOSSARY.md`, `DECISIONS.md`, `README.md` y las órdenes del ejemplo en
    `hecate/README.md`.
- **Resultados:**
  - Hécate: 875 pruebas en macOS y en el contenedor Linux; fmt, clippy con los avisos como errores y cargo-deny, sin
    avisos ni dependencias nuevas;
  - Hermes: 866 pruebas con el binario release de Hécate, Docker y el E2E, sin ninguna saltada; ruff, el formato,
    `mypy --strict` y pip-audit, sin vulnerabilidades;
  - los recorridos de los apartados 2 (12/12), 3 (8/8) y 4 (16/16) dieron lo esperado; ver «Lo que se vio en los
    recorridos».
- **Lo que se vio en los recorridos:**
  - **apartado 2**, con el binario release:
    - las salidas de [contracts/cli.md](contracts/cli.md);
    - los tres `validation`: `the app 'hermes' is not granted access to itself`, `the identity 'admin' is reserved` y
      la regla de nombres para `hades/x`;
    - con Hécate sellada, `conflict` en las cuatro órdenes;
    - doce filas `access_*` con `subject: admin`; la de `hades/x` no lleva la app, porque no está bien formada;
  - **apartado 3**, con el binario release y el ejemplo `component` dado de alta como `hermes`:
    - `{"app":"hermes","callers":[],"version":0}` y, tras conceder, `["caller"]` en la versión 1;
    - los argumentos de más no cambian la respuesta;
    - con Hécate sellada, `unavailable`, y una sola fila `callers_list` (`sealed`);
  - **apartado 4**, con el compose de `hermes/deploy/` y el proveedor fake de `compose.test.yaml`:
    - la copia inicial de Hermes, vacía y en la versión 0, con permisos 0600;
    - `caller` recibe 401 antes de conceder y envía en la llamada siguiente a la concesión;
    - `other` recibe 401, y el log dice `not_allowed` con el `request_id` de la respuesta;
    - con Hécate sellada, `caller` sigue enviando y `other` recibe 503;
    - la retirada valió a los 226 s, sin reiniciar Hermes;
    - sin `access.json` y con Hécate parada, `not ready: access list not loaded` (salida 1), y los envíos, 503.
- **Tus decisiones durante la implementación:**
  - las filas de `callers_list` siguen el patrón de la 003: un rechazo de la autenticación lleva el SPIFFE ID, la
    identidad y la serie si se conocen, con `list` en `detail`; los rechazos de antes de la operación (`sealed`,
    `request_id`) no nombran a nadie;
  - aprobaste el MVP (US1), US2 y US3 en sus puntos de revisión.
- **Decisiones mías durante la implementación:**
  - las pruebas de la CLI de Hécate corren dentro del proceso, como las de `policies.rs`, aunque T012 decía
    assert_cmd;
  - una orden repetida devuelve `changed: false` y la versión de la lista; las órdenes sin versión esperada vuelven a
    leer hasta tres veces si otra se adelanta;
  - `GET /v1/callers` ignora el query string, como el resto de operaciones; `/v1/callers/<app>`, `/v1/callers/` y
    `POST /v1/callers` responden `not_found`;
  - en Hermes, la lista llega en el mismo refresco que la lista de revocación y las claves, todo o nada: si falla una
    de las tres peticiones, sigue la copia anterior de las tres;
  - nada recuerda a quien no tiene acceso: cada llamada suya vuelve a pedir la lista, una petición a la vez (R5);
  - cuando Hécate no responde a la búsqueda, el 503 dice `hecate does not answer about the access list`, como el de
    las claves (`hecate does not answer about the key`). Eso le dice a quien llama que no estaba en la copia; si
    prefieres un mensaje genérico, es una línea;
  - en `DECISIONS.md`, la referencia del principio VII son las políticas de autorización de Istio, con cuatro
    desviaciones: dónde vive y quién aplica la lista, denegar sin ninguna política, el acceso a la app entera y cómo se
    propagan los cambios. Las dos primeras están contrastadas con la documentación oficial de Istio: la política la
    aplica el proxy del servidor, y un servicio sin políticas `ALLOW` acepta cualquier petición.
- **Desviaciones y aclaraciones:**
  - la fase roja de US3 fue en dos pasos: primero fallaban al importar `AccessList`, que aún no existía; declarados los
    puertos de T028, fallaban por la conducta (29 fallos y 7 errores);
  - T021 no tocó `hermes/tests/support/network_api.py`, porque usa `AcceptingAuthenticator`, que no consulta ninguna
    lista; sí cambiaron `support/identity.py` y, para el E2E, `support/compose.py`;
  - T024 gana una prueba más: el acceso solo se mira tras el certificado y el token, así que si fallan no se pregunta a
    Hécate;
  - `shared/contracts/README.md` no nombra versiones, pero su fila de `hecate.v1.json` describe lo que fija, y gana la
    lista de acceso;
  - el ADR 0013 sigue diciendo que, hasta la 009, Hermes no comprueba el acceso: no lo toqué, porque un ADR registra la
    decisión en su fecha.
- **Los tiempos son de un Mac de desarrollo:**
  - la suite de Hermes tarda unos 8 min y medio, con el E2E, que espera un refresco de 240 s como mucho;
  - la pasada de Hécate en el contenedor Linux, con un solo hilo de compilación, tarda unos 24 min, 23 de ellos
    compilando;
  - en el recorrido del apartado 4, la retirada valió a los 226 s.
- **Fuera de alcance**, como fija la spec:
  - los permisos por función, el perfilado de lo que alcanza cada identidad dentro de una app, y los usuarios y roles
    de las personas;
  - las identidades hijas, la comprobación en otras apps y el acceso a la propia API de Hécate;
  - la reestructuración de la documentación de Hermes, que es el cambio siguiente.

  Orthros no cambia, y en la 009 no se ha tocado `hermes/deploy/`.

## Dónde mirar con más sospecha

- **Las órdenes del admin (`app_access/service.rs`):**
  - sin versión esperada, vuelven a leer si otra se adelanta (`change` y `MAX_RACES`);
  - la fila toma la app y quien llama de los argumentos, solo si están bien formados, también con Hécate sellada.
- **La consulta de una app (`callers` en `identity/service.rs`):**
  - Hécate sellada responde `unavailable` antes de autenticar, con su fila `sealed`;
  - una consulta atendida no deja fila;
  - `operation_of` tiene que seguir a la ruta.
- **La búsqueda en Hermes (`lookup_access` en `identity.py`):**
  - las llamadas que llegan a la vez esperan la misma petición, y la siguiente vuelve a preguntar;
  - un fallo de Hécate es `unavailable` y no cambia la copia;
  - un refresco y una búsqueda a la vez pueden dejar la copia del primero que respondió. Una concesión perdida así se
    recupera en la llamada siguiente, que vuelve a preguntar; una retirada vale en el refresco siguiente, dentro de los
    240 s.
- **El orden de la autenticación (`authenticate` en `authentication.py`):** `_allowed` va después del token y antes de
  `remember`, para que quien no tiene acceso no deje `jti`; su `unavailable` no pasa por `_Refused`.
- **La copia al arrancar (`ensure_publications` y `material`):** sin `access.json`, el arranque vuelve a pedir las tres
  publicaciones, y la salud dice `access list not loaded` mientras no llegan.

## Después de la revisión

El primer `/speckit-converge` no encontró nada: la implementación cumple la spec, el plan y las tareas. Queda T037, el
mensaje del commit de squash y la descripción de la PR, como en las features anteriores.
