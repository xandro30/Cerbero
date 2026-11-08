# 0005 — Rust para Orthros y Hécate; Python para el resto

Estado: aceptada · Fecha: 2026-11-08 · Sustituye a [0002](0002-lenguajes-por-proyecto.md)

## Contexto

El [ADR 0002](0002-lenguajes-por-proyecto.md) puso Eurytion en Rust, por ser un agente que corre en todos los equipos y se distribuye como binario, y el resto de piezas en Python. Al planificar la feature 003 de Hécate (CA interna y su primera API entre componentes, por mTLS) aparecieron dos límites de Python en la pieza más sensible de Cerbero:

- **El certificado del cliente.** La constitución (principio V) exige que el receptor ligue el token corto de cada llamada al canal, así que la aplicación tiene que leer en cada petición el certificado que presentó el cliente. uvicorn, el servidor de FastAPI previsto para Hermes, no se lo pasa a la aplicación.
- **Los secretos en memoria.** Python no permite borrar de la memoria los secretos de paso (frases, DEK, valores descifrados): es un riesgo aceptado en DECISIONS.

En Orthros, DECISIONS tenía abierto el lenguaje del NSM y del control plane. El NSM analiza tráfico hostil a velocidad de red, y las tres piezas comparten el modelo de Policy, su firma y el motor de Parsers; el control plane, además, tiene que poder ejecutar ese motor para validar un Parser antes de publicarlo.

Dos factores no inclinan la balanza. El objetivo del propietario con Hécate (aprender a aplicar criptografía y CA y ver los análisis de seguridad que salen) se cumple en los dos lenguajes, y el tiempo de la migración no es crítico. Tampoco Shamir: ni en Python (PyCryptodome) ni en Rust hay una implementación con auditoría pública confirmada.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Rust para Orthros entero y Hécate; Python para el resto** | Hécate borra los secretos de paso al usarlos (`zeroize`), lee el certificado del cliente con rustls y su verificador propio, y expresa sellada y desbloqueada como tipos. Orthros comparte en crates el modelo de Policy, la firma y el motor de Parsers, y el NSM analiza tráfico hostil con seguridad de memoria y rendimiento | Portar Hécate (features 001 y 002); la cadena de herramientas de Rust pesa más en el repositorio; la API de Hécate no usa el stack previsto para Hermes |
| Hécate en Python con aiohttp; Orthros como en el 0002 | Sin migración | Se mantiene el riesgo de memoria; un stack HTTP distinto solo para leer el certificado; el lenguaje de Orthros sigue abierto |
| Núcleo criptográfico de Hécate en Rust como extensión de Python | Cierra buena parte del riesgo de memoria sin reescribir Hécate | Dos lenguajes y una frontera dentro del núcleo criptográfico; compilar Rust dentro de un miembro de uv |
| Solo el NSM en Rust, junto a Eurytion; control plane y Hécate en Python | Rust donde el rendimiento más importa | El control plane reimplementa el modelo de Policy y no puede ejecutar el motor de Parsers; Hécate mantiene el riesgo de memoria |

## Decisión

**Rust (edition 2024) para Orthros entero (Eurytion, NSM y control plane) y para Hécate; Python 3.15, gestionado con uv, para Echidna, Estigia, Caronte, Hades, Hermes y Ténaro.**

En Hécate pesan el control de los secretos en memoria y el acceso nativo al certificado del cliente; en Orthros, el análisis de tráfico hostil y los bloques compartidos entre sus tres piezas. El resto sigue en Python por legibilidad, mantenibilidad y escalabilidad, y por las herramientas de su ecosistema: son servicios con dominio rico, donde esas cualidades pesan más que lo que aporta Rust.

En las piezas en Rust, todo el código propio es Rust y sus dependencias son crates seguras y maduras, aunque envuelvan una herramienta escrita en C; no se escribe C propio ni se montan bindings a mano (constitución, «Herramientas»).

## Consecuencias

- **Repositorio:** Orthros y Hécate son workspaces de Cargo en `orthros/` y `hecate/`, fuera del workspace de uv. Eurytion pasa a ser un crate del workspace de Orthros, y el NSM y el control plane dejan de ser miembros de uv. Complementa al [ADR 0001](0001-monorepo-uv-workspaces.md), que sigue vigente para los proyectos en Python y no se edita.
- **Hécate se porta** con la paridad de comportamiento de las features 001 y 002: mismos contratos del canal local y mismo modelo de datos. Las partes de recuperación cambian de formato, porque Shamir pasa a vsss-rs (GF(256)), y los datos de desarrollo se inicializan de nuevo.
- **Contratos:** siguen en `shared/contracts` y deben poder usarse desde Rust y desde Python (constitución).
- **Decisiones que arrastra:** las pruebas en Rust ([ADR 0006](0006-pruebas-rust-cargo-proptest.md); el [0004](0004-pruebas-pytest-hypothesis.md) es de Python), la pila de Hécate en Rust ([ADR 0007](0007-pila-hecate-rust.md)) y el permiso adicional de la licencia para enlazar con OpenSSL y sus derivadas ([ADR 0008](0008-excepcion-openssl-agpl.md)). El stack HTTP y TLS de la API de Hécate se decide con la feature 003.
- **Riesgo de memoria:** la raíz sigue en memoria mientras Hécate está desbloqueada; lo que cambia es que los secretos de paso se borran al usarse, y el riesgo aceptado de DECISIONS se reescribe en esos términos al consolidar la migración.
- **Servicios en Python con API:** Hermes sigue en Python con FastAPI. Al implementar su API mTLS tendrá el mismo problema de acceso al certificado del cliente, que queda como decisión abierta en DECISIONS.
- **Al consolidar la migración:** se actualizan el README (el árbol del repositorio y el párrafo de los proyectos en Python, con su referencia al ADR 0002), el README de Orthros (la tabla de lenguajes), el doc de Hécate (las librerías del `CryptoProvider` en los apartados 1 y 7 y el riesgo de memoria del apartado 12) y el riesgo de memoria de DECISIONS. Hasta entonces describen la implementación actual.
