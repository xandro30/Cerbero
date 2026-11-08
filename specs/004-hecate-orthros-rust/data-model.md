# Data Model: Hécate y Orthros en Rust: la decisión documentada

Este cambio no tiene datos en ejecución: sus entidades son los documentos que gobiernan el proyecto y las unidades de
trabajo que esos documentos regulan. Aquí están sus campos, sus reglas y sus estados, tal como los fija la spec; dónde
se escribe cada regla está en [research.md](research.md) (R3, R4).

## Cambio

Unidad de trabajo que entra en `main` por una PR desde su rama (FR-018).

| Campo | Regla |
|---|---|
| Número `<NNN>` | Tres cifras: el siguiente al mayor de los que hay en `main` y en los cambios pausados de DECISIONS (FR-005) |
| Nombre | Corto, en inglés, en minúsculas y con guiones |
| Carpeta | `specs/<NNN>-<nombre>/`; las dos excepciones no la tienen |
| Rama | `<tipo>/<nombre>`; las de tipo `feature/` llevan el número: `feature/<NNN>-<nombre>` |
| Tipo de rama | Uno de la tabla de tipos; fija el tipo del commit de squash |
| Fases | specify → clarify si hay dudas → plan → tasks → analyze → implement → converge; ninguna se sustituye por un acuerdo en la conversación |

**Excepciones** (en su rama y con PR, sin ciclo ni carpeta): una corrección que no cambia el significado de ningún
documento ni toca código ni configuración (erratas, enlaces rotos, formato), o una subida de versión del toolchain o de
las dependencias que no obliga a cambiar código. Si la subida obliga a cambiar código, o la corrección cambia el
significado, deja de ser una excepción y sigue el ciclo.

**Estados:**

```text
en curso ──(el propietario lo pausa)──► pausado ──(se reanuda)──► en curso ──(PR fusionada)──► fusionado
```

- **En curso:** solo uno a la vez; avanza fase a fase y cada fase la revisa el propietario.
- **Pausado:** tiene su fila en DECISIONS, con su carpeta, su rama, el motivo y el cambio al que espera (FR-006).
- **Reanudado:** vuelve a estar en curso; su fila sigue en DECISIONS hasta que se fusiona y solo reserva su número. Si
  una enmienda lo afecta, su spec y su plan se revisan antes de continuar (gobierno de la constitución).
- **Fusionado:** su PR retira su fila de DECISIONS, si la tenía, y su spec queda como registro de su fecha (FR-019).

## Tipo de rama

| Rama | Commit de squash |
|---|---|
| `feature/` | `feat:` |
| `fix/` | `fix:` |
| `refactor/` (nueva) | `refactor:` |
| `chore/` | `chore:` |
| `docs/` | `docs:` |

`refactor/` es para los cambios de código que conservan el comportamiento, como una migración de lenguaje (FR-005), y
`feature/`, para los que añaden funcionalidad (FR-018). La constitución no acota más los otros tipos.

## Constitución

| Campo | Regla |
|---|---|
| Versión | Semver; esta enmienda pasa de 1.1.0 a 1.2.0, MINOR (R1) |
| Reglas | Principios, restricciones, flujo y gobierno; las reglas nuevas van en el flujo (R3) |
| Historial | Una entrada por versión; cada bloque con su origen, qué, por qué, alternativas y afecta (FR-007) |
| Sync Impact Report | Temporal: lo escribe `/speckit-constitution` y se quita antes del commit (FR-007) |

Las entradas antiguas del historial no se reescriben (R8).

## ADR

| Campo | Regla |
|---|---|
| Número y título | `NNNN-titulo.md`, una decisión por fichero |
| Estado | propuesta → aceptada → sustituida por NNNN |
| Partes | Contexto, Opciones, Decisión y Consecuencias (FR-009) |
| Fila del índice | En `docs/adr/README.md`: número, decisión y estado |

- Una vez fusionado, solo cambian su estado y su fila del índice; el ADR que lo complementa o lo sustituye lo dice en
  su propio texto (FR-019). Los ADRs 0005 a 0008 son de este cambio y se pueden corregir hasta que se fusione.
- El sustituido remite a su sustituto: el 0002 al 0005.

## DECISIONS

| Sección | Formato |
|---|---|
| Desviaciones respecto a la referencia | Una subsección por componente, con lo descartado |
| Riesgos aceptados | `\| Riesgo \| Por qué se acepta \|`; los que describen la implementación actual siguen hasta la migración (FR-013) |
| Tomadas | `\| Decisión \| ADR \|`, con enlace |
| Abiertas | `\| Decisión \| Notas \|` |
| Cambios pausados (nueva) | `\| Cambio \| Rama \| Motivo \| Espera a \|`, sin enlace a la carpeta, que solo existe en su rama (R4) |

## Permiso adicional de la licencia

`LICENSE-EXCEPTION`, en inglés, como la licencia. Lo enlazan el README (junto con el ADR 0008), la regla de licencia de
la constitución y el ADR 0008. `LICENSE` sigue siendo el texto íntegro de la AGPL.

## Doc de componente

Describe el diseño vigente y se actualiza al consolidar lo implementado. Lo que describe la implementación actual se
queda hasta la migración y figura en las consecuencias del ADR que lo cambia (R6). Sus afirmaciones tienen que ser
ciertas hoy: la de librerías auditadas lleva la excepción de Shamir (FR-015).
