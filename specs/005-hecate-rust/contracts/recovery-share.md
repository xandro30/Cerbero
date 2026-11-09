# Contrato: partes de recuperación `HEC2`

Sustituye al formato `HEC1` de la feature 001. Cambia la división de Shamir (de dos mitades en GF(2^128) a GF(256), ADR
0007) y la versión; se conservan la estructura, la detección de errores de copia y los mensajes.

## Texto

```text
HEC2-XXXX-XXXX-…-XXXX
```

- Cuerpo de 42 bytes: versión `0x02`, índice (1 a 3), identificador del juego (4 bytes) y los 32 valores de la parte en
  GF(256).
- Suma de comprobación de 4 bytes: los primeros de SHA-256 del cuerpo.
- Cuerpo y suma en Crockford Base32 (`0123456789ABCDEFGHJKMNPQRSTVWXYZ`), en grupos de cuatro separados por guiones y
  detrás del prefijo `HEC2`.

## Lectura

1. Se quitan los espacios y los guiones, se pasa a mayúsculas y se corrigen las confusiones `O`→`0` e `I`/`L`→`1`.
2. Debe empezar por `HEC2`; si no, `validation`: «recovery share must start with HEC2» (una `HEC1` también).
3. Solo caracteres del alfabeto y la longitud exacta; si no, `validation`.
4. La suma debe coincidir; si no, `validation`: «recovery share checksum does not match: check how it was copied».
5. La versión debe ser 2 y el índice, de 1 a 3; si no, `validation`.

Ningún `validation` cuenta como intento fallido; tampoco dos partes con el mismo índice, que dan `validation`. Dos
partes del mismo juego y distintas recuperan la clave; partes de juegos distintos o que no recuperan la raíz dan `auth` y
cuentan como intento, como en la feature 001. Los demás mensajes son los de la versión en Python.
