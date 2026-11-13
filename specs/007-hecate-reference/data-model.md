# Data Model: Hécate documentada antes de pausarla

En un cambio de documentación, las «entidades» son los documentos: qué desarrolla cada uno, para quién, de dónde salen
sus datos y cómo se enlazan. También los formatos que otros leen: el marco de un documento específico y las filas
nuevas de DECISIONS. Decisiones en [research.md](research.md).

## Los documentos

| Documento | Para quién | Desarrolla | Datos que se contrastan (FR-012) | Fuentes |
|---|---|---|---|---|
| `docs/hecate.md` | Cualquiera | Por qué existe, qué hace y qué no, quién la usa, cómo encaja todo, un párrafo por concepto y por uso, ante fallos, decisiones y riesgos, preguntas frecuentes | Los nombres de los aggregates y las superficies | El documento actual (R3) |
| `docs/hecate/sellado.md` | Diseño | La raíz, la frase y las partes, desbloquear (también, como diseño, desde Ténaro), sellar, el freno de intentos, la configuración de sellado y el adapter de hardware | Los tiempos del freno, el mínimo de la frase y los perfiles de Argon2id | `domain/src/sealing/`, `application/src/sealing/` |
| `docs/hecate/secretos.md` | Diseño | Secret y Key, la jerarquía de claves, el formato cifrado, las versiones y su ciclo de vida, la huella, los metadatos, la rotación de las claves de ámbito, `wrap`/`unwrap` y sus tablas | La gracia, `max_versions`, el formato de la huella y los límites de `rotate_every` | `domain/src/secrets/`, `application/src/secrets/` |
| `docs/hecate/identidades.md` | Diseño | Identity y Certificate, la CA en dos niveles y sus ceremonias, el alta, la renovación única, la revocación, el freno de altas, el certificado propio, las identidades hijas y sus tablas | Las duraciones (token, certificado, intermedia), el freno de altas y los avisos | `domain/src/identity/`, `domain/src/ca/`, `application/src/ca/`, `application/src/identity/` |
| `docs/hecate/acceso.md` | Diseño | La Access Policy (también la regla de lectura de las identidades hijas), la resolución en dos fases y su tiempo mínimo, el permiso de estado, los avisos y los permisos que salen de las declaraciones | El mínimo de los rechazos, las 100 referencias y los campos del estado | `domain/src/access_policy/`, `application/src/access_policy/`, ADR 0010 |
| `docs/hecate/auditoria.md` | Diseño | El registro de accesos (qué deja fila y qué no), su retención y la firma de checkpoints | Las operaciones y los métodos del registro | `domain/src/sealing/access_log.rs` y los data-model de las specs 003 y 006 |
| `docs/hecate/seguridad.md` | Diseño y revisión | Amenazas, el mecanismo que cubre cada una, principios, datos y riesgos aceptados | Ninguno propio: enlaza a cada concepto | El apartado 9 actual, DECISIONS, ADR 0010 |
| `docs/hecate/integracion.md` | Quien desarrolla otro componente | El lado del componente (R4) | Las rutas, la autenticación, el formato del token corto, los tiempos (5 minutos, caché de 300 y 3600 s), los códigos de error y los límites | El OpenAPI, `api/`, `call_token.rs`, el componente de prueba |
| `docs/hecate/cli.md` | El admin | Lanzar, los requisitos del sistema, las variables y las 32 órdenes (R5) | Cada orden, sus argumentos y salidas, los códigos de salida y cada variable con su valor por defecto y sus límites | `cli/src/lib.rs`, `config.rs`, `process.rs`, `control/server.rs` |
| `docs/hecate/operacion.md` | El admin | La puesta en marcha y los procedimientos (R6) | Cada orden de cada paso, y que se pueden hacer como están escritos (recorrido, R11) | La referencia de la CLI y el apartado 10 actual |
| `hecate/README.md` | Quien desarrolla Hécate | Compilar, probar en macOS y Linux, la calidad, el componente de prueba y los crates (R8) | Las órdenes de compilación y prueba, y la versión de Rust | Los `Cargo.toml`, el quickstart de la 006 |

## Enlaces entre documentos

```text
docs/hecate.md ──▶ cada documento de docs/hecate/ y hecate/README.md (un párrafo y un enlace cada uno)
docs/hecate/*.md ──▶ docs/hecate.md y los documentos vecinos («Relacionado»), DECISIONS para los riesgos
integracion.md ──▶ el OpenAPI de shared/contracts (los esquemas) y el componente de prueba (cliente de referencia)
operacion.md ──▶ cli.md (cada orden) y DECISIONS (lo que hoy no tiene un procedimiento completo)
seguridad.md ──▶ el concepto que desarrolla cada mecanismo y los riesgos de DECISIONS
ARCHITECTURE, README, hecate/README.md ──▶ docs/hecate.md
```

- Un tema que toca varios documentos se desarrolla en uno y los demás lo enlazan (FR-003).
- Todo enlace lleva a un fichero o apartado que existe (FR-015).

## El marco de un documento específico

```markdown
# <Concepto o uso> en Hécate

<Un párrafo: qué cubre este documento y para quién.>

## ...

<El desarrollo del tema, con sus diagramas y tablas.>

## Relacionado

- [Hécate](../hecate.md): el documento de alto nivel.
- <Los documentos vecinos que se citan.>
```

- Los documentos de diseño no dicen nada del estado de lo que describen (R0).
- Las referencias de uso (integración, CLI y operación) describen solo lo que existe (FR-013).
- Ningún documento usa palabras de tiempo («hoy», «todavía», «por ahora»): lo que no existe se dice como un hecho y
  remite a DECISIONS.

## Las filas nuevas de DECISIONS

En «Abiertas», con el formato de las filas que ya recogen lo diseñado sin implementar:

| Decisión | Notas |
|---|---|
| Firma de checkpoints de Hécate | Diseñado, no implementado: … |
| `wrap` / `unwrap` de Hécate | Diseñado, no implementado: … |
| Identidades hijas | Diseñado, no implementado: … |
| Permisos que salen de las declaraciones | Diseñado, no implementado: … |
| Rotación de las claves de ámbito | Diseñado, no implementado: … |
| Gestión y desbloqueo de Hécate desde Ténaro | Diseñado, no implementado: … |
| Despliegue en el home lab | Cómo corren los componentes (gestor de servicios o contenedores, usuarios y arranque); común a todos |

Las notas dicen qué es cada pieza y quién la necesita, sin fechas ni planes.

## La línea del README

En «Estado del Proyecto», una casilla marcada que dice lo que Hécate ya hace (sellado, secretos, identidades, Access
Policy y resolución por la API) y enlaza a `docs/hecate.md`.

## La entrada de la constitución

En su historial, con el formato de las anteriores: versión, fecha y título, y después qué cambia, por qué, qué
alternativas se descartaron y a qué afecta (R10). La versión pasa a 1.2.1 y la fecha de enmienda, a la del cambio.
