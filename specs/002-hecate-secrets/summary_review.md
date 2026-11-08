# Guía de revisión: Secretos de Hécate

Rama `feature/002-hecate-secrets`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **La spec** ([spec.md](spec.md)). Cinco historias, 22 requisitos y 8 criterios de éxito. Las aclaraciones de la
   sesión fijan el alcance:
   - la resolución no tiene canal todavía;
   - el admin escribe pero nunca lee un valor, solo ve una huella;
   - destruir exige volver a demostrarlo;
   - la caducidad es solo informativa.
2. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md),
   [contrato](contracts/control-protocol.md)). Event Sourcing de `Secret` y `Key` (R1), material cifrado fuera de los
   eventos (R2), cifrado por capas con datos asociados por capa (R3), huella con clave (R4), gracia calculada al leer
   (R5) y colaboración con el sellado para la prueba de destrucción (R6).
3. **El código nuevo**, en este orden:
   - `hecate/src/hecate/domain/secrets/`: ruta, tipos de valor y el aggregate `Secret` con sus transiciones y su
     máximo de versiones.
   - `hecate/src/hecate/domain/keys/` y `domain/events.py`: la clave de ámbito y la codificación común de eventos.
   - `hecate/src/hecate/application/secrets/envelope.py`: las tres capas de cifrado, sus datos asociados y la huella.
   - `hecate/src/hecate/application/secrets/service.py`: toda la lógica; es el fichero más importante.
   - `hecate/src/hecate/infrastructure/`: tablas `events`, `key_material` y `secret_material` en SQLite, operaciones
     `secrets.*` del canal local y `hecate secrets ...` en la CLI.
4. **Lo que toca código de la feature 001**, que ya está en `main`:
   - `SealingService` expone `root()` y `verify_proof()`;
   - `StateConflict` acepta un mensaje y `current_version` además del estado de sellado;
   - error nuevo `NotFound`;
   - el registro de accesos gana `subject`, `path` y `version`, con migración al arrancar;
   - `CryptoProvider` gana HMAC y HKDF.

   Las pruebas de la 001 no se han debilitado; solo se han añadido.
5. **Los tests:** 548 en total; 380 están en ficheros nuevos y `test_status_op.py` se amplía. Lo más crítico está en
   `tests/integration/`:
   - `test_secret_no_leak.py`: ningún valor ni huella en disco ni en el registro de accesos.
   - `test_secret_crypto_shred.py`: una versión destruida desaparece del fichero.
   - `test_secret_crash.py`: un corte deja el secreto en el estado anterior o en el nuevo.
   - `test_secret_concurrency.py`: dos escrituras a la vez.
   - `test_envelope.py`: propiedades con hypothesis.

## Cómo probarlo

```bash
uv run --package cerbero-hecate pytest hecate/tests
```

El recorrido manual con la CLI real está en [quickstart.md](quickstart.md), con `HECATE_KDF_PROFILE=moderate` para no
esperar unos 22 s en cada derivación de Argon2id.

## Qué hay que saber

- **Nada está en `main`.** Todo está sin commit: spec, plan, código, pruebas y los docs consolidados (`docs/hecate.md`
  y la entrada Fingerprint de `GLOSSARY.md`). Esta feature no cambia la constitución.
- **La resolución es un caso de uso interno**, `resolve(identity, references)`, probado con pruebas unitarias y de
  integración. Su canal autenticado llega con la feature de identidades.
- **`max_versions` va de 2 a 100.** Con 1, rotar destruía en el mismo cambio la versión recién pasada a `deprecated`.
  Solo la versión anterior inmediata tiene garantizada su gracia (SC-005). Si se rota N veces dentro de la gracia con
  un máximo de N, la más antigua se destruye aunque siga en gracia (FR-010).
- **Desviaciones del plan**, ya anotadas en plan, data-model, research y contrato tras el converge:
  - el módulo `domain/events.py`;
  - HMAC de la librería estándar en lugar de `cryptography` (R12);
  - `PRAGMA secure_delete` para el borrado criptográfico;
  - los límites añadidos: valor no vacío, `rotate_every` de 1 a 64 caracteres imprimibles, 100 referencias por
    resolución;
  - `Result.NOT_FOUND` y `Method.IDENTITY` en el registro de accesos;
  - `rotate_every` en la respuesta de `secrets.list`.
- **Los tiempos son de un Mac de desarrollo:** resolver 10 secretos tarda unos 6 ms (SC-001 pide menos de 1 s).
- **El borrado criptográfico se comprueba en los ficheros de SQLite.** Lo que pueda quedar por debajo, en bloques
  libres del sistema de ficheros o del SSD (por ejemplo, del journal de la transacción), queda fuera del alcance de
  la aplicación.
- **La rotación de las claves de ámbito queda fuera de alcance:** cada identidad tiene una sola versión de su clave.
- **Siguen pendientes de la feature 001:** probar en Linux (`SO_PEERCRED`, `mlockall`) y medir en el servidor de casa.
  Python sigue sin borrar la memoria con garantías, un riesgo ya aceptado en DECISIONS.

## Dónde mirar con más sospecha

- **`resolve` en `service.py`:** todo o nada, y un único `not_found` para "no existe", "es de otra identidad" y "sin
  versión `active`". La comprobación de identidad va antes de cargar nada.
  - Los rechazos son idénticos en contenido, pero no se ha medido si lo son en tiempo.
  - Hoy no importa porque no hay canal; importará con la feature de identidades.
- **`destroy` en `service.py`:** decide qué cuenta como intento fallido en el contador común.
  - Comprueba el estado antes que la prueba, para no gastar intentos en una destrucción imposible.
  - Vuelve a comprobarlo dentro de la transacción.
- **Los datos asociados de cada capa (`envelope.py`):** son los que impiden mover un dato cifrado a otra ruta, versión
  o identidad.
- **El registro de accesos:** nunca un valor ni una huella, y solo rutas bien formadas; los argumentos mal formados se
  registran sin su contenido.
- **La CLI:** el valor se lee con `getpass` o con `--from-file`, nunca como argumento, para que no quede en el
  historial del shell.
- **El cierre de la feature:** `docs/hecate.md` ya está consolidado; hay que comprobar que el diseño vigente no
  contradiga lo implementado.

## Después de la revisión

`/speckit-converge` se ha pasado tres veces y la última salió limpia. Quedan los commits por bloques y una PR con
squash merge.
