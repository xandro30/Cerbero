# Formatos de la feature 006

Las entradas siguen su formato estándar, como fijan el FR-018 del cambio 005 y el FR-029 de la feature 003; lo demás
se rechaza con `validation` sin efectos.

## Referencia a un secreto

`secret://<identidad>/<resto>` con la regla de rutas de la feature 002, y opcionalmente `@N` al final para una versión
concreta: N es un entero decimal de 1 a 9 cifras, sin ceros a la izquierda, como en la feature 002.

- Una petición de resolución lleva de 1 a 100 referencias, en una lista de textos.
- Una referencia repetida se resuelve otra vez, con el mismo resultado.
- En las políticas, el secreto se nombra sin `@N`: una excepción vale para el secreto, con las reglas de versiones de la
  resolución.

## `rotate_every`

Una duración ISO 8601 en días o en semanas, en mayúsculas, con n en dígitos decimales sin signo ni ceros a la
izquierda, y sin nada delante ni detrás:

| Forma | Límites | Ejemplos válidos | Ejemplos que se rechazan |
|---|---|---|---|
| `P<n>D` | n de 1 a 3650 | `P1D`, `P90D`, `P365D` | `P0D`, `P090D`, `P+90D`, `P3651D`, `p90d`, `90d` |
| `P<n>W` | n de 1 a 520 | `P1W`, `P12W` | `P0W`, `P012W`, `P521W`, `P1W2D` |

También se rechazan las demás formas de ISO 8601 (`P3M`, `P1Y`, `PT36H`): un mes o un año no tienen una duración fija.
Un secreto debe rotarse cuando su versión activa tiene esa duración o más: en el instante exacto ya cuenta.

## Secreto resuelto

| Campo | Formato |
|---|---|
| `reference` | La referencia tal como llegó en la petición |
| `version` | Entero de 1 en adelante |
| `value` | El valor en texto UTF-8, tal como se guardó, sea cual sea su tipo (`string`, `json` o `pem`) |
| `cache_ttl_soft_s`, `cache_ttl_hard_s` | Enteros en segundos: 300 y 3600 |
