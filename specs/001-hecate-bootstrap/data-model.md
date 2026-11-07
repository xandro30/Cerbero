# Data Model: Arranque de Hécate

Entidades de la [spec](spec.md) con sus campos, reglas y transiciones. Las decisiones de formato están en
[research.md](research.md).

## Estado de sellado (derivado, no se guarda)

| Estado | Cuándo |
|---|---|
| `uninitialized` | No existe configuración de sellado |
| `sealed` | Existe configuración y la raíz no está en memoria |
| `unsealed` | La raíz está en memoria |

```text
uninitialized ──init.confirm──▶ sealed ──unseal (frase o 2 partes)──▶ unsealed
                                   ▲                                     │
                                   └──────── seal / reinicio ────────────┘
```

Reglas:

- Al arrancar el proceso, el estado es `uninitialized` o `sealed`; nunca `unsealed` (FR-006).
- En `sealed` solo se aceptan `unseal` y `status` (FR-007).
- `unsealed_via` (`passphrase` | `recovery`) se guarda en memoria para recomendar cambiar la frase (FR-011).

## Configuración de sellado (SQLite, una fila)

| Campo | Contenido |
|---|---|
| `config_version` | Entero; sube con cada cambio de frase o de partes |
| `initialized_at`, `updated_at` | Marcas temporales UTC |
| `passphrase_protection` | Protección por frase (abajo) |
| `recovery_protection` | Protección por partes (abajo) |

**Protección por frase**: `kdf` (`argon2id`), `kdf_opslimit`, `kdf_memlimit`, `salt` (16 bytes), `aead`
(`AES-256-GCM`), `nonce` (12 bytes), `ciphertext` (raíz cifrada con la clave derivada).

**Protección por partes**: `share_set_id` (4 bytes), `threshold` (2), `share_count` (3), `aead`, `nonce`,
`ciphertext` (raíz cifrada con la clave de recuperación).

Reglas:

- Datos asociados de cada cifrado: `{"purpose": "root", "protection": "passphrase"|"recovery", "config_version": n}`, donde `n` es el campo `version` de esa protección: la versión de la configuración en la que se escribió. Así, cambiar la frase no invalida la protección por partes (conserva su `version`) y viceversa (FR-013).
- Cambiar la frase o las partes sustituye su protección y sube `config_version` en una sola transacción
  (FR-012, SC-008). La raíz no cambia (FR-013).
- Nunca se guardan la raíz descifrada, la frase, la clave de recuperación ni las partes (FR-014).

## Raíz (solo en memoria)

32 bytes aleatorios. Existe descifrada solo en `unsealed`, en un búfer que se sobrescribe con ceros al sellar.

## Clave de recuperación (solo en memoria y de forma transitoria)

32 bytes aleatorios que cifran la raíz en la protección por partes. Existe solo mientras se generan o se
combinan las partes.

## Parte de recuperación (valor, nunca se guarda)

| Campo | Contenido |
|---|---|
| `version` | `1` |
| `index` | 1, 2 o 3 |
| `share_set_id` | 4 bytes; identifica el juego |
| `fragments` | 32 bytes (dos fragmentos de 16 bytes, uno por bloque de la clave) |
| `checksum` | 4 primeros bytes de SHA-256 de los campos anteriores |

Texto: `HEC1-` + Base32 de Crockford en grupos de 4 caracteres. Reglas:

- Una parte con suma incorrecta se rechaza con `validation` sin contar como intento (FR-009).
- Una parte de otro juego (`share_set_id` distinto) se rechaza con `auth` e indica que es de un juego antiguo.
- Dos partes con el mismo índice no cuentan como dos.

## Contador de intentos (SQLite, una fila)

| Campo | Contenido |
|---|---|
| `consecutive_failures` | Fallos seguidos de desbloqueo o de prueba para cambiar la protección |
| `next_attempt_at` | Hora UTC a partir de la cual se admite otro intento; vacía si no hay espera |

Reglas (FR-016): a partir del 3.er fallo, espera de 1, 2, 4, 8 y después 15 minutos fijos; persiste tras
reiniciar; un desbloqueo correcto lo pone a cero; frase y partes suman en el mismo contador.

## Registro de accesos (SQLite, append-only)

| Campo | Contenido |
|---|---|
| `id` | Secuencial |
| `at` | Marca temporal UTC |
| `operation` | `init`, `unseal`, `seal`, `passphrase_change`, `shares_rekey` |
| `method` | `passphrase`, `recovery` o `local` |
| `result` | `success`, `failure`, `rejected_rate_limited`, `rejected_conflict`, `invalid_input` |
| `detail` | Motivo legible, sin secretos |

Reglas: nunca se modifica ni se borra una fila; nunca contiene la frase ni las partes (FR-015).

## Inicialización y reparto pendientes (solo en memoria)

Entre `begin` y `confirm`, la raíz, la clave de recuperación y las partes nuevas viven solo en memoria, con un
identificador y una caducidad de 15 minutos. Si se aborta, caduca o se reinicia el proceso, no queda nada
persistido (FR-004).
