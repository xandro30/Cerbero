# 0008 — Permiso adicional de enlace con OpenSSL en la AGPL

Estado: aceptada · Fecha: 2026-11-08

## Contexto

Cerbero se publica bajo la `AGPL-3.0-or-later`. La pila de Hécate en Rust ([ADR 0007](0007-pila-hecate-rust.md)) usa aws-lc-rs, que es también el proveedor criptográfico por defecto de rustls; ring, su alternativa, está en la misma situación. Las dos llevan código heredado de OpenSSL, a través de BoringSSL, bajo la licencia antigua de OpenSSL y SSLeay. Su cláusula de publicidad es incompatible con la GPL según la FSF.

La constitución dice que la licencia no condiciona la elección de herramientas: si la mejor tiene una licencia incompatible, se revisa la del proyecto.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Permiso adicional, al amparo de la sección 7 de la AGPLv3, para enlazar con OpenSSL y sus derivadas** | Conserva el copyleft en red elegido para Cerbero; es el mecanismo habitual de los proyectos GPL que enlazan con OpenSSL; no cambia nada más de la licencia | Un texto adicional que mantener; quien redistribuya un derivado y quiera conservar el permiso tiene que mantenerlo |
| Licencia permisiva (Apache-2.0 o MIT) | Sin conflictos con ninguna dependencia | Se pierde el copyleft en red |
| Evitar las dependencias afectadas (OpenSSL 3 o RustCrypto) | La licencia no cambia | Contradice la constitución, porque sería la licencia la que elige la herramienta, y son opciones con menos garantías |

## Decisión

**La `AGPL-3.0-or-later` de Cerbero gana un permiso adicional, al amparo de su sección 7, para enlazar o combinar Cerbero con OpenSSL o con librerías derivadas de ella (como BoringSSL, AWS-LC o ring) y distribuir el resultado.**

El texto, en inglés como la licencia, está en [LICENSE-EXCEPTION](../../LICENSE-EXCEPTION). Sigue la forma que propone la FSF para bibliotecas incompatibles con la GPL.

## Consecuencias

- `LICENSE` sigue siendo el texto íntegro de la AGPL; el permiso está en `LICENSE-EXCEPTION`, y el README lo enlaza.
- Los manifiestos siguen declarando `AGPL-3.0-or-later`: SPDX no tiene un identificador genérico para este permiso, así que se documenta en `LICENSE-EXCEPTION`.
- La política de licencias de cargo-deny, que se crea con la migración, admite la licencia de OpenSSL solo en las crates afectadas: aws-lc-sys y, si llegara a usarse, ring.
- El permiso lo concede el titular de los derechos de Cerbero; las contribuciones se aceptan bajo la misma licencia, con el permiso.
- No es asesoramiento legal: es el mecanismo que la FSF propone para estos casos.
