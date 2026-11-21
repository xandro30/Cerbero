# Hermes: librerías de terceros con contrato

Hermes reúne las librerías que envuelven servicios de terceros detrás de contratos estándar por tipo de servicio. Qué
es, por qué existe y cómo se usa está en [docs/hermes.md](../docs/hermes.md), que enlaza a cada documento de detalle
de [docs/hermes/](../docs/hermes/). Este README explica cómo se desarrolla: las herramientas, la suite con la Hécate
real y Docker, el E2E, las comprobaciones de calidad, los ayudantes de pruebas y cómo se organiza el paquete.

Las órdenes de este README se ejecutan desde la raíz del repositorio.

## Herramientas

- **uv**, que gestiona el workspace de Cerbero (`cerbero-hermes` y `cerbero-contracts`, con el OpenAPI de
  `shared/contracts`) e instala Python 3.15 la primera vez.
- **Docker**, con Colima en macOS (`colima start`), para el PostgreSQL de usar y tirar de las pruebas, Envoy y el E2E,
  y **Docker Compose** (`docker-compose` o `docker compose`) para el E2E.
- **El binario de Hécate**, para las pruebas que lanzan una Hécate real: se compila desde `hecate/`, como dice su
  [README](../hecate/README.md), y se da en `HECATE_BIN`.

```bash
cargo build --release --manifest-path hecate/Cargo.toml
```

## La suite

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests
```

- **Unitarias** en `hermes/tests/unit/`; **de integración** en `integration/`: la API, el canal local, el cliente de
  Hécate, la persistencia, Telegram con respuestas grabadas, `hermes serve` y las fugas; **de contrato** en
  `contract/`: el OpenAPI y la batería de conformidad de `chat` ([proveedores](../docs/hermes/proveedores.md)); y el
  **E2E** en `e2e/`.
- **Marcas:** `docker`, las que necesitan Docker (el PostgreSQL de usar y tirar, Envoy o el compose), y `hecate`, las
  que necesitan una Hécate. Sin `HECATE_BIN`, las que lanzan la Hécate real se saltan; sin Docker, las de `docker`
  fallan, así que se dejan fuera con `-m "not docker"`.
- **Lo que mueve el reloj**, como la caché de los secretos, las renovaciones o los límites de uso, usa dobles con el
  reloj inyectado.
- **Cada respuesta** que reciben las pruebas de la API, también las de error, se valida contra el OpenAPI publicado.
- **Fugas:** una prueba recorre el alta, la configuración, un código y envíos que funcionan y que fallan, y busca el
  token del bot, los códigos, las direcciones, los tokens y el certificado, en claro y codificados, en los logs, las
  respuestas, la auditoría, lo exportado y la base.
- Las pruebas usan el proveedor fake y respuestas grabadas de Telegram; el recorrido con un bot de Telegram de pruebas
  está en [operación](../docs/hermes/operacion.md#el-compose-de-pruebas).

## El E2E

Las pruebas de `hermes/tests/e2e/` pasan con la suite, o solas:

```bash
HECATE_BIN="$PWD/hecate/target/release/hecate" uv run --package cerbero-hermes pytest hermes/tests/e2e
```

- **El flujo** levanta el compose de `hermes/deploy/` con `compose.test.yaml`: PostgreSQL, una Hécate de pruebas,
  Hermes con el fake y Envoy, y un cliente de referencia que se da de alta y llama como un componente, con el acceso que
  le concede el admin. Recorre la puesta en marcha, la verificación de un canal, los envíos, las credenciales que no
  valen, el acceso concedido y retirado, el certificado revocado de un componente, la revocación urgente del
  certificado de Hermes y su alta nueva, y PostgreSQL parado.
- **Envoy** delante de un upstream de eco, y **la recarga** de su certificado tras una renovación real en Hécate, sin
  reiniciarlo ni cortar una conexión abierta.
- La primera vez construye las imágenes, y la de Hécate se compila desde el repositorio. Sin Docker Compose, el flujo
  se salta. Es un entorno de pruebas: no fija cómo corre Hermes en el home lab.

## Comprobaciones de calidad

```bash
uv run --package cerbero-hermes ruff check hermes
```

```bash
uv run --package cerbero-hermes ruff format --check hermes
```

```bash
uv run --package cerbero-hermes mypy --strict hermes/src
```

```bash
uv run --package cerbero-hermes pip-audit
```

- ruff, con líneas de 120 caracteres y las reglas de `hermes/pyproject.toml`, comprueba el código y su formato.
- mypy, en modo estricto y con el plugin de Pydantic, comprueba los tipos.
- pip-audit busca avisos de seguridad en las dependencias; los paquetes del propio workspace, que no están en PyPI,
  salen como saltados.

## Los ayudantes de pruebas

En `hermes/tests/support/`:

| Ayudante | Qué es |
|---|---|
| `hecate.py` | Una Hécate real desde `HECATE_BIN`, con su propio directorio, el perfil `moderate` y certificados de una hora, preparada por la entrada estándar de su CLI |
| `fake_hecate.py` | Un doble de la API de Hécate con las reglas que necesitan las pruebas de la identidad |
| `pki.py` | Una CA de pruebas con la forma de la de Hécate: raíz e intermedia, certificados SPIFFE, listas de revocación, claves de firma de llamadas y tokens cortos |
| `identity.py` | Una identidad válida de Hermes escrita con su almacén, como la deja un alta, para las pruebas de `hermes serve` |
| `caller.py` | El cliente de referencia: se da de alta, firma su token corto y llama a Hermes a través de Envoy, con sus órdenes `enroll`, `send` y `bench` ([integración](../docs/hermes/integracion.md#probar-un-componente-contra-hermes)) |
| `envoy.py` | Envoy con la configuración de `hermes/deploy/envoy/` y un upstream de eco en el socket de la API, en Docker |
| `xfcc.py` | La cabecera `X-Forwarded-Client-Cert` como la escribe Envoy |
| `compose.py` | El compose de `hermes/deploy/` para el E2E, con su Hécate preparada por su CLI |
| `postgres.py` | Un PostgreSQL de usar y tirar, con la base y el usuario `hermes` y otra base a la que `hermes` no llega |
| `audit_log.py` | La tabla de la auditoría como la ven las pruebas, y una base que se niega a escribir una fila más |
| `network_api.py` | La API de red con dobles de sus ports, el fake y Telegram con respuestas grabadas; las claves, en memoria o en el PostgreSQL que le pase la prueba |
| `telegram.py` y `telegram_recordings/` | Respuestas grabadas de la Bot API de Telegram, para respx |
| `doubles.py` | Dobles de los ports: quien llama, los canales, la auditoría, los secretos, un proveedor de chat y la confianza de Hermes con la copia de su lista de acceso |
| `configuration.py` y `config_files.py` | Una configuración de pruebas (una instancia de Telegram, una del fake, una plantilla con dos versiones y tres contactos), como objeto y como fichero |
| `contract.py` | Comprueba respuestas y peticiones contra `hermes.v1.json` y `common.v1.json` |
| `clock.py` | Un reloj que las pruebas mueven a mano |
| `paths.py` | Directorios temporales cortos, para que quepa la ruta de un socket |
| `serve_with_password.py` | `hermes serve` con la contraseña de la base leída de un fichero en lugar de Hécate |

## Organización

```text
hermes/src/hermes/
├── domain/
│   ├── shared/          # códigos de error, secretos enmascarados, referencias, nombres y reloj
│   ├── configuration/   # modelo de la configuración, validación, plan y huella
│   ├── messages/        # mensaje neutral y sustitución de plantillas
│   ├── services/        # el manifiesto y un paquete por tipo de servicio: contrato común, port y dibujo (chat)
│   ├── idempotency/     # clave, huella de la petición y estado de una clave
│   ├── instances/       # estado de una instancia
│   └── verification/    # código de verificación
├── application/         # casos de uso: enviar, configurar, verificar, identidad, secretos, autenticación,
│                        # idempotencia, límites, capacidades, salud y arranque, con sus ports
└── infrastructure/
    ├── api/             # la API de red (FastAPI) y la autenticación de cada llamada
    ├── admin/           # el canal local y la CLI
    ├── config_file/     # el fichero de la configuración (YAML estricto) y la exportación
    ├── persistence/     # SQLAlchemy Core y las migraciones de Alembic
    ├── hecate/          # el cliente de Hécate, el TLS fijado a su raíz y el almacén de la identidad
    └── providers/       # una librería por proveedor (fake y telegram), los reintentos y el registro
