# 0006 — Pruebas en Rust con cargo test y proptest

Estado: aceptada · Fecha: 2026-11-08

## Contexto

El [ADR 0005](0005-rust-para-orthros-y-hecate.md) pasa Hécate y Orthros a Rust, y el [ADR 0004](0004-pruebas-pytest-hypothesis.md) fija pytest e hypothesis solo para Python. Las piezas en Rust necesitan lo equivalente para cumplir el principio IV de la constitución: TDD; happy path, error path y casos límite en el nivel que corresponda (unitario, integración, API o E2E); pruebas de contrato y propiedades. Además, el port de Hécate tiene que demostrar la paridad con las features 001 y 002, cuyas pruebas en Python describen el comportamiento esperado.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **cargo test + proptest + assert_cmd** | Lo estándar de Rust: unitarias junto al código e integración en `tests/` sin dependencias. proptest es lo más parecido a hypothesis (estrategias componibles y reducción de casos), y assert_cmd prueba la CLI real, como los quickstarts | proptest es más lento que quickcheck con entradas grandes |
| cargo test + quickcheck | Más simple | Reducción de casos menos controlable y estrategias menos expresivas que las de hypothesis |
| bolero (propiedades y fuzzing) | Une propiedades y fuzzing guiado | Más complejo de montar; el fuzzing no es lo que hace falta ahora |
| cargo-nextest como ejecutor | Más rápido y aísla cada prueba en su proceso | Complementa a cargo test, no lo sustituye; se puede añadir después |

## Decisión

**cargo test para las pruebas unitarias y de integración, proptest para las propiedades y assert_cmd (con predicates y tempfile) para la CLI de extremo a extremo.**

La calidad se comprueba con `cargo fmt --check`, `cargo clippy` con los avisos como errores y `cargo deny check`: licencias según la política del proyecto, avisos de RUSTSEC y crates duplicadas o vetadas.

## Consecuencias

- Los mismos niveles que en Python: unitarias en cada módulo (`#[cfg(test)]`), integración y contrato en `tests/`, y extremo a extremo con la CLI real.
- Las propiedades que hoy cubre hypothesis (manipular un dato cifrado o moverlo de ruta impide descifrarlo, transiciones del ciclo de vida, aislamiento entre identidades) pasan a proptest.
- El port de Hécate traslada las pruebas de 001 y 002 caso a caso: son la definición de la paridad.
- La política de licencias de cargo-deny admite la licencia de OpenSSL solo en las crates que cubre el permiso adicional del [ADR 0008](0008-excepcion-openssl-agpl.md).
- Antes de cada PR se ejecutan las dos cadenas: pytest para las piezas en Python y cargo para las piezas en Rust.
