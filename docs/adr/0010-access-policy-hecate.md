# 0010 — Access Policy de Hécate

Estado: aceptada · Fecha: 2026-11-11

## Contexto

La feature 006 abre la resolución de secretos a los componentes por la API mTLS de la feature 003
(`POST /v1/secrets:resolve`). Hace falta decidir qué secretos y qué operaciones puede usar cada identidad, y cómo se
comparte un secreto entre dos componentes. Lo condicionan:

- la constitución (principio VII): OpenBao / Vault como referencia, que deniega por defecto y da permisos con políticas
  por ruta asociadas a tokens;
- el modelo de Hécate: aggregates event-sourced sin valores en los eventos, y un ámbito de cifrado por identidad que
  coincide con su ámbito de acceso;
- la auditoría: cada cambio de permisos tiene que quedar con quién lo hizo, y tiene que poder saberse qué podía leer
  una identidad en un momento pasado;
- quien mide es un componente autenticado, desde la red interna y con tantas peticiones como quiera: un rechazo no puede
  revelar si un secreto existe, si es de otra identidad o si no tiene una versión resoluble, ni por su contenido ni por
  su tiempo;
- un componente necesita conocer el estado de Hécate (sellada, secretos por rotar o caducar, certificados por caducar)
  para avisar, sin ver rutas ni metadatos de un secreto concreto.

## Opciones

### Dónde vive la política

| Opción | A favor | En contra |
|---|---|---|
| **Un aggregate por identidad** (stream `policy/<identidad>`) | El historial con quién y cuándo es el propio stream; qué podía leer en un momento es aplicar los eventos hasta ese instante | Un stream más por identidad con cambios |
| Una tabla de permisos | Consulta directa | Pierde el historial, o hay que llevarlo aparte |
| Una política global para todas las identidades | Un solo sitio | Un cambio en una identidad versiona la de todas |

### Cómo se comparte un secreto

| Opción | A favor | En contra |
|---|---|---|
| **Una excepción por secreto concreto**, que tiene que existir al concederla | No abre más de lo que nombra y se revisa de un vistazo | Una orden por secreto compartido |
| Prefijos o comodines, como las políticas de OpenBao | Una sola regla cubre muchos secretos | Abre también los secretos que se creen después bajo esa ruta, y cuesta saber qué alcanza |
| Políticas asociadas a tokens, como en OpenBao | Permisos distintos por token | Quien llama ya se identifica con su certificado y su token corto; sería una segunda fuente de permisos |

### Cómo se igualan los rechazos

| Opción | A favor | En contra |
|---|---|---|
| **Comprobar todas las referencias antes de descifrar y responder todo `not_found` con un tiempo mínimo** | Un rechazo no abre ninguna clave, y la diferencia de trabajo queda por debajo del mínimo | Cada rechazo tarda al menos ese mínimo |
| Hacer el mismo trabajo en todos los casos, con descifrados falsos | Sin espera | Cualquier rama distinta vuelve a filtrar tiempo y es frágil de mantener |
| Un retraso aleatorio | Sin espera fija | Con muchas muestras se promedia y no oculta la diferencia |

### Qué ve un componente del estado

| Opción | A favor | En contra |
|---|---|---|
| **Un permiso de estado aparte, con recuentos agregados**, que también responde sellada | El componente avisa sin ver nombres, y sabe que Hécate está sellada | Una operación y un permiso más |
| El mismo `status` que ve el admin | Una sola respuesta | Lleva la huella de la CA raíz y la serie del certificado de Hécate, que un componente no necesita |

## Decisión

- **Regla por defecto:** cada identidad resuelve solo `secret://<su identidad>/**`; lo demás está denegado. La regla no
  se guarda: una identidad sin cambios de política solo lee lo suyo.
- **Access Policy:** un aggregate event-sourced por identidad, en el stream `policy/<identidad>`, con los eventos
  `ReadGranted` y `ReadRevoked` (la ruta y quién) y `StatusGranted` y `StatusRevoked` (quién). Su versión es la del
  stream y su historial, sus eventos.
- **Excepciones por secreto concreto:** el admin concede a una identidad la lectura de un secreto de otra identidad,
  nombrado por su ruta, sin `@N`, sin prefijos ni comodines, y que exista al concederla. No se conceden a `hecate` ni a
  `admin`, ni para los secretos de la propia identidad. Retirar una excepción vale desde la petición siguiente: la
  resolución lee la política una vez por petición. Un secreto compartido se descifra con la clave de ámbito de su dueño
  y sigue sus reglas de versiones y caducidad.
- **Permiso de estado:** deja consultar `GET /v1/status`, que responde también sellada, con si Hécate está sellada y
  los recuentos agregados de secretos y certificados. Nunca rutas, nombres ni metadatos de un secreto concreto.
- **Órdenes del admin:** por el canal local y la CLI (`hecate policies ...`), sin versión esperada; una que no cambia
  nada no escribe eventos, pero deja su fila. Exigen Hécate desbloqueada, como el resto de la gestión.
- **Rechazos que no se distinguen:** la resolución comprueba cada referencia (permiso, existencia, versión resoluble y
  material guardado) antes de descifrar ninguna. Un secreto que no existe, uno de otra identidad sin excepción y uno sin
  versión resoluble dan el mismo `not_found`, con el mismo mensaje, que no se responde antes de 250 ms desde que la
  petición llega a la operación; la duración que indica el sobre incluye la espera.

## Consecuencias

- El historial de la política responde quién compartió qué y cuándo, y qué podía leer una identidad en cualquier
  momento pasado.
- Compartir varios secretos son varias órdenes, y un secreto nuevo bajo la misma ruta no queda compartido hasta que se
  conceda.
- Cada `not_found` de la resolución tarda al menos 250 ms. La primera fase con 100 referencias tiene que quedar muy por
  debajo de ese mínimo, y una prueba lo vigila.
- Las desviaciones frente a OpenBao / Vault quedan en [DECISIONS](../../DECISIONS.md#hécate-frente-a-sus-referencias-openbao--vault-step-ca-spiffe).
