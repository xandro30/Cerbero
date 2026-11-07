# Guía de revisión: Arranque de Hécate

Rama `feature/001-hecate-bootstrap`. Guía para revisar la feature: por dónde empezar, cómo probarla y qué conviene saber.

## Por dónde empezar a revisar

Se va de lo que decide a lo que ejecuta, con el código al final:

1. **Constitución 1.1.0** ([constitution.md](../../.specify/memory/constitution.md)).
   - Leer primero el historial de enmiendas del final: el cambio del principio II y la justificación obligatoria.
   - Es lo único que afecta a todo el proyecto, y la enmienda debe pasar por la aprobación del propietario.
2. **La spec** ([spec.md](spec.md)). Cinco historias, 19 requisitos y las aclaraciones de la sesión: cambiar la protección exige demostrarlo de nuevo, el contador de intentos es persistente y común, y Hécate no ofrece reinicialización.
3. **El plan y la investigación** ([plan.md](plan.md), [research.md](research.md)). Las decisiones técnicas con sus alternativas, y los ADR [0003](../../docs/adr/0003-persistencia-hecate-sqlite.md) y [0004](../../docs/adr/0004-pruebas-pytest-hypothesis.md).
4. **El código**, en este orden:
   - `hecate/src/hecate/domain/sealing/`: las reglas puras (intentos, partes, protecciones).
   - `hecate/src/hecate/application/sealing/service.py`: toda la lógica; es el fichero más importante.
   - `hecate/src/hecate/infrastructure/`: criptografía, SQLite, socket y CLI.
5. **Los tests** (130), organizados por capa. `tests/integration/test_no_secret_leak.py` y `test_crash_during_change.py` cubren lo más crítico.

## Cómo probarlo

```bash
uv run --package cerbero-hecate pytest hecate/tests
```

El recorrido manual está en [quickstart.md](quickstart.md). Con `HECATE_KDF_PROFILE=moderate` va más rápido; con el perfil por defecto, cada derivación de Argon2id tarda unos 22 s.

## Qué hay que saber

- **Nada está en `main`.** Hay cosas mezcladas sin commit: la constitución, los ADR, la spec, el código y los docs consolidados. Se recomienda separar la enmienda de la constitución en su propio commit (o PR).
- **Sin probar en Linux.** `SO_PEERCRED` y `mlockall` solo se ejecutan en Linux. En macOS, `serve` avisa de que no bloquea la memoria.
- **Los tiempos son de un Mac de desarrollo**, no del servidor de casa: 21,9 s para inicializar y 21,8 s para desbloquear con `sensitive`. Quedan por medir allí.
- **Dos desviaciones del plan**, ya anotadas:
  - el coste real de Argon2id (`research.md`, R2), con `sensitive` como perfil por defecto por decisión del propietario;
  - la `version` por protección en los datos asociados del cifrado (`data-model.md`), necesaria para que cambiar la frase no invalide la protección por partes.
- **Python no borra la memoria con garantías.** Es un riesgo ya aceptado en DECISIONS, no un fallo nuevo.

## Dónde mirar con más sospecha

- **Los mensajes de error:** no deben revelar nada sensible. Hay tests, pero merece una lectura.
- **`service.py`:** el orden de las comprobaciones (estado, validación, espera, evaluación) decide qué cuenta como intento fallido.
- **El cierre de la feature:** `docs/hecate.md` ya está consolidado, pero hay que comprobar que el diseño vigente no contradiga lo implementado.

## Después de la revisión

`/speckit-converge` hace la comprobación automática del código contra la spec, el plan y las tareas.
