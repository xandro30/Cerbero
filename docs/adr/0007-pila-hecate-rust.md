# 0007 — Pila de Hécate en Rust

Estado: aceptada · Fecha: 2026-11-08

## Contexto

El [ADR 0005](0005-rust-para-orthros-y-hecate.md) pasa Hécate a Rust. Portar las features 001 y 002 necesita:

- un runtime asíncrono para el canal local y las señales, que también usará la API de la feature 003;
- persistencia en SQLite ([ADR 0003](0003-persistencia-hecate-sqlite.md)), CLI, logs estructurados, serialización y errores;
- manejo de secretos en memoria y endurecimiento del proceso: sin volcados de memoria, sin swap y credencial del par en el socket local;
- criptografía: AES-256-GCM, HKDF-SHA256, HMAC-SHA256, aleatorios, Argon2id y Shamir 2 de 3.

Los criterios son los de la constitución («Herramientas»): seguridad, garantías (auditorías, validaciones) y rendimiento; todo el código propio en Rust; dependencias en crates seguras y maduras, aunque envuelvan C, sin C propio ni bindings a mano; y la licencia no condiciona.

## Opciones

### Primitivas criptográficas

| Opción | A favor | En contra |
|---|---|---|
| **aws-lc-rs** | Módulo FIPS 140-3 de AWS-LC validado por el NIST y partes con verificación formal (SAW, HOL Light con s2n-bignum, Fiat-Crypto) que se comprueba en la CI en cada cambio. Ensamblador optimizado. Crate oficial de AWS, muy adoptada, y proveedor por defecto de rustls | Núcleo en C y ensamblador; no tiene Argon2id; necesita compilador de C |
| OpenSSL 3 (crate `openssl`) | Proveedor FIPS validado; lo cubre todo, Argon2id incluido desde la 3.2 | En la feature 003, la TLS sería la de OpenSSL, en C, en lugar de rustls; API de C envuelta, poco idiomática |
| RustCrypto | Todo en Rust | Solo AES-GCM tiene auditoría pública (NCC Group, 2020, sobre una versión antigua); su proveedor para rustls es experimental |
| ring | Muy usada; proveedor alternativo de rustls | Misma API que aws-lc-rs, con menos algoritmos y sin validación FIPS |

### Argon2id

| Opción | A favor | En contra |
|---|---|---|
| **libsodium mediante `libsodium-rs`** | libsodium está auditada (2017, Matthew Green) y su Argon2 está optimizado. El binding seguro lo mantiene el autor de libsodium. Es la misma primitiva, con los mismos parámetros, que la versión en Python usaba con PyNaCl | El binding es joven (desde 2025) y sigue en 0.x |
| `argon2` de RustCrypto | Todo en Rust; muy madura y adoptada | Sin auditoría pública; sin comparativas de rendimiento independientes |
| OpenSSL 3.2 o posterior | Implementación de referencia, con hilos | Una segunda librería criptográfica grande solo para una función |

### Shamir

| Opción | A favor | En contra |
|---|---|---|
| **vsss-rs** | Mantenida; reparte bytes en GF(256), así que una clave de 32 bytes se reparte entera | Dice que tuvo auditorías financiadas, pero sin informe público |
| blahaj | Copia corregida de sharks tras el sesgo que encontró Cure53 (RUSTSEC-2024-0398) | No se ha podido verificar su estado ni su mantenimiento |

Ninguna implementación de Shamir, ni en Rust ni en Python, tiene una auditoría pública confirmada.

## Decisión

- **Primitivas:** aws-lc-rs, con la compilación normal. El módulo validado se activa con la feature `fips`, que exige CMake y Go para compilar; queda como opción.
- **Argon2id:** libsodium mediante `libsodium-rs`, con los mismos perfiles que hoy (`sensitive` por defecto y `moderate`).
- **Shamir:** vsss-rs, repartiendo en GF(256).
- **Runtime:** tokio. **SQLite:** rusqlite, con SQLite incluido en la compilación. **CLI:** clap. **Logs:** tracing con tracing-subscriber, en JSON a stdout.
- **Serialización:** serde y serde_json. **Errores:** thiserror. **Secretos en memoria:** zeroize y secrecy. **Codificación de las partes:** data-encoding (Crockford).
- **Sistema:** nix para `mlockall` y `setrlimit`, y la credencial del par que da tokio en el socket local.
- **Toolchain:** la última versión estable de Rust (hoy 1.99.1, edition 2024), fijada en `rust-toolchain.toml` y actualizada en cada versión estable nueva, salvo que algo se rompa. Las dependencias siguen la misma regla.

aws-lc-rs es la opción con más garantías y la que mejor encaja con la TLS de la feature 003; libsodium aporta para Argon2id la única implementación auditada disponible desde Rust.

## Consecuencias

- Compilar Hécate necesita un compilador de C: AWS-LC, libsodium y SQLite se compilan con ella.
- aws-lc-rs trae código bajo la licencia antigua de OpenSSL; lo cubre el permiso adicional del [ADR 0008](0008-excepcion-openssl-agpl.md).
- Shamir sin implementación auditada queda como riesgo aceptado en DECISIONS.
- Las partes de recuperación cambian de formato: GF(256) en lugar de dos mitades en GF(2^128). Los datos de desarrollo se inicializan de nuevo.
- El modo FIPS no está activado. La compilación normal usa el código principal de AWS-LC, donde corre la verificación formal; el módulo validado es una versión concreta que se activa con `fips`.
- La TLS y la PKI de la API se deciden en el plan de la feature 003. El camino previsto, rustls sobre aws-lc-rs con rcgen, usa estas mismas primitivas.
