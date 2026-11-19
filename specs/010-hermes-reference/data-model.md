# Data Model: Hermes documentado en un documento de alto nivel y documentos específicos

En un cambio de documentación, las «entidades» son los documentos: qué desarrolla cada uno, para quién, de dónde salen
sus datos y cómo se enlazan. También los formatos que otros leen: el marco de un documento específico, el de un
procedimiento y los enlaces de DECISIONS que cambian. Decisiones en [research.md](research.md).

## Los documentos

| Documento | Para quién | Desarrolla | Datos que se contrastan (FR-014) | Fuentes |
|---|---|---|---|---|
| `docs/hermes.md` | Cualquiera | Por qué existe, qué hace y qué no, quién lo usa, cómo encaja todo, un párrafo por concepto y por uso, ante fallos, decisiones y riesgos, preguntas frecuentes y referencias (R2) | Los nombres de las superficies, de los componentes y de las funciones | El documento actual (R3) |
| `docs/hermes/contratos.md` | Diseño | La taxonomía, las tres capas, el sobre común, el contrato de una función, `chat.send_message` y el formato y el versionado de la API | Los códigos de error, los estados HTTP, los campos del contrato de una función y el orden de las comprobaciones | `hermes.v1.json`, `common.v1.json`, `domain/services/`, `application/send_message.py`, contrato `api.md` de la 008 |
| `docs/hermes/llamadas.md` | Diseño | La llamada síncrona, la task, la idempotencia, los reintentos seguros, el resultado desconocido y los timeouts | La vida de una clave (24 horas), las esperas de los reintentos (base, tope y jitter) y el timeout de `chat.send_message` | `application/idempotency.py`, `domain/idempotency/`, `providers/retry.py`, contrato `api.md` de la 008 |
| `docs/hermes/comunicacion.md` | Diseño | Los contactos y sus canales, las plantillas y sus variables, el mensaje neutral y su dibujo, la verificación de un canal y `chat.ask` | El código de verificación (alfabeto, longitud y vida), el orden del recorte y la respuesta «Canal verificado.» | `domain/messages/`, `domain/verification/`, `application/verification.py`, `domain/services/chat/drawing.py`, contratos `config-file.md` y `telegram.md` de la 008 |
| `docs/hermes/configuracion.md` | Diseño y el admin | La configuración, su fichero, el plan, la aplicación y la exportación, y las instancias | Las reglas de lectura del fichero (tamaño, nombres, campos), el orden del plan y el rango del límite de uso | `infrastructure/config_file/`, `domain/configuration/`, `application/configuration.py`, contratos `config-file.md` y `admin-channel.md` de la 008 |
| `docs/hermes/proveedores.md` | Quien desarrolla una librería | Las librerías, sus partes, el manifiesto, su versión, el polling, la batería de conformidad, el fake y la librería de Telegram | Los campos del manifiesto, los casos de la batería, las bandejas del fake y la traducción de los errores de Telegram | `infrastructure/providers/`, `domain/services/manifest.py`, contratos `conformance.md`, `fake.md` y `telegram.md` de la 008 |
| `docs/hermes/seguridad.md` | Diseño y revisión | Amenazas, el mecanismo que cubre cada una, el proxy, la autenticación y el acceso, los secretos, la auditoría, los datos y los riesgos (R7) | Lo que exige al proxy y el orden de las comprobaciones | Los apartados 8 y 9 actuales, contrato `proxy.md` de la 008, `api/xfcc.py`, `application/authentication.py`, DECISIONS, ADR 0011 y 0013 |
| `docs/hermes/integracion.md` | Quien desarrolla un componente que llama a Hermes | El lado de quien llama (R4) | Las rutas, las cabeceras, el token corto, los códigos de error con sus estados HTTP, los estados de una instancia y los límites | `hermes.v1.json`, `common.v1.json`, `api/`, `application/authentication.py`, el cliente de referencia |
| `docs/hermes/cli.md` | El admin | Lanzar Hermes, lo que necesita, las variables y las seis órdenes (R5) | Cada orden, sus argumentos y salidas, los códigos de salida y cada variable con su valor por defecto y sus límites | `admin/cli.py`, `settings.py`, `main.py`, `sockets.py`, contratos `cli.md` y `admin-channel.md` de la 008 |
| `docs/hermes/operacion.md` | El admin | La puesta en marcha, los procedimientos y el compose de pruebas (R6) | Cada orden de cada paso, y que se pueden hacer como están escritos (recorrido, R11) | Las referencias de la CLI de Hermes y de Hécate, el apartado 10 actual, los quickstarts de la 008 y la 009 y `hermes/deploy/` |
| `hermes/README.md` | Quien desarrolla Hermes | Las herramientas, la suite y sus marcas, el E2E, la calidad, los ayudantes de pruebas y la organización del paquete (R8) | Las órdenes de la suite y de la calidad, las marcas y la versión de Python | `hermes/pyproject.toml`, `hermes/tests/`, el apartado 1 de los quickstarts de la 008 y la 009 |

