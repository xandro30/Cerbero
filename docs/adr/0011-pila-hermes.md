# 0011 — Pila de Hermes: FastAPI detrás de Envoy

Estado: aceptada · Fecha: 2026-11-14

## Contexto

La feature 008 construye Hermes, el primer servicio de Cerbero en Python con una API entre componentes. El
[ADR 0005](0005-rust-para-orthros-y-hecate.md) lo deja en Python, y docs/hermes.md prevé FastAPI. Lo condicionan:

- la constitución (principio V): mTLS con identidad SPIFFE y un JWT corto firmado por quien llama y ligado al canal, así
  que Hermes necesita en cada petición el certificado del cliente;
- uvicorn no pasa ese certificado a la aplicación: es la decisión abierta «APIs internas en Python y certificado del
  cliente» de DECISIONS;
- la decisión del propietario: FastAPI detrás de un proxy TLS que termina el mTLS y reenvía el certificado del cliente
  en una cabecera, como `X-Forwarded-Client-Cert` de Envoy, con tres condiciones: Hermes no publica su puerto, el token
  corto se liga a la identidad que declara el proxy (riesgo aceptado), y la decisión va con la del despliegue en el home
  lab;
- la [guía de integración de Hécate](../hecate/integracion.md): alta con la huella de la CA raíz, renovación repitiendo
  la misma petición, token corto EdDSA, lista de revocación X.509 y claves publicadas;
- el proveedor de chat elegido, Telegram, cuyo cliente tiene que distinguir si una petición llegó a salir.

## Opciones

### Certificado del cliente en Python

| Opción | A favor | En contra |
|---|---|---|
| **Proxy TLS que reenvía el certificado** | FastAPI y uvicorn sin cambios; un proxy maduro hace el TLS | Una pieza más; el token se liga a lo que declara el proxy |
| uvicorn adaptado | Sin pieza nueva | Depende de detalles internos de uvicorn que cambian entre versiones |
| aiohttp | Expone el certificado | Sale de FastAPI y de su validación |

### Proxy

| Opción | A favor | En contra |
|---|---|---|
| **Envoy** | Reenvía la cadena del cliente (`Chain`), sustituye la cabecera del cliente (`SANITIZE_SET`), acepta certificados sin verificarlos para que valide Hermes, recarga el certificado vigilando un directorio y habla con Hermes por un socket Unix | Configuración más larga; escrito en C++ |
| Traefik | Escrito en Go; configuración más corta | Su documentación no dice si pasa la cadena ni si quita la cabecera del cliente |
| Caddy | Escrito en Go; configuración muy corta | Solo pasa el certificado hoja, sin la cadena |

### Enlace entre el proxy y Hermes

| Opción | A favor | En contra |
|---|---|---|
| **Socket Unix** | Hermes no abre ningún puerto TCP; solo llega quien puede abrir el fichero | Proxy y Hermes en el mismo equipo |
| Red interna de docker-compose | Separa los contenedores | Un puerto TCP, aunque no se publique |
| mTLS entre los dos | Funciona entre equipos | Otro par de certificados que gestionar |

### Lo demás

| Pieza | Elegida | Descartada |
|---|---|---|
| Cliente HTTP y Telegram | httpx contra la Bot API: distingue si la petición salió | python-telegram-bot y aiogram, con sus propios reintentos y bucles |
| X.509 y token corto | cryptography (verificación de clientes desde la 43) y PyJWT con EdDSA | joserfc y authlib, equivalentes y menos adoptadas |
| Fichero de configuración | YAML con PyYAML en modo seguro y modelos estrictos de Pydantic | TOML, verboso para listas anidadas; JSON, sin texto de varias líneas |
| Plantillas | Sustitución propia de `${...}`, sin lógica | Jinja2 en sandbox, con más superficie de ataque |
| CLI | argparse, de la biblioteca estándar | typer o click, una dependencia para cinco órdenes |

## Decisión

- **Servicio:** Python 3.15 con uv y `uv_build`; FastAPI 0.162 con Pydantic 2.14, servido por uvicorn 0.54 en sockets
  Unix: uno para la API y otro para el canal local de administración, en el mismo proceso.
- **Proxy:** Envoy, en la última versión estable al implementar, delante de Hermes en el mismo equipo y con el mismo
  usuario. TLS 1.3 con el certificado de Hermes, recargado por SDS de ficheros con `watched_directory`; certificado de
  cliente pedido sin exigirlo y aceptado sin verificar (`ACCEPT_UNTRUSTED`); sin reanudar sesiones; cabecera
  `X-Forwarded-Client-Cert` con `SANITIZE_SET`, el certificado, la cadena y el SPIFFE ID; reenvío por el socket Unix de
  la API.
- **Hermes valida todo lo demás:** la cadena con `cryptography.x509.verification`, la lista de revocación de Hécate, el
  SPIFFE ID y el token corto con PyJWT 2.15, con los tiempos y el `jti` comprobados por código propio.
- **Cliente de Hécate y de Telegram:** httpx 0.28; las llamadas a Hécate exigen su SPIFFE ID tras el saludo TLS y antes
  de enviar nada. Claves y solicitudes con cryptography 50.
- **Persistencia:** la del [ADR 0012](0012-persistencia-postgresql-comun.md), con SQLAlchemy 2.1 Core en modo asíncrono
  sobre psycopg 3.3 y migraciones de Alembic 1.20, sin ORM.
- **Configuración:** YAML con PyYAML 6 (sin alias, anclas ni claves repetidas) y Pydantic; los esquemas de las
  plantillas, con jsonschema 4.
- **Contrato:** OpenAPI 3.1 en JSON en `shared/contracts`, con el sobre común; pruebas de contrato con jsonschema y
  referencing.
- **Pruebas y calidad:** pytest e hypothesis ([ADR 0004](0004-pruebas-pytest-hypothesis.md)), anyio, respx y
  testcontainers; ruff, mypy en modo estricto y pip-audit.
- **Versiones:** la última estable de cada dependencia, fijadas en `uv.lock`.

## Consecuencias

- Resuelve «APIs internas en Python y certificado del cliente» para Hermes y deja la base que reutilizarán los demás
  servicios en Python con API.
- El token corto se liga a la identidad que declara Envoy, no a la conexión TLS que ve Hermes: riesgo aceptado, acotado
  porque solo Envoy llega al socket de la API y sustituye la cabecera. Se registra en DECISIONS al cerrar la 008.
- Envoy lee la clave TLS de Hermes, así que es parte de su frontera. No ve la clave de firma de llamadas ni el canal de
  administración.
- Cómo corren Hermes y Envoy en el home lab va con la decisión abierta del despliegue; el compose de la feature es solo
  para pruebas.
- Los secretos que Hermes resuelve viven en la memoria de Python, que no se puede borrar: riesgo aceptado al elegir
  Python, que se registra en DECISIONS al cerrar la 008.
