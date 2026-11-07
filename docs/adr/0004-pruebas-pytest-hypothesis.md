# 0004 — Pruebas en Python con pytest e hypothesis

Estado: aceptada · Fecha: 2026-11-07

## Contexto

La constitución exige TDD (principio IV) y la primera feature implementable (arranque de Hécate) necesita un
framework de pruebas. Varias de sus reglas son propiedades sobre todas las combinaciones posibles (cualquier par
de partes recupera la raíz, una sola nunca; cualquier byte alterado impide descifrar).

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **pytest + hypothesis** | Estándar de facto; fixtures y parametrización; hypothesis genera casos para probar propiedades | Dos dependencias de desarrollo |
| Solo pytest | Una dependencia | Las propiedades se enumeran a mano |
| unittest (stdlib) | Sin dependencias | Más verboso; sin el ecosistema de pytest |

## Decisión

**pytest** como framework de pruebas de todos los componentes Python e **hypothesis** para las pruebas por
propiedades. Son dependencias de desarrollo; nunca se instalan en producción. Licencias compatibles con
AGPL-3.0-or-later (MIT y MPL-2.0).

## Consecuencias

- Cada miembro del workspace organiza sus pruebas en `tests/unit` y `tests/integration` (y `tests/contract`
  cuando publique contratos).
- Las propiedades de seguridad (recuperación, manipulación, límites) se expresan con hypothesis.
- Rust (Eurytion) sigue con `cargo test`; queda fuera de este ADR.