## Enlaces entre documentos

```text
docs/hermes.md ──▶ cada documento de docs/hermes/ y hermes/README.md (un párrafo y un enlace cada uno)
docs/hermes/*.md ──▶ docs/hermes.md y los documentos vecinos («Relacionado»), DECISIONS para lo abierto y los riesgos
integracion.md ──▶ el OpenAPI de shared/contracts (los esquemas), la guía de integración de Hécate (la identidad),
                   llamadas.md (la idempotencia) y el cliente de referencia
operacion.md ──▶ cli.md, la referencia de la CLI de Hécate y DECISIONS (lo que no tiene un procedimiento completo)
seguridad.md ──▶ el documento que desarrolla cada mecanismo y los riesgos de DECISIONS
DECISIONS ──▶ llamadas.md (las tasks) y comunicacion.md (chat.ask y los grupos de contactos)
ARCHITECTURE, README, hermes/README.md, los ADR y los docs de otros componentes ──▶ docs/hermes.md
```

- Un tema que toca varios documentos se desarrolla en uno y los demás lo enlazan (FR-003).
- Todo enlace lleva a un fichero o apartado que existe (FR-017).

## El marco de un documento específico

```markdown
# <Concepto o uso> de Hermes

<Un párrafo: qué cubre este documento y para quién.>

## ...

<El desarrollo del tema, con sus diagramas y tablas.>

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- <Los documentos vecinos que se citan.>
```

- Los documentos de diseño no dicen nada del estado de lo que describen (R0).
- Las referencias de uso (integración, CLI y operación) describen solo lo que existe (FR-013).
- Ningún documento usa palabras de tiempo («hoy», «todavía», «por ahora»): lo que no existe se dice como un hecho y
  remite a DECISIONS.

## El marco de un procedimiento

```markdown
### <Situación>

**Cuándo:** <qué lo dispara y cómo se nota>.

**Qué necesitas:** <el acceso, los ficheros o los datos que hacen falta>.

**Pasos:**

1. <Una orden de hermes o de hecate, con su salida esperada.>

**Cómo comprobarlo:** <la orden o la señal que confirma que ha funcionado>.
```

El apartado del compose de pruebas dice, una vez, cómo se ejecuta en él cada orden; los procedimientos no repiten la
envoltura de `docker compose`.

## Los enlaces de DECISIONS que cambian

| Fila de «Abiertas» | Enlaza hoy a | Pasa a enlazar a |
|---|---|---|
| `chat.ask` | `docs/hermes.md#comunicación-contactos-y-plantillas` | El apartado de `chat.ask` en `docs/hermes/comunicacion.md` |
| Tasks de Hermes | `docs/hermes.md#task` | El apartado de la task en `docs/hermes/llamadas.md` |
| Grupos de contactos | `docs/hermes.md#instancia-contacto-y-configuración` | El apartado de los contactos en `docs/hermes/comunicacion.md` |

El texto de las filas no cambia. Si al implementar aparece una pieza de Hermes sin construir que no figura, se añade con
el formato de las demás: «<pieza> | Diseñado, no implementado: <qué es>; <quién lo necesita>».

## La línea del README

En «Estado del Proyecto», la casilla de Hermes ya dice lo que hace y enlaza a `docs/hermes.md`; no cambia salvo que
falte algo de lo construido.
