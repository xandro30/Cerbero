# 0012 — Persistencia de las apps en un PostgreSQL común

Estado: aceptada · Fecha: 2026-11-14

## Contexto

«Persistencia por proyecto» sigue abierta en DECISIONS: Hécate guarda sus datos en SQLite
([ADR 0003](0003-persistencia-hecate-sqlite.md)), y faltan el event store de Hades, el almacenamiento y la búsqueda de
Estigia y el buffer de las cabezas. La feature 008 necesita la persistencia de Hermes: su configuración (instancias,
plantillas, variables y contactos), la verificación de los canales, las claves de idempotencia y el registro de
auditoría. El propietario lo decidió el 2026-11-14. Lo condicionan:

- el principio I: cada componente se despliega y evoluciona por separado, y no comparte su modelo con otros;
- el principio V: ningún secreto se guarda como valor, y los contactos y los valores de variables viven en la base de la
  app que los usa;
- las copias de seguridad, que tienen que cubrir todos los almacenes de Cerbero (otra decisión abierta);
- Hécate custodia los secretos de todos, también las credenciales de una base: no puede depender de esa base para
  arrancar.

## Opciones

| Opción | A favor | En contra |
|---|---|---|
| **Un PostgreSQL común, con una base y un usuario por app** | Un solo sitio para controlar los datos y las copias; transacciones, bloqueos y concurrencia maduros; cada app aislada en su base | Un punto de fallo común; las credenciales de cada base viven en Hécate; un servicio más que operar |
| SQLite por app | Sin servidor; cada app con su fichero | Copias repartidas en ficheros de cada equipo; un solo escritor |
| Un PostgreSQL por app | Aislamiento total | Tantos servidores como apps que operar y copiar |

## Decisión

- **Hécate sigue en SQLite** (ADR 0003): necesita sus datos sin depender de otro servicio, y guarda las credenciales de
  las bases de las demás.
- **Las demás apps, empezando por Hermes, usan un servidor PostgreSQL común,** con una base y un usuario propios por
  app, sin acceso a las bases de las demás. Cada app guarda en su base lo que necesita.
- **La contraseña de cada usuario es un secreto de esa app en Hécate** (en Hermes, `secret://hermes/postgres/password`),
  que la app resuelve con su identidad. Nunca va en una variable de entorno ni en la configuración.
- **El buffer de las cabezas sigue en cada cabeza:** existe para cuando el centro no responde, así que no puede vivir en
  el servidor común.
- **La versión:** la última versión mayor estable de PostgreSQL al implementar cada cambio; la de las pruebas se fija en
  su compose.
- La forma de acceder la decide cada app en su ADR: Hermes, en el [ADR 0011](0011-pila-hermes.md).

## Consecuencias

- Resuelve «Persistencia por proyecto»; DECISIONS lo recoge al cerrar la 008.
- Si el servidor cae, las apps que lo usan dejan de estar listas y responden `unavailable` hasta que vuelve; Hécate
  sigue.
- Las apps dependen de Hécate para conectarse a su base, como ya dependen de ella para su certificado.
- Las copias se hacen en un solo sitio; su política sigue abierta en DECISIONS.
- Cómo corre el servidor en el home lab va con la decisión abierta del despliegue.
