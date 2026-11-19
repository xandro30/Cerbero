# La seguridad de Hermes

Frente a qué protege Hermes y con qué: su identidad, sus superficies, el proxy, la autenticación y el acceso, los
secretos, la auditoría, los datos personales y los logs, qué datos guarda y dónde, y los riesgos aceptados. Es para
quien quiere revisar la seguridad de Hermes; cada mecanismo se desarrolla en su documento, al que enlaza la tabla.

## Frente a qué protege

- alguien en la red interna sin un certificado de Cerbero;
- un componente de Cerbero sin acceso a Hermes;
- un token robado o repetido;
- una petición que intenta saltarse a Envoy o falsear el certificado que declara;
- alguien que se hace pasar por Hécate ante Hermes;
- una fuga de secretos o de datos personales;
- un error del admin al configurar;
- una reescritura de la auditoría.

## Qué lo cubre

| Amenaza | Mecanismo | Dónde se desarrolla |
|---|---|---|
| Alguien sin un certificado de Cerbero | mTLS en Envoy, y la cadena, el nombre y la revocación del certificado, comprobados por Hermes | [La autenticación y el acceso](#la-autenticación-y-el-acceso) |
| Un componente sin acceso | La lista de acceso de Hermes, que guarda Hécate; el token no concede nada | [La autenticación y el acceso](#la-autenticación-y-el-acceso) |
| Un token robado o repetido | Un token corto firmado con una clave de la identidad del certificado, para Hermes, de 300 s como mucho y con un `jti` de un solo uso | [La autenticación y el acceso](#la-autenticación-y-el-acceso) |
| Una petición que se salta a Envoy o falsea la cabecera del certificado | El socket de la API, que solo alcanza Envoy, y la cabecera, que Envoy sustituye | [El proxy](#el-proxy) |
| Alguien que se hace pasar por Hécate | TLS fijado a la CA raíz guardada y la identidad de Hécate comprobada antes de enviar nada | [La identidad de Hermes](#la-identidad-de-hermes) |
| Una fuga de secretos | Referencias en la configuración y valores solo en memoria, nunca en logs, eventos, la base ni lo exportado | [Los secretos](#los-secretos) |
| Una fuga de direcciones de los contactos | Datos personales solo en la base de Hermes | [Los datos personales](#los-datos-personales) |
| Un error del admin al configurar | La validación completa antes de escribir nada, el plan con confirmación y la aplicación en una sola transacción | [Configuración](configuracion.md#la-validación) |
| Una reescritura de la auditoría | Un registro append-only y sus checkpoints firmados por Hécate | [La auditoría](#la-auditoría) |
| Un secreto o una dirección en un log | Los logs enmascarados | [Los logs](#los-logs) |

## La identidad de Hermes

- **`spiffe://cerbero.home/hermes`**, que le da Hécate con un alta con token de un solo uso (`hermes enroll`), y que
  renueva a los dos tercios de su vida, repitiendo la misma petición si se pierde la respuesta.
- **Las publicaciones de Hécate** (las listas de revocación, las claves publicadas y su lista de acceso) se refrescan
  cada 240 s, y la última copia vale mientras Hécate no responde.
- **Una identidad que Hécate rechaza deja de valer:** si Hécate responde `auth` a una llamada hecha con su certificado,
  como tras una revocación urgente, Hermes no está listo y no vuelve a llamar a Hécate con él hasta un alta nueva o un
  reinicio, que lo comprueba con una sola llamada. Así, el registro de accesos de Hécate deja de ganar filas de Hermes
  con ese certificado y muestra a quien use la clave robada
  ([la identidad de Hermes deja de valer](operacion.md#la-identidad-de-hermes-deja-de-valer)).
- **Hermes habla con Hécate** con TLS fijado a la CA raíz guardada, y comprueba que el servidor es
  `spiffe://<trust domain>/hecate` antes de enviar nada.
- Ver [Identidad y seguridad](../../ARCHITECTURE.md#6-identidad-y-seguridad) y las
  [identidades de Hécate](../hecate/identidades.md).

## Las superficies

- **La API de servicios**, solo para los componentes internos y a través de Envoy.
- **La gestión y la salud**, por el canal local: un socket en un directorio 0700, sin superficie de red. No usa la lista
  de acceso: su seguridad es el acceso local al equipo de Hermes.
- Nada de Hermes se expone directamente a internet.

## El proxy

Hermes no escucha en ningún puerto: su API es un socket Unix 0600 que solo alcanza Envoy, que termina el mTLS. Lo que se
exige al proxy, con la configuración de pruebas de `hermes/deploy/envoy/` como referencia:

| Qué | Por qué |
|---|---|
| TLS 1.3, con el certificado y la clave de Hermes de `HERMES_PROXY_DIR/current/` | Es Hermes quien presenta su identidad |
| Recargar el certificado renovado sin reiniciarse ni cortar las conexiones en curso | Hermes escribe cada generación en `gen-<n>/` y cambia el enlace `current` de una vez |
| Pedir el certificado de cliente sin rechazar ninguno | Hermes valida la cadena, la revocación y el token, y el rechazo llega en el sobre común, con `auth` |
| Sin reanudar sesiones TLS | Cada conexión presenta su certificado |
| Sustituir `X-Forwarded-Client-Cert` por la del certificado de la conexión (`SANITIZE_SET`, con el certificado, la cadena y el URI) | La que traiga la petición nunca llega a Hermes |
| Reenviar por el socket de `HERMES_RUN_DIR`, con el mismo usuario que Hermes, y montar solo `HERMES_PROXY_DIR` y `HERMES_RUN_DIR` | Envoy no ve el estado de Hermes ni su canal local |
| Sin interfaz de administración y con logs de acceso sin cabeceras | Nunca el certificado ni el token |

La cabecera trae un solo elemento con `Hash` (el SHA-256 de `Cert`), `Cert`, `Chain`, `URI` (el SPIFFE ID del
certificado) y, si viene, `By` (el de Hermes). Sin cabecera, con más de un elemento, sin uno de esos campos o con un
valor que no se decodifica, la llamada responde `auth`. Envoy no valida la cadena de `Chain`: la valida Hermes entera.

## La autenticación y el acceso

Cada operación de la API exige el certificado de cliente de un componente de Cerbero, que reenvía Envoy, y un token
corto en `Authorization: Bearer`, con el formato de la
[guía de integración de Hécate](../hecate/integracion.md#cada-llamada-autenticada), el `kid` de su clave de firma en la
cabecera y `aud` igual al SPIFFE ID de Hermes.
Hermes comprueba, por orden:

1. que la cadena llega a la CA raíz vigente de Hécate, que el certificado tiene como único nombre un SPIFFE ID de su
   trust domain, que no ha caducado y que su serie no está en la lista de revocación de su intermedia;
2. que el token lo firmó una clave de firma de llamadas activa de la identidad del certificado, de las que publica
   Hécate; si no la tiene, porque el componente se acaba de dar de alta o de renovar, se la pide a Hécate;
3. que `iss` es el SPIFFE ID del certificado y `aud` el de Hermes, y que vive 300 s como mucho, con 30 s de margen
   entre relojes;
4. que la identidad del certificado está en su lista de acceso, que guarda Hécate. Si no está en su copia, se la vuelve
   a pedir a Hécate antes de responder, con una sola petición para las llamadas que llegan a la vez, así que una
   concesión vale en la llamada siguiente; si Hécate no responde, `unavailable`;
5. y que su `jti` no se ha usado antes, también tras reiniciar: quien no tiene acceso no deja su `jti` guardado.

- **Un solo mensaje:** cualquier fallo responde `auth` con el mismo mensaje, y el motivo solo va al log (`auth.refused`,
  con el `request_id` de la petición; `not_allowed` si quien llama no tiene acceso).
- **El token no concede nada:** lo que diga sobre permisos, roles o accesos no cuenta, y Hermes decide con la lista de
  Hécate ([ADR 0013](../adr/0013-acceso-por-app.md)). Sin nadie en ella, no atiende a nadie.
- **Una retirada** del acceso vale en el siguiente refresco de la lista, como mucho a los 240 s.
- Dónde va esta comprobación entre las de cada función está en
  [contratos](contratos.md#chatsend_message), y lo que hace quien llama, en la [guía de integración](integracion.md).

## Los secretos

- **Referencias:** los secretos de las instancias son `secret://hermes/<proveedor>/<instancia>/<campo>`, y la
  contraseña de la base, `secret://hermes/postgres/password`, del ámbito de Hermes en Hécate, que los custodia.
- **Solo en memoria:** Hermes los resuelve al usarlos, y nunca van a los logs, los eventos, la base ni lo exportado.
- **La caché**, con los tiempos que da Hécate, los refresca a los 5 minutos y los olvida a la hora: con Hécate sin
  responder, Hermes sigue con el último valor hasta entonces, y después falla con `internal` sin llegar al proveedor.

## La auditoría

- **Qué registra:** las llamadas con `side_effect` o `discloses_data`, con el texto resuelto de su descripción, las
  aplicaciones de la configuración con sus cambios, los códigos pedidos y los canales verificados.
- **Append-only:** el registro vive en la base de Hermes, y un trigger rechaza cambiar o borrar sus filas. Sus
  checkpoints los firma Hécate.
- **Con su contexto:** cada entrada guarda el que propaga quien llama en `cerbero_audit` (`source`, la Response, el
  Playbook o el Job, el owner, el maintainer o la persona), tal cual, y la clave de idempotencia. La gestión queda
  registrada con `actor: admin`.
- **En la misma transacción** que el cambio que registra: si la entrada no se puede escribir, el cambio no queda. Un
  envío que un corte deja a medias también queda atribuido ([llamadas](llamadas.md#la-idempotencia)).

## Los datos personales

- **Las direcciones** de los contactos, el chat de cada canal verificado, solo viven en la base de Hermes: nunca en el
  repositorio, en lo exportado, en las respuestas, en los logs ni en Hades.
- **El fichero de la configuración** lleva los contactos y los valores de las variables, así que nunca va a un
  repositorio ([configuración](configuracion.md#dónde-vive)).
- **Lo que sale a un tercero** lo dice el `discloses_data` de cada función: en `chat.send_message`, el proveedor recibe
  el texto del mensaje y el chat del contacto.

## Los logs

- **JSON a la salida estándar,** una línea por evento, con secretos, tokens, certificados, códigos y direcciones
  enmascarados.
- **Una línea por petición:** cada petición a la API deja al terminar una línea `api.request` con su `request_id`, la
  operación, quien llama, la clave de idempotencia, el resultado y el estado HTTP, y el canal local deja una línea por
  operación.
- **El `request_id`** va en toda línea que deja una petición, así que el que recibe quien llama lleva a todo lo que
  pasó.

## Qué datos guarda y dónde

| Dato | Dónde | Copia |
|---|---|---|
| Registro append-only de llamadas con `side_effect` o `discloses_data`, aplicaciones de la configuración, códigos pedidos y canales verificados | Base de datos de Hermes; un trigger rechaza cambiar o borrar sus filas | Sí, lo más importante (auditoría) |
| Configuración: instancias, plantillas, variables y contactos | Base de datos de Hermes, en el PostgreSQL común ([ADR 0012](../adr/0012-persistencia-postgresql-comun.md)); la aplica el admin con la CLI | Sí, dentro de la copia de la base de datos |
| Estado de verificación de los canales y sus direcciones | Base de datos de Hermes | Sí |
| Claves de idempotencia y `jti` usados | Base de datos de Hermes; las claves, 24 horas, y cada `jti`, hasta que caduca su token | Sí |
| Event store de las tasks | Base de datos de Hermes | Sí |
| Librerías propias | Repositorio privado (ruta `hermes/`); se cambian por PR desde Ténaro | Es el propio repositorio |
| Identidad: clave de firma de llamadas, metadatos, renovación en curso y últimas publicaciones de Hécate | Directorio de estado (`HERMES_STATE_DIR`), 0700 con ficheros 0600 | Se recupera con un alta nueva |
| Copia de su lista de acceso: las identidades que pueden llamarlo, la versión y cuándo la obtuvo (`access.json`) | Directorio de estado, 0600 | Se vuelve a pedir a Hécate |
| Certificado, cadena, clave TLS y CA raíz | Directorio del proxy (`HERMES_PROXY_DIR`), 0700 con ficheros 0600, en `gen-<n>/` con el enlace `current`; lo lee Envoy, con el mismo usuario | Se recupera con un alta nueva |
| Secretos resueltos, estado de las instancias, fichas de los límites, cerrojos de las claves, cursores del polling | Memoria | No: se reconstruyen o se empieza desde "ahora" |
| Código, contratos y manifiestos de las librerías oficiales | Repositorio de Cerbero; no se editan desde Ténaro | Es el propio repositorio |

Cómo se hacen las copias está en [operación](operacion.md).

## Lo que no cubre y los riesgos aceptados

- **El token ligado a la identidad que declara el proxy:** quien llegara al socket de la API sin pasar por Envoy podría
  declarar otra. Lo acota que solo Envoy lo alcance ([El proxy](#el-proxy), [ADR 0011](../adr/0011-pila-hermes.md)).
- **Los secretos en la memoria de un proceso Python**, que no puede borrarlos: nunca van a disco ni a los logs, y la
  caché los acota.
- **El acceso local al equipo de Hermes:** quien llega al canal local gestiona Hermes; su protección es la del equipo.
- **La revocación urgente de su propio certificado** se nota en la primera llamada que Hécate le rechaza, como mucho a
  los 240 s, porque Hécate ya no le da la lista de revocación que la recoge: hasta entonces, Hermes sigue atendiendo con
  ese certificado ([la identidad de Hermes deja de valer](operacion.md#la-identidad-de-hermes-deja-de-valer)).
- El detalle y el porqué de los riesgos aceptados están en [DECISIONS](../../DECISIONS.md#riesgos-aceptados).

## Relacionado

- [Hermes](../hermes.md): el documento de alto nivel.
- [Integrar un componente](integracion.md): la autenticación desde el lado de quien llama.
- [Configuración](configuracion.md): la validación y la aplicación de la configuración.
- [La seguridad de Hécate](../hecate/seguridad.md): la identidad, los secretos y la lista de acceso que guarda Hécate.
- Envoy: [`X-Forwarded-Client-Cert`][xfcc]

[xfcc]: https://www.envoyproxy.io/docs/envoy/latest/configuration/http/http_conn_man/headers#x-forwarded-client-cert
