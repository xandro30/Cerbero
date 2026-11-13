# Seguridad de Hécate

Frente a qué protege Hécate y con qué. Este documento desarrolla las amenazas que tiene en cuenta, el mecanismo que
cubre cada una y el documento que lo explica, los principios que siguen todos, los datos que guarda y los que nunca
salen de la memoria, y los riesgos que se aceptan. Es para quien revisa Hécate o decide si confiarle un secreto.

## Frente a qué protege

| Amenaza | Mecanismo | Dónde se desarrolla |
|---|---|---|
| **Robo del disco o de una copia** | Todo lo cifrado cuelga de la raíz, que solo vive en memoria y que protegen la frase del admin (Argon2id) y las partes de recuperación: sin la frase o dos partes, los datos y las copias no sirven | [Sellado](sellado.md) y [secretos](secretos.md#la-jerarquía-de-claves) |
| **Un dato cifrado movido o cambiado** | Cada capa del cifrado por sobre lleva datos asociados (ruta, versión, ámbito): fuera de su sitio no se descifra | [Secretos](secretos.md#el-formato-de-un-secreto-cifrado) |
| **Fuerza bruta contra la frase o las partes** | El freno de intentos, común al desbloqueo y a todas las pruebas, que sobrevive a los reinicios | [Sellado](sellado.md#el-freno-de-intentos) |
| **Alguien en la red interna** | La API habla solo TLS 1.3; cada llamada lleva el certificado de su componente y un token corto firmado con su clave de firma de llamadas, ligado a ese certificado y de un solo uso; el alta, un token de un solo uso con la huella de la CA raíz | [Identidades](identidades.md) e [integración](integracion.md) |
| **Una clave de componente robada** | Certificados de 3 días que se renuevan una sola vez: un segundo intento revoca con urgencia los de esa identidad; además, la revocación urgente y deshabilitar la identidad | [Identidades](identidades.md#la-revocación) |
| **Un componente que prueba rutas ajenas** | La Access Policy deniega por defecto, y los rechazos son iguales en contenido y en tiempo (250 ms como mínimo) y no abren ninguna clave | [Acceso](acceso.md#la-resolución) |
| **Un componente comprometido** | Solo lee sus secretos y los que el admin le comparte, uno a uno; su clave de ámbito solo descifra lo suyo; no puede reescribir su auditoría ya firmada | [Acceso](acceso.md#la-access-policy), [secretos](secretos.md#la-jerarquía-de-claves) y [auditoría](auditoria.md#la-firma-de-checkpoints) |
| **La CA comprometida o perdida** | La clave de la CA raíz nunca entra en Hécate; ante una intermedia comprometida o la clave raíz perdida, se sustituye la CA entera | [Identidades](identidades.md#la-ca-en-dos-niveles) |
| **Un error del admin** | Las versiones con su gracia, volver a la anterior y restaurar; destruir exige la prueba de sellado; la huella deja comprobar lo escrito sin leer el valor | [Secretos](secretos.md#el-ciclo-de-vida-de-un-secreto) |
| **Una intrusión en curso** | Sellar a mano con una sola acción borra la raíz de la memoria | [Sellado](sellado.md#los-estados) |
| **Una reescritura de la historia** | El registro de accesos y los eventos son append-only; los checkpoints firmados detectan cualquier cambio en la auditoría de los componentes | [Auditoría](auditoria.md) |
| **Secretos en la memoria o en un volcado** | La raíz, las claves y los valores de paso se borran al terminar cada operación, y los valores resueltos al enviar la respuesta; el proceso desactiva los volcados de memoria y bloquea su memoria | [Sellado](sellado.md#la-raíz-en-memoria), [acceso](acceso.md#la-resolución) y [CLI](cli.md) |

## Principios

- **Nunca expuesta a internet:** Hécate solo vive en la red interna y nunca llama a nadie: siempre le piden.
- **Denegar por defecto:** cada identidad solo lee lo suyo; compartir es una excepción explícita del admin para un
  secreto concreto.
- **Nada secreto en eventos, logs ni filas:** ningún evento lleva un valor, y el registro de accesos y los logs no
  llevan valores, huellas, frases, partes ni tokens.
- **Los secretos de paso se borran:** la frase de la CA, las claves y los tokens de paso se borran de la memoria al
  terminar cada operación.
- **El admin escribe valores pero nunca los lee:** el valor solo sale por la resolución, que es de las identidades.
- **Una clave, un uso,** y primitivas de librerías auditadas, sin inventar criptografía.
- **Quién hace qué:** los componentes solo se dan de alta, renuevan, leen lo publicado, resuelven sus secretos y los
  que les comparten y, con el permiso de estado, consultan el estado; la gestión y el desbloqueo son solo del admin, por
  la CLI local o desde Ténaro.
- **Cada rechazo deja rastro:** Hécate valida la cadena del certificado en la aplicación, así que todo rechazo de
  autenticación responde `auth` y deja su fila; cada cambio de política se versiona con quién lo hizo.
- **El proceso** corre con un usuario propio, sin swap ni volcados de memoria.

## Los datos

| Dato | Dónde | Se copia |
|---|---|---|
| Configuración de sellado (la raíz cifrada dos veces) y contador de intentos | SQLite ([ADR 0003](../adr/0003-persistencia-hecate-sqlite.md)) | Sí |
| Eventos (solo metadatos) y material cifrado: valores y DEK envueltas, claves de ámbito envueltas, certificados de la CA y claves de las intermedias envueltas | SQLite | Sí |
| La clave propia de Hécate, en claro, y el freno de altas | SQLite | Sí |
| El registro de accesos y las políticas | SQLite | Sí |
| La raíz en claro y las cachés | Solo en memoria | Nunca |
| La clave de la CA raíz | Nunca en Hécate: solo la copia protegida con la frase de la CA, que el admin guarda en dos soportes en sitios distintos | — |

- **Sin la frase o las partes, las copias no sirven**, a propósito.
- **Datos personales:** ninguno más allá de quién hizo cada cambio.
- Cómo se hace y se restaura una copia está en [operación](operacion.md).

## Riesgos aceptados

Hécate acepta estos riesgos; el porqué de cada uno está en [DECISIONS](../../DECISIONS.md#riesgos-aceptados):

- firma sus propios checkpoints, y eso solo lo cubre un anclaje externo;
- tras un corte de luz de toda la casa queda sellada y no hay avisos hasta desbloquearla;
- la raíz está en memoria mientras está desbloqueada, y pueden quedar copias de los secretos de paso que el proceso no
  controla;
- ninguna implementación de Shamir tiene una auditoría pública, tampoco vsss-rs;
- su clave propia se guarda en claro, para responder sellada con su certificado vigente.

Lo que la reforzaría y no está decidido, como el adapter de hardware o el anclaje externo de los checkpoints, está entre
las [abiertas](../../DECISIONS.md#abiertas).

## Relacionado

- [Hécate](../hecate.md): el documento de alto nivel.
- [Sellado](sellado.md), [secretos](secretos.md), [identidades](identidades.md), [acceso](acceso.md) y
  [auditoría](auditoria.md): el desarrollo de cada mecanismo.
- [DECISIONS](../../DECISIONS.md#riesgos-aceptados): los riesgos aceptados.
