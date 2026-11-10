# Casos de uso: Identidades de Hécate

Qué cubre la feature 003, caso por caso. El detalle está en [spec.md](spec.md): 7 historias de usuario con 48
escenarios de aceptación.

Hay dos actores:

- **Admin:** opera desde el equipo de Hécate, por el canal local y la CLI.
- **Componente:** una pieza de Cerbero (Hermes, Hades…) que habla con Hécate por la API mTLS.

## P1: la base

### 1. Crear la CA de Cerbero

Actor: admin (historia 1).

- Es una ceremonia: la CLI genera la CA raíz y con ella firma la intermedia que prepara Hécate.
- La clave raíz nunca entra en Hécate. Sale en una copia protegida con la frase de la CA, y la CA no se activa hasta que
  el admin devuelve esa copia y se abre con la frase.
- Hécate se emite su propio certificado.

### 2. Dar de alta un componente

Actores: admin y componente (historia 2).

- El admin genera un token de un solo uso (15 minutos, se muestra una vez) y puede listarlo o anularlo.
- El componente comprueba a Hécate con la huella que lleva el token y obtiene su primer certificado SPIFFE de 3 días. Al
  darse de alta registra su clave de firma de llamadas.
- Repetir exactamente la misma petición devuelve el mismo certificado, y tras 10 altas fallidas seguidas las altas se
  frenan.

### 3. Renovar el certificado

Actor: componente (historia 3).

- Con su certificado vigente y un token corto, aporta claves nuevas.
- Cada certificado se renueva una sola vez. Un segundo intento se trata como robo de clave y revoca con urgencia todos
  los certificados válidos de esa identidad.

## P2: confianza entre componentes y respuesta a incidentes

### 4. Publicar las claves de firma de llamadas

Actor: componente (historia 4).

- Solo para componentes autenticados.
- Cada clave indica si vale para llamadas nuevas o solo para verificar firmas pasadas. Lo publicado vale 5 minutos.

### 5. Revocar un certificado o deshabilitar una identidad

Actor: admin (historia 5).

- Revocación pasiva (el certificado deja de renovarse) o urgente: entra en la lista de revocación, una CRL firmada que
  vale 5 minutos.
- Deshabilitar, habilitar, listar y consultar identidades.
- Revocar con urgencia el certificado de Hécate, que se reemite al momento.

## P3: mantenimiento de la CA

### 6. Renovar la CA intermedia

Actor: admin (historia 6).

- Es otra ceremonia con la copia de la clave raíz.
- La intermedia anterior queda solo para verificar, y su clave se destruye cuando caduca el último certificado que
  firmó.
- El estado avisa cuando quedan menos de 30 días.

### 7. Sustituir la CA entera

Actor: admin (historia 7).

- Se usa ante una intermedia comprometida o si se pierde la clave raíz o su frase.
- Se demuestra con la frase de desbloqueo o con dos partes, se crea una CA nueva y todo lo anterior deja de valer.
- Cada componente vuelve a darse de alta.

## Lo que atraviesa todos los casos

- Con Hécate sellada, la API sigue respondiendo con el certificado de Hécate, pero solo para decir que está sellada. La
  clave de ese certificado se guarda en claro: es el riesgo aceptado.
- Cada cambio es atómico ante un corte de luz.
- Los secretos de paso se borran de la memoria, y las entradas tienen que seguir su formato estándar.
- Todo queda en el registro de accesos, pero nunca un token, una clave ni una frase.
- `status` da recuentos sin nombres: identidades que caducan en 24 horas, renovaciones sospechosas, días que le quedan a
  la intermedia y si las altas están frenadas.
- Las identidades `hecate` y `admin` están reservadas: no admiten tokens ni secretos. Rechazar sus secretos es el único
  cambio de comportamiento para las features 001 y 002.

## Dónde viven en el código

- [`ca/service.rs`](../../hecate/crates/application/src/ca/service.rs) (`CaService`): los casos 1, 6 y 7, el certificado
  propio de Hécate, las CRL y el retiro de las intermedias anteriores.
- [`identity/service.rs`](../../hecate/crates/application/src/identity/service.rs) (`IdentityService`): los casos 2 a 5
  y los recuentos de `status`.

## Fuera de alcance

- La Access Policy y la resolución de secretos por el canal autenticado, que siguen abiertas en DECISIONS.
- Que los componentes consulten el estado, las identidades hijas, `wrap` / `unwrap` y la firma de checkpoints.
- El desbloqueo desde Ténaro, el adapter de hardware y los avisos.
- El lado de cada componente: esta feature lo prueba con un componente de prueba.
- OpenTelemetry y Orthros.