```

Junto a esos paquetes, `infrastructure/` tiene el proceso de `hermes serve` y su arranque (`main.py`), las variables de
entorno (`settings.py`), los sockets, los logs y las tareas de fondo.

| Patrón | Dónde | Por qué |
|---|---|---|
| Hexagonal | Tipo de servicio = port; proveedor = adapter; FastAPI y la CLI = adapters de entrada | Cambiar de proveedor no toca el dominio |
| DDD | Lo común: el sobre, los contratos, la configuración y los contactos | El mismo lenguaje y las mismas reglas en todos los proveedores |

## Las specs

| Feature | Spec |
|---|---|
| La base de Hermes y `chat.send_message` con Telegram | [specs/008-hermes-send-message](../specs/008-hermes-send-message/) |
| La comprobación del acceso por app | [specs/009-app-access](../specs/009-app-access/) |
| La documentación de referencia | [specs/010-hermes-reference](../specs/010-hermes-reference/) |
| Hermes nota la revocación urgente de su propio certificado | [specs/011-hermes-own-revocation](../specs/011-hermes-own-revocation/) |
| El contrato de llamada: repetir con la misma clave y el `kid` del token | [specs/012-hermes-call-contract](../specs/012-hermes-call-contract/) |

Las decisiones de Hermes están en los ADR [0011](../docs/adr/0011-pila-hermes.md),
[0012](../docs/adr/0012-persistencia-postgresql-comun.md), [0013](../docs/adr/0013-acceso-por-app.md) y
[0014](../docs/adr/0014-clave-de-idempotencia-hermes.md).
