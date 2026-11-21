# Data Model: el contrato de llamada de Hermes: repetir con la misma clave y el `kid` del token

Lo que el cambio 012 cambia en el modelo de Hermes: qué filas quedan en la tabla de las claves de idempotencia. **No
hay tablas, columnas ni migraciones nuevas.** Decisiones en [research.md](research.md) (R2 y R3).

## La fila de una clave

La tabla `idempotency_keys` de la base de Hermes, como la dejó la 008
([data model de la 008](../008-hermes-send-message/data-model.md)):

| Columna | Qué guarda |
|---|---|
| `caller`, `key` | La identidad que llama y su `Idempotency-Key`: la clave primaria |
| `fingerprint` | La huella de la petición: SHA-256 del método, la ruta y el JSON canónico del cuerpo |
| `state` | `in_progress` desde justo antes del proveedor; `completed` con su respuesta |
| `created_at` | La hora del intento, que cuenta para las 24 horas |
| `http_status`, `response` | La respuesta que devuelve a toda repetición, con su estado HTTP |
| `pending_audit` | Mientras está `in_progress`, la entrada de auditoría del envío con un resultado desconocido |

**Lo que cambia es qué respuesta puede guardar una fila `completed`:**

| Resultado del intento | Hoy | Con el cambio |
|---|---|---|
| Un envío completado | Fila `completed`, 200 | Igual |
| `timeout` con `outcome_unknown` del proveedor | Fila `completed`, 200 | Igual |
| Un fallo sin envío del proveedor (no conecta, pide esperar más de lo que cabe, rechaza las credenciales o la petición, la persona no admite el bot) | Fila `completed` con el fallo, 200 | Sin fila: se libera (R2) |
| Un error de Hermes antes de que la petición salga (la instancia sin token del bot) | Fila `completed` con el error | Sin fila: se libera (R2) |
| Una excepción inesperada al enviar | Fila `completed` con `internal`, 500 | Fila `completed` con `timeout` y `outcome_unknown`, 200 (R3) |
| Un corte entre la reserva y el resultado | Fila `in_progress`; la repetición la completa con `timeout` y `outcome_unknown` | Igual |
| Un fallo antes del proveedor | Sin fila | Igual |

**El invariante nuevo:** una fila `completed` guarda un envío completado o un resultado desconocido (`timeout` con
`outcome_unknown`), nunca otro fallo. Las filas guardadas antes del cambio con otro fallo caducan con la limpieza, a las
24 horas, sin migrarlas (R2).

### Transiciones

```text
sin fila ──reserva, justo antes del proveedor──▶ in_progress (con la entrada de un resultado desconocido)
in_progress ──envío completado o resultado desconocido, con su entrada de auditoría──▶ completed
in_progress ──fallo sin envío, con su entrada de auditoría──▶ sin fila (nueva: la liberación)
in_progress ──repetición tras un corte──▶ completed con timeout y outcome_unknown, con la entrada que guardó la reserva
cualquiera ──limpieza, pasadas 24 horas──▶ sin fila (una in_progress deja antes la entrada que guardó la reserva)
```

- **La liberación** borra la fila `in_progress` e inserta la entrada de auditoría del intento en una sola transacción.
  Si la entrada no se puede escribir, no borra nada; con la base caída, la petición responde `unavailable`, la fila
  sigue `in_progress` y la repetición responde `timeout` con `outcome_unknown`, como tras un corte.
- **Las peticiones iguales a la vez** siguen pasando de una en una por el bloqueo en memoria de la clave; tras una
  liberación, la siguiente no encuentra fila y reserva la suya.
- **Una petición con otro cuerpo** responde `conflict` mientras la clave tiene fila, y es una petición nueva cuando no
  la tiene: tras un fallo antes del proveedor, como hoy, y tras una liberación.

## La auditoría

No cambia su forma. Cada intento que llega al proveedor deja una entrada, también el que libera la clave, con el
resultado de siempre (`{"status": "failed", "code": ..., "outcome_unknown": false}`). La excepción inesperada al enviar
deja `{"status": "failed", "code": "timeout", "outcome_unknown": true}` en lugar de `internal` (R3).

## Lo que no cambia

- **El esquema de la base:** la tabla, sus columnas, sus índices y las migraciones de Alembic.
- **La vida de las claves y su limpieza:** 24 horas, cada 5 minutos.
- **El bloqueo en memoria de cada clave** y la huella de la petición.
- **El estado de cada instancia:** cuenta los mismos fallos del proveedor que hoy; la excepción inesperada al enviar
  sigue sin contar (R3).
