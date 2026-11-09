# Data Model: Hécate en Rust: migración con paridad de las features 001 y 002

El modelo de datos no cambia (FR-006): las entidades, sus campos, sus estados y sus reglas son los de los data-model de
[001](../001-hecate-bootstrap/data-model.md) y [002](../002-hecate-secrets/data-model.md) y los del apartado 8 del doc
de Hécate. Aquí está solo lo que este cambio fija o añade.

## Lo que se conserva

Todo se conserva byte a byte salvo el JSON guardado (eventos y protecciones del sellado), que se conserva en contenido:
las mismas claves y valores, sin imitar los separadores ni los escapes de la versión en Python (FR-006).

| Elemento | Formato | Se comprueba con |
|---|---|---|
| Esquema de SQLite | Tablas `seal_config`, `attempts`, `access_log`, `events`, `key_material` y `secret_material`; disparadores que impiden modificar o borrar `access_log` y `events`; `synchronous = FULL` y `secure_delete = ON` | `schema.sql` |
| Instantes guardados | `datetime.isoformat()` de Python: sin fracción si los microsegundos son 0; desfase `+HH:MM`; `expires_at` con el desfase que escribió el admin y el resto en UTC | `instants.json` |
| Eventos y protecciones del sellado | El contenido del JSON que guarda Python: las mismas claves y valores | `events.json` (eventos) y `root.json` (protecciones) |
| Datos asociados | JSON con las claves ordenadas y sin espacios, por capa (research R3) | `aad.json` |
| Protección de la raíz | Clave de Argon2id con los límites guardados y AES-256-GCM con los datos asociados de la raíz | `kdf.json` y `root.json` |
| Cifrado por capas | Raíz → clave de ámbito (una por identidad) → clave de versión → valor | `envelope.json` |
| Huella | 8 bytes de HMAC-SHA256 con una clave derivada de la de ámbito, en hexadecimal y en grupos de cuatro | `fingerprint.json` |

Los ficheros están en `hecate/tests/vectors/` (research R5).

## Parte de recuperación (versión 2)

| Campo | Regla |
|---|---|
| Prefijo | `HEC2` |
| Versión | `0x02` (un byte) |
| Índice | 1, 2 o 3 (un byte) |
| Identificador del juego | 4 bytes; una parte de otro juego da `auth` |
| Valores | 32 bytes: la parte de Shamir en GF(256) de la clave de recuperación |
| Suma de comprobación | Los 4 primeros bytes de SHA-256 de los 42 anteriores |

Dos partes cualesquiera del mismo juego recuperan la clave; una sola nunca. Una parte mal copiada, o que no empieza por
`HEC2` (también una `HEC1` de la versión en Python), da `validation` y no cuenta como intento. El texto y la lectura
están en [contracts/recovery-share.md](contracts/recovery-share.md).

## Secreto de paso

Dato secreto que solo existe durante una operación. Vive siempre en un tipo que se borra al soltarse, sin `Clone` ni
`Debug` que lo muestre (FR-007, research R14):

| Secreto | Dónde aparece |
|---|---|
| Frase del admin | Inicializar, desbloquear, cambiar la frase y pruebas de destrucción |
| Partes de recuperación (valores) | Inicializar, desbloquear, repartir de nuevo y pruebas |
| Clave derivada de la frase | Proteger o abrir la raíz |
| Clave de recuperación | Proteger o abrir la raíz por la vía de las partes |
| Clave de ámbito | Cifrar o abrir las claves de versión de una identidad y derivar la clave de la huella |
| Clave de versión | Cifrar o abrir un valor |
| Valor en claro | Escribir un secreto o resolverlo |

La raíz no es un secreto de paso: vive en el guardián de la raíz mientras Hécate está desbloqueada y se borra al sellar.

## Vector de prueba

| Campo | Regla |
|---|---|
| Cabecera | Versión de Python, versión de Unicode y origen (`main` en `0287c38`) |
| Casos | Entrada y salida esperada, o entrada y rechazo esperado |

Se generan una sola vez con el script de `specs/005-hecate-rust/vectors/`, antes de borrar la versión en Python, y no se
editan a mano. Las pruebas en Rust los leen y deben reproducir el 100 % (SC-008).

## Mapeo de pruebas

`specs/005-hecate-rust/test-mapping.md`, una fila por cada una de las 346 funciones de prueba de Python:

| Columna | Contenido |
|---|---|
| Prueba en Python | `<fichero>::<función>`, relativa a `hecate/tests/` de `main` en `0287c38` |
| Prueba en Rust | `<crate>::<ruta>::<función>` |
| Nivel | Unitaria, integración, contrato o extremo a extremo |
| Notas | Vacío si el comportamiento es el mismo; el motivo si es un cambio a propósito (el formato `HEC2` o una entrada que no sigue su formato estándar, FR-018) |

Cada prueba en Rust lleva un comentario `// py: <fichero>::<función>` con su origen.

Al final, una sección «Cambios a propósito», en lista y no en tabla para que no cuente en el apartado 5 del quickstart,
con `HEC2` y FR-018 y las pruebas en Rust que los cubren, también las nuevas.
