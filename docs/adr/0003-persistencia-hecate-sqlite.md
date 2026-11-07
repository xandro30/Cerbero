# 0003 — Persistencia de Hécate en SQLite

Estado: aceptada · Fecha: 2026-11-07

## Contexto

Hécate es la raíz de confianza de Cerbero: guarda la raíz cifrada (por frase y por partes de recuperación), el
contador de intentos fallidos y el registro de accesos, y más adelante sus aggregates event-sourced. Corre como
una sola instancia, sin alta disponibilidad, en el servidor de casa. La persistencia por proyecto está abierta
en [DECISIONS](../../DECISIONS.md); el arranque de Hécate (feature 001) es el primero que la necesita.

Requisitos que condicionan:

- Cambios atómicos: un corte de luz no puede dejar la protección de la raíz a medias.
- Hécate no puede depender de otro servicio para arrancar: cualquier otro almacén necesitaría credenciales,
  y las credenciales las custodia Hécate.
- Volumen pequeño y un solo escritor.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **SQLite** | Transacciones ACID en un fichero local; en la librería estándar de Python; sin servicio que operar ni credenciales; copia de seguridad sencilla | Un solo escritor (suficiente para Hécate); no se comparte con otros componentes |
| PostgreSQL | Mismo motor que podrían usar otros componentes | Hécate dependería de un servicio que a su vez necesita secretos; más operación |
| Ficheros con renombrado atómico | Sin motor | Más código propio para la atomicidad y el registro append-only |

## Decisión

**SQLite para Hécate**, en un fichero dentro de su directorio de datos (solo accesible por su usuario). El
registro de accesos es una tabla append-only. La decisión vale solo para Hécate; la persistencia del resto de
componentes sigue abierta.

## Consecuencias

- Toda modificación de la protección de la raíz o del contador va en una transacción.
- La copia de seguridad de Hécate es copiar su fichero de forma consistente (API de backup de SQLite); sin la
  frase o las partes no sirve, a propósito.
- Si algún día Hécate necesita más de un escritor o alta disponibilidad, este ADR se sustituye.
